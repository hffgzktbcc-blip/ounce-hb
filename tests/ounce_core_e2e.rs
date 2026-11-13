//! End-to-end checks of `ounce-hb run` against the real ounce-core CLI: one run per exit code that a
//! real program can reach, plus the wall-clock timeout with a check that the child is killed.
//!
//! The programs are small NROs built here (not committed). They use the same layout as the synthetic
//! helper in tests/ounce_core_run.rs. Tests that need ounce-core skip with a SKIPPED line when
//! `OUNCE_CORE_BIN` and PATH give no binary, and fail if `OUNCE_CORE_BIN` is set but unusable. CI sets
//! `OUNCE_CORE_BIN` to the pinned v0.8.1 release (.github/workflows/ounce-core-e2e.yml) on Linux,
//! macOS and Windows. The only platform-specific code is `cfg`-gated: how the process list is read
//! and how a process is killed (`ps`/`pkill` on unix, PowerShell on Windows), the signal wording in
//! the SIGKILL test, and the file used for the "cannot be started" test.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

use ounce_hb::{CORE_BIN_ENV, locate_core};

const MOV_W0_0: u32 = 0x5280_0000; // mov w0, #0
const SVC_EXIT_PROCESS: u32 = 0xD400_00E1; // svc #7 (svcExitProcess)
const SVC_OUTPUT_DEBUG_STRING: u32 = 0xD400_04E1; // svc #0x27 (x0 = address, x1 = length)
const SVC_BREAK: u32 = 0xD400_04C1; // svc #0x26
const SVC_UNHANDLED: u32 = 0xD400_0FC1; // svc #0x7E
const LDR_X0_X1: u32 = 0xF940_0020; // ldr x0, [x1]
const ADD_X1_SP_0: u32 = 0x9100_03E1; // add x1, sp, #0
const MOV_X2_4: u32 = 0xD280_0082; // mov x2, #4
const SPIN: u32 = 0x1400_0000; // b .

/// `adr x0, .+offset`.
fn adr_x0(offset: u32) -> u32 {
    0x1000_0000 | ((offset & 3) << 29) | (((offset >> 2) & 0x7FFFF) << 5)
}

/// `mov x1, #imm` (imm below 65536).
fn mov_x1(imm: u32) -> u32 {
    0xD280_0001 | (imm << 5)
}

/// An NRO0 whose code is `words` at 0x80. `text_size_override` writes a text size that runs past the
/// end of the file, which ounce-core rejects at load time.
fn nro(words: &[u32], text_size_override: Option<u32>) -> Vec<u8> {
    nro_with_data(words, b"", text_size_override)
}

/// A program that prints `message` with svcOutputDebugString, then runs `then`. The message sits
/// right after the code; this is the layout of the spoof programs in
/// tests/fixtures/ounce-core-output/make_nros.py, byte for byte.
fn spoof(message: &[u8], then: &[u32]) -> Vec<u8> {
    let mut words = vec![0, mov_x1(message.len() as u32), SVC_OUTPUT_DEBUG_STRING];
    words.extend_from_slice(then);
    words[0] = adr_x0(4 * words.len() as u32);
    nro_with_data(&words, message, None)
}

/// Like [`nro`], with `data` after the code words.
fn nro_with_data(words: &[u32], data: &[u8], text_size_override: Option<u32>) -> Vec<u8> {
    let mut text: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    text.extend_from_slice(data);
    let total = 0x80 + text.len();
    let mut b = vec![0u8; total];
    b[0x00..0x04].copy_from_slice(&0x1400_0020u32.to_le_bytes()); // b #0x80
    b[0x10..0x14].copy_from_slice(b"NRO0");
    b[0x18..0x1C].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x24..0x28].copy_from_slice(&(text_size_override.unwrap_or(total as u32)).to_le_bytes());
    b[0x80..].copy_from_slice(&text);
    b
}

/// A fresh directory for one test. ounce-hb stages its private copy under the temp directory, so the
/// directory name appears in the ounce-core command line, which the kill check relies on. The temp
/// directory comes from TMPDIR on unix and from TMP or TEMP on Windows, so all three are set.
/// `temp_dir_env` is the single place that sets them.
fn work_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ounce-hb-e2e-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("tmp")).unwrap();
    dir
}

/// The ounce-core binary, or `None` after printing a SKIPPED line when no binary is configured.
fn core_bin(test: &str) -> Option<PathBuf> {
    match locate_core() {
        Ok(bin) => Some(bin),
        Err(e) => {
            assert!(
                std::env::var_os(CORE_BIN_ENV).is_none(),
                "{CORE_BIN_ENV} is set but unusable: {e}"
            );
            eprintln!("SKIPPED {test}: no ounce-core binary available ({e})");
            None
        }
    }
}

/// Runs `ounce-hb run <nro> [extra]` with ounce-core at `core` and the temp directory inside `dir`.
fn run_hb(
    dir: &Path,
    nro_bytes: &[u8],
    core: &Path,
    env: &[(&str, &str)],
    extra: &[&str],
) -> Output {
    let nro_path = dir.join("program.nro");
    fs::write(&nro_path, nro_bytes).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ounce-hb"));
    cmd.env(CORE_BIN_ENV, core)
        .envs(temp_dir_env(&dir.join("tmp")))
        .arg("run")
        .arg(&nro_path)
        .args(extra);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

/// The environment variables that point the temp directory at `tmp`, on every platform.
fn temp_dir_env(tmp: &Path) -> [(&'static str, PathBuf); 3] {
    [
        ("TMPDIR", tmp.to_path_buf()),
        ("TMP", tmp.to_path_buf()),
        ("TEMP", tmp.to_path_buf()),
    ]
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// Asserts the exit code and that the explanation names the category.
fn expect_stop(out: &Output, code: i32, phrase: &str) {
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(code), "stderr:\n{err}");
    assert!(err.contains(phrase), "stderr:\n{err}");
    assert!(
        err.contains(&format!("(exit code {code})")),
        "stderr:\n{err}"
    );
    assert!(
        err.contains("To reproduce: ounce-hb run "),
        "stderr:\n{err}"
    );
}

#[test]
fn clean_guest_exit_is_zero_with_no_explanation() {
    let Some(core) = core_bin("clean_guest_exit_is_zero_with_no_explanation") else {
        return;
    };
    let dir = work_dir("clean");
    let out = run_hb(
        &dir,
        &nro(&[MOV_W0_0, SVC_EXIT_PROCESS], None),
        &core,
        &[],
        &[],
    );
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{err}");
    assert!(text(&out.stdout).contains("[VM] Process exited gracefully (svcExitProcess)."));
    assert!(
        !err.contains("ounce-hb:"),
        "no diagnosis for a clean exit:\n{err}"
    );
    // ounce-core's host audio layer prints device errors on machines with no sound device (CI runners
    // do this), and ounce-core has no switch to skip it. Only those known lines are tolerated; any other
    // stderr text fails the test.
    for line in err.lines().filter(|l| !l.trim().is_empty()) {
        assert!(
            is_host_audio_noise(line),
            "unexpected stderr line from ounce-core: {line:?}\n{err}"
        );
    }
    let _ = fs::remove_dir_all(dir);
}

/// stderr lines ounce-core prints when the host has no usable audio device. Captured from CI runners;
/// see tests/fixtures/ounce-core-output/NOTES.md.
fn is_host_audio_noise(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("ALSA lib ") || line.starts_with("[HostAudioSink] ")
}

#[test]
fn captured_audio_noise_is_the_only_stderr_tolerated() {
    for capture in [
        include_str!("fixtures/ounce-core-output/clean_audio_noise_linux.stderr"),
        include_str!("fixtures/ounce-core-output/clean_audio_noise_macos.stderr"),
    ] {
        assert!(capture.lines().count() > 0);
        for line in capture.lines() {
            assert!(is_host_audio_noise(line), "{line:?}");
        }
    }
    assert!(!is_host_audio_noise(
        "[VM Error] Unimplemented instruction 0x0 at PC: 0x80000000"
    ));
    assert!(!is_host_audio_noise("[Loader Error] anything"));
}

#[test]
fn unimplemented_instruction_exits_10() {
    let Some(core) = core_bin("unimplemented_instruction_exits_10") else {
        return;
    };
    let dir = work_dir("unimpl");
    let out = run_hb(&dir, &nro(&[0x0000_0000], None), &core, &[], &[]);
    expect_stop(&out, 10, "0x00000000 at address 0x0000000080000080");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn memory_fault_exits_11() {
    let Some(core) = core_bin("memory_fault_exits_11") else {
        return;
    };
    let dir = work_dir("memfault");
    // ldr x0, [x1] with x1 unmapped.
    let out = run_hb(&dir, &nro(&[0xF940_0020], None), &core, &[], &[]);
    expect_stop(&out, 11, "memory access that Ounce Core rejected");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cpu_error_from_brk_exits_12() {
    let Some(core) = core_bin("cpu_error_from_brk_exits_12") else {
        return;
    };
    let dir = work_dir("brk");
    // brk #0
    let out = run_hb(&dir, &nro(&[0xD420_0000], None), &core, &[], &[]);
    expect_stop(&out, 12, "BRK #0x0 breakpoint hit");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn svc_break_exits_13() {
    let Some(core) = core_bin("svc_break_exits_13") else {
        return;
    };
    let dir = work_dir("svcbreak");
    // add x1, sp, #0 ; mov x2, #4 ; svc #0x26 (svcBreak)
    let out = run_hb(
        &dir,
        &nro(&[0x9100_03E1, 0xD280_0082, 0xD400_04C1], None),
        &core,
        &[],
        &[],
    );
    expect_stop(&out, 13, "svcBreak");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn unhandled_svc_exits_14() {
    let Some(core) = core_bin("unhandled_svc_exits_14") else {
        return;
    };
    let dir = work_dir("unhandled");
    // svc #0x7E
    let out = run_hb(&dir, &nro(&[0xD400_0FC1], None), &core, &[], &[]);
    expect_stop(&out, 14, "Ounce Core does not provide");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn exhausted_budget_exits_16() {
    let Some(core) = core_bin("exhausted_budget_exits_16") else {
        return;
    };
    let dir = work_dir("budget");
    let out = run_hb(
        &dir,
        &nro(&[SPIN], None),
        &core,
        &[("OUNCE_NRO_BUDGET", "1000")],
        &[],
    );
    expect_stop(&out, 16, "ran out of instructions");
    assert!(
        text(&out.stderr).contains("within 1000 instructions"),
        "{}",
        text(&out.stderr)
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn loader_error_exits_17() {
    let Some(core) = core_bin("loader_error_exits_17") else {
        return;
    };
    let dir = work_dir("loader");
    let out = run_hb(
        &dir,
        &nro(&[MOV_W0_0, SVC_EXIT_PROCESS], Some(0x10000)),
        &core,
        &[],
        &[],
    );
    expect_stop(&out, 17, "could not load the program");
    let _ = fs::remove_dir_all(dir);
}

// ---- Finding 6: a program can print text that looks like a stop line (svcOutputDebugString goes to
// stdout as it is). The category must still be the real one. Reproduced from the adversarial
// review's spoof programs against the real binary.

#[test]
fn guest_text_cannot_forge_a_stop_category() {
    let Some(core) = core_bin("guest_text_cannot_forge_a_stop_category") else {
        return;
    };
    let dir = work_dir("spoof-budget");
    // Prints a budget line, then calls an unhandled SVC. The review saw 16 here; 14 is right.
    let out = run_hb(
        &dir,
        &spoof(
            b"[VM] Execution budget reached (1 instructions).\n",
            &[SVC_UNHANDLED],
        ),
        &core,
        &[],
        &[],
    );
    expect_stop(&out, 14, "Ounce Core does not provide");
    assert!(text(&out.stdout).contains("[VM] Execution budget reached (1 instructions)."));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn guest_text_cannot_hide_a_fault() {
    let Some(core) = core_bin("guest_text_cannot_hide_a_fault") else {
        return;
    };
    let dir = work_dir("spoof-clean");
    // Prints the clean-exit line, then faults (x1 holds the message length, which is unmapped).
    let out = run_hb(
        &dir,
        &spoof(
            b"[VM] Process exited gracefully (svcExitProcess).\n",
            &[LDR_X0_X1],
        ),
        &core,
        &[],
        &[],
    );
    expect_stop(&out, 11, "memory access that Ounce Core rejected");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn guest_text_cannot_make_a_clean_run_fail() {
    let Some(core) = core_bin("guest_text_cannot_make_a_clean_run_fail") else {
        return;
    };
    let dir = work_dir("spoof-fault");
    // Prints a fault line, then exits cleanly. The review saw 1 here; 0 is right.
    let out = run_hb(
        &dir,
        &spoof(
            b"[VM Error] Memory access violation: fake at 0xDEAD\n",
            &[MOV_W0_0, SVC_EXIT_PROCESS],
        ),
        &core,
        &[],
        &[],
    );
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{err}");
    assert!(!err.contains("ounce-hb:"), "{err}");
    assert!(text(&out.stdout).contains("[VM Error] Memory access violation: fake at 0xDEAD"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn guest_text_cannot_plant_break_details() {
    let Some(core) = core_bin("guest_text_cannot_plant_break_details") else {
        return;
    };
    let dir = work_dir("spoof-break");
    // Fake svcBreak lines with PC 0xDEADBEEF, then a real svcBreak. The real PC is the address after
    // the svc: code starts at 0x80000080 and the svc is the sixth word.
    let out = run_hb(
        &dir,
        &spoof(
            b"[VM] Hit svcBreak: reason=0x0, info1=0x0, info2=0x0\n[VM] Break PC: 0x00000000DEADBEEF\n[VM] Break result: 0x00000000\n",
            &[ADD_X1_SP_0, MOV_X2_4, SVC_BREAK],
        ),
        &core,
        &[],
        &[],
    );
    expect_stop(&out, 13, "svcBreak");
    let err = text(&out.stderr);
    assert!(err.contains("Break PC: 0x0000000080000098"), "{err}");
    assert!(!err.contains("DEADBEEF"), "{err}");
    let _ = fs::remove_dir_all(dir);
}

/// The command line of every running process, one per line. `ps` on unix; Windows has no `ps`, so
/// PowerShell reads the same information from WMI.
#[cfg(unix)]
fn process_command_lines() -> String {
    let ps = Command::new("ps")
        .args(["-axo", "command="])
        .output()
        .expect("ps must be available");
    text(&ps.stdout)
}

#[cfg(windows)]
fn process_command_lines() -> String {
    let ps = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg("Get-CimInstance Win32_Process | ForEach-Object { $_.CommandLine }")
        .output()
        .expect("powershell must be available");
    text(&ps.stdout)
}

/// True when a running process's command line mentions `needle`.
fn process_running_with(needle: &str) -> bool {
    process_command_lines()
        .lines()
        .any(|line| line.contains(needle))
}

/// Kills every process whose command line mentions `needle`: SIGKILL on unix, TerminateProcess on
/// Windows. True when at least one process was found and killed.
#[cfg(unix)]
fn kill_processes_with(needle: &str) -> bool {
    Command::new("pkill")
        .args(["-KILL", "-f", needle])
        .status()
        .unwrap()
        .success()
}

#[cfg(windows)]
fn kill_processes_with(needle: &str) -> bool {
    // The needle goes in an environment variable so that no PowerShell quoting is needed.
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg("Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine.Contains($env:OUNCE_HB_E2E_NEEDLE) } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force; $_.ProcessId }")
        .env("OUNCE_HB_E2E_NEEDLE", needle)
        .output()
        .unwrap();
    out.status.success() && !text(&out.stdout).trim().is_empty()
}

#[test]
fn wall_clock_timeout_exits_4_and_kills_ounce_core() {
    let Some(core) = core_bin("wall_clock_timeout_exits_4_and_kills_ounce_core") else {
        return;
    };
    let dir = work_dir("timeout");
    // The budget is far beyond what fits in the limit, so only the wall-clock limit can stop the run.
    let started = Instant::now();
    let out = run_hb(
        &dir,
        &nro(&[SPIN], None),
        &core,
        &[
            ("OUNCE_NRO_BUDGET", "100000000000"),
            ("OUNCE_HB_TIMEOUT_SECS", "2"),
        ],
        &[],
    );
    let elapsed = started.elapsed();
    expect_stop(&out, 4, "did not finish in time");
    assert!(elapsed.as_secs() < 60, "took {elapsed:?}");
    // The staged copy's path is in ounce-core's command line; it must be gone with the process.
    let needle = dir.join("tmp").display().to_string();
    assert!(
        !process_running_with(&needle),
        "an ounce-core process is still running for {needle}"
    );
    let _ = fs::remove_dir_all(dir);
}

/// Code 1. ounce-core prints nothing listed when the OS kills it: this is the one way a real run ends
/// with no recognised line. The test kills the real ounce-core process from outside, as the
/// out-of-memory killer would (SIGKILL on unix, TerminateProcess on Windows), and checks that
/// ounce-hb reports it as unrecognised with exit 1.
#[test]
fn signal_killed_ounce_core_is_unrecognised_exit_1() {
    let Some(core) = core_bin("signal_killed_ounce_core_is_unrecognised_exit_1") else {
        return;
    };
    let dir = work_dir("killed");
    let nro_path = dir.join("program.nro");
    fs::write(&nro_path, nro(&[SPIN], None)).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .env(CORE_BIN_ENV, &core)
        .envs(temp_dir_env(&dir.join("tmp")))
        .env("OUNCE_NRO_BUDGET", "100000000000")
        .env("OUNCE_HB_TIMEOUT_SECS", "60")
        .arg("run")
        .arg(&nro_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let needle = dir.join("tmp").display().to_string();
    // Wait until the real ounce-core process exists, then kill it by its command line.
    let started = Instant::now();
    while !process_running_with(&needle) {
        assert!(
            started.elapsed().as_secs() < 30,
            "ounce-core never started for {needle}"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        kill_processes_with(&needle),
        "no ounce-core process was found to kill for {needle}"
    );
    let out = child.wait_with_output().unwrap();
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{err}");
    assert!(err.contains("does not recognise"), "{err}");
    // Only unix reports a signal. Windows has none: Stop-Process ends the process with exit code
    // 0xFFFFFFFF, which Rust reads as -1, and ounce-hb prints that code unchanged. Observed in the
    // ounce-core-e2e workflow on windows-latest. It is the code of this kill method, not of a
    // real out-of-memory kill, which has not been run on Windows.
    #[cfg(unix)]
    assert!(err.contains("killed by a signal"), "{err}");
    #[cfg(windows)]
    assert!(err.contains("ounce-core exit code: -1."), "{err}");
    assert!(err.contains("To reproduce: ounce-hb run "), "{err}");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn refused_guest_argument_exits_2_without_core() {
    let dir = work_dir("refused");
    // The guest-argument refusal happens before ounce-core starts, so any existing file will do here.
    let existing = std::env::current_exe().unwrap();
    let out = run_hb(
        &dir,
        &nro(&[MOV_W0_0, SVC_EXIT_PROCESS], None),
        &existing,
        &[],
        &["--", "-gui"],
    );
    expect_stop_refused(&out, 2, "starts with '-'");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn invalid_timeout_override_exits_2_without_core() {
    let dir = work_dir("badtimeout");
    for bad in ["0", "-1", "nan", "1e9", "soon"] {
        let out = run_hb(
            &dir,
            &nro(&[MOV_W0_0, SVC_EXIT_PROCESS], None),
            Path::new("/nonexistent/ounce-core"),
            &[("OUNCE_HB_TIMEOUT_SECS", bad)],
            &[],
        );
        expect_stop_refused(&out, 2, "OUNCE_HB_TIMEOUT_SECS must be");
    }
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn ounce_core_that_cannot_be_started_exits_5() {
    let dir = work_dir("hostio");
    let not_exec = not_a_program(&dir);
    let out = run_hb(
        &dir,
        &nro(&[MOV_W0_0, SVC_EXIT_PROCESS], None),
        &not_exec,
        &[],
        &[],
    );
    expect_stop_refused(&out, 5, "could not run backend");
    let _ = fs::remove_dir_all(dir);
}

/// A file that `find_core` accepts but the OS cannot run, so the spawn fails. On unix it has no
/// execute bit. On Windows it is a `.exe` whose contents are not a program.
#[cfg(unix)]
fn not_a_program(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("ounce-core-not-executable");
    fs::write(&path, b"not a program").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    path
}

#[cfg(windows)]
fn not_a_program(dir: &Path) -> PathBuf {
    let path = dir.join("ounce-core-not-executable.exe");
    fs::write(&path, b"not a program").unwrap();
    path
}

/// For refusals and host errors: the exit code and phrase, with no reproduce line required.
fn expect_stop_refused(out: &Output, code: i32, phrase: &str) {
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(code), "stderr:\n{err}");
    assert!(err.contains(phrase), "stderr:\n{err}");
}
