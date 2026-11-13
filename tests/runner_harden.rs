//! Process-level regression tests for the runner hardening (review findings 3, 4, 5, 16 and 21).
//! Each test runs the real `ounce-hb` binary against a stand-in ounce-core and failed on the code
//! before the fix. The stand-ins are shell scripts, so most of this file is unix only; the
//! closed-pipe test has a `.cmd` stand-in for Windows.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
use ounce_hb::{MAX_CAPTURED_BYTES, truncation_marker};

fn work_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ounce-hb-harden-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("tmp")).unwrap();
    dir
}

/// A header-only file that ounce-hb accepts as an NRO. The stand-ins never read it.
fn fake_nro(dir: &Path) -> PathBuf {
    let mut nro = vec![0u8; 0x80];
    nro[0x10..0x14].copy_from_slice(b"NRO0");
    let p = dir.join("demo.nro");
    fs::write(&p, nro).unwrap();
    p
}

/// A stand-in ounce-core: a `sh` script on unix, a `.cmd` file on Windows.
#[cfg(unix)]
fn fake_core(dir: &Path, name: &str, sh_body: &str, _cmd_body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join(name);
    fs::write(&bin, format!("#!/bin/sh\n{sh_body}\n")).unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

#[cfg(windows)]
fn fake_core(dir: &Path, name: &str, _sh_body: &str, cmd_body: &str) -> PathBuf {
    let bin = dir.join(format!("{name}.cmd"));
    fs::write(&bin, format!("@echo off\r\n{cmd_body}\r\n")).unwrap();
    bin
}

/// `ounce-hb run <nro>` with `core` as ounce-core and the temp directory inside `dir`.
fn hb(dir: &Path, core: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ounce-hb"));
    let tmp = dir.join("tmp");
    cmd.env("OUNCE_CORE_BIN", core)
        .env("TMPDIR", &tmp)
        .env("TMP", &tmp)
        .env("TEMP", &tmp)
        .arg("run")
        .arg(fake_nro(dir));
    cmd
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Finding 3: a consumer that stops reading stdout (`| head`, a crashed log collector) made
/// `print!` panic, so `ounce-hb run` exited 101 instead of the documented code.
#[test]
fn closed_stdout_pipe_keeps_the_documented_exit_code() {
    let dir = work_dir("epipe-out");
    let core = fake_core(
        &dir,
        "ounce-core-clean",
        "sleep 0.3\necho guest text\necho '[VM] Process exited gracefully (svcExitProcess).'\nexit 0",
        "ping -n 2 127.0.0.1 >nul\r\necho guest text\r\necho [VM] Process exited gracefully (svcExitProcess).\r\nexit /b 0",
    );
    let mut child = hb(&dir, &core)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Close the read end before ounce-hb writes anything: it only writes after ounce-core exits.
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{err}");
    assert!(!err.contains("panicked"), "{err}");
    let _ = fs::remove_dir_all(dir);
}

/// Finding 3, stderr side: the explanation goes to stderr; a closed stderr must not cost the code.
#[test]
fn closed_stderr_pipe_keeps_the_documented_exit_code() {
    let dir = work_dir("epipe-err");
    let core = fake_core(
        &dir,
        "ounce-core-fault",
        "sleep 0.3\necho '[VM Error] Memory access violation: Unmapped memory access at virtual address: 0x10' >&2\nexit 1",
        "ping -n 2 127.0.0.1 >nul\r\necho [VM Error] Memory access violation: Unmapped memory access at virtual address: 0x10 1>&2\r\nexit /b 1",
    );
    let mut child = hb(&dir, &core)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stderr.take());
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(11),
        "stdout:\n{}",
        text(&out.stdout)
    );
    let _ = fs::remove_dir_all(dir);
}

/// The command line of every running process, one per line.
#[cfg(unix)]
fn process_command_lines() -> String {
    text(
        &Command::new("ps")
            .args(["-axo", "command="])
            .output()
            .expect("ps must be available")
            .stdout,
    )
}

#[cfg(unix)]
fn process_running_with(needle: &str) -> bool {
    process_command_lines().lines().any(|l| l.contains(needle))
}

/// Waits up to `limit` for no process with `needle` in its command line to remain.
#[cfg(unix)]
fn assert_gone_within(needle: &str, limit: Duration) {
    let deadline = Instant::now() + limit;
    while process_running_with(needle) {
        assert!(
            Instant::now() < deadline,
            "a process with {needle} is still running:\n{}",
            process_command_lines()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Finding 4: the timeout killed only the direct child. A process it started kept the pipes open,
/// so ounce-hb waited for it (12 s in the review's reproduction, forever for a daemon). Now the
/// whole process group is killed and nothing of it remains.
#[cfg(unix)]
#[test]
fn timeout_kills_the_whole_process_group_and_returns_promptly() {
    let dir = work_dir("tree");
    let token = format!("ounce-hb-harden-tree-{}", std::process::id());
    // A grandchild (sh) and great-grandchild (sleep) that would outlive the direct child.
    let core = fake_core(
        &dir,
        "ounce-core-tree",
        &format!("sh -c 'sleep 30; echo late' {token} &\nexec sleep 100"),
        "",
    );
    let started = Instant::now();
    let out = hb(&dir, &core)
        .env("OUNCE_HB_TIMEOUT_SECS", "1")
        .output()
        .unwrap();
    let elapsed = started.elapsed();
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(4), "stderr:\n{err}");
    assert!(err.contains("(exit code 4)"), "{err}");
    assert!(elapsed < Duration::from_secs(10), "took {elapsed:?}");
    assert_gone_within(&token, Duration::from_secs(5));
    // The staged copy's directory is gone too (its path is in ounce-core's command line).
    let staged = dir.join("tmp").display().to_string();
    assert_gone_within(&staged, Duration::from_secs(5));
    assert_eq!(fs::read_dir(dir.join("tmp")).unwrap().count(), 0);
    let _ = fs::remove_dir_all(dir);
}

/// Finding 4 on Windows: a grandchild holding the pipes must not keep ounce-hb waiting after the
/// timeout. `taskkill /T` ends the tree.
#[cfg(windows)]
#[test]
fn timeout_kills_the_whole_process_tree_and_returns_promptly() {
    let dir = work_dir("tree");
    let core = fake_core(
        &dir,
        "ounce-core-tree",
        "",
        "start /b ping -n 31 127.0.0.1 >nul\r\nping -n 101 127.0.0.1 >nul",
    );
    let started = Instant::now();
    let out = hb(&dir, &core)
        .env("OUNCE_HB_TIMEOUT_SECS", "1")
        .output()
        .unwrap();
    let elapsed = started.elapsed();
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(4), "stderr:\n{err}");
    assert!(elapsed < Duration::from_secs(15), "took {elapsed:?}");
    let _ = fs::remove_dir_all(dir);
}

/// Finding 5: all output was buffered without a cap. Now each stream is cut to
/// `MAX_CAPTURED_BYTES`, the cut is marked in the output and noted after the explanation, and the
/// stop line at the end still decides the category.
#[cfg(unix)]
#[test]
fn long_output_is_capped_marked_and_still_classified() {
    let dir = work_dir("cap");
    let total = 3 * MAX_CAPTURED_BYTES;
    let core = fake_core(
        &dir,
        "ounce-core-spam",
        &format!(
            "head -c {total} /dev/zero | tr '\\0' x\necho\necho '[VM] Execution budget reached (7 instructions).'\nexit 1"
        ),
        "",
    );
    let out = hb(&dir, &core).output().unwrap();
    let stdout = text(&out.stdout);
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(16), "stderr:\n{err}");
    assert!(err.contains("within 7 instructions"), "{err}");
    assert!(
        stdout.len() < MAX_CAPTURED_BYTES + 1024,
        "stdout is {} bytes",
        stdout.len()
    );
    let stop = "\n[VM] Execution budget reached (7 instructions).\n";
    let dropped = (total + stop.len() - MAX_CAPTURED_BYTES) as u64;
    assert!(
        stdout.contains(&truncation_marker("stdout", dropped)),
        "no marker in stdout"
    );
    assert!(stdout.ends_with(stop));
    assert!(
        err.contains(&format!(
            "{dropped} bytes of stdout and 0 bytes of stderr were dropped"
        )),
        "{err}"
    );
    let _ = fs::remove_dir_all(dir);
}

/// Finding 16: SIGTERM or SIGINT to ounce-hb (a job runner's stop, Ctrl-C) ended ounce-hb but left
/// the ounce-core child running and the staged copy on disk. The death watch now removes the
/// directory and kills the child's group.
#[cfg(unix)]
#[test]
fn sigterm_and_sigint_kill_the_child_group_and_remove_the_staged_copy() {
    for signal in ["TERM", "INT"] {
        let dir = work_dir(&format!("signal-{signal}"));
        let token = format!("ounce-hb-harden-sig{signal}-{}", std::process::id());
        // The direct child becomes a sh that keeps `sleep` as a grandchild; both hold the pipes. (A
        // one-command `sh -c` would exec into `sleep` and lose the token from its command line.)
        let core = fake_core(
            &dir,
            "ounce-core-sleep",
            &format!("exec sh -c 'sleep 60; true' {token}"),
            "",
        );
        let mut child = hb(&dir, &core)
            .env("OUNCE_HB_TIMEOUT_SECS", "60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !process_running_with(&token) {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "the stand-in never started"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(fs::read_dir(dir.join("tmp")).unwrap().count(), 1);
        let killed = Command::new("kill")
            .args([&format!("-{signal}"), &child.id().to_string()])
            .status()
            .unwrap();
        assert!(killed.success());
        let status = child.wait().unwrap();
        assert!(!status.success());
        assert_gone_within(&token, Duration::from_secs(5));
        let deadline = Instant::now() + Duration::from_secs(5);
        while fs::read_dir(dir.join("tmp")).unwrap().count() > 0 {
            assert!(
                Instant::now() < deadline,
                "the staged directory was not removed after SIG{signal}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = fs::remove_dir_all(dir);
    }
}

/// Finding 21: an empty PATH entry resolved to the current directory, so an `ounce-core` there was
/// run. Now empty entries are skipped and the lookup reports nothing found (exit 3).
#[cfg(unix)]
#[test]
fn empty_path_entry_does_not_run_ounce_core_from_the_current_directory() {
    let dir = work_dir("emptypath");
    let marker = dir.join("was-run");
    fake_core(
        &dir,
        "ounce-core",
        &format!(
            "touch '{}'\necho '[VM] Process exited gracefully (svcExitProcess).'\nexit 0",
            marker.display()
        ),
        "",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .env_remove("OUNCE_CORE_BIN")
        .env("PATH", ":")
        .current_dir(&dir)
        .arg("run")
        .arg(fake_nro(&dir))
        .output()
        .unwrap();
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "stderr:\n{err}");
    assert!(
        !marker.exists(),
        "ounce-core in the current directory was run"
    );
    let _ = fs::remove_dir_all(dir);
}

/// Finding 21: a non-executable `ounce-core` earlier on PATH was taken and failed to start (exit 5)
/// instead of falling through to the executable one.
#[cfg(unix)]
#[test]
fn non_executable_file_on_path_is_passed_over() {
    use std::os::unix::fs::PermissionsExt;
    let dir = work_dir("nonexec");
    let first = dir.join("first");
    let second = dir.join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(first.join("ounce-core"), b"not a program").unwrap();
    fs::set_permissions(first.join("ounce-core"), fs::Permissions::from_mode(0o644)).unwrap();
    fake_core(
        &second,
        "ounce-core",
        "echo '[VM] Process exited gracefully (svcExitProcess).'\nexit 0",
        "",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .env_remove("OUNCE_CORE_BIN")
        .env("PATH", std::env::join_paths([&first, &second]).unwrap())
        .arg("run")
        .arg(fake_nro(&dir))
        .output()
        .unwrap();
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{err}");
    let _ = fs::remove_dir_all(dir);
}
