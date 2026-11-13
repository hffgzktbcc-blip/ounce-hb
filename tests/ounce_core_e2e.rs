#![cfg(unix)]

//! End-to-end checks of `ounce-hb run` against the real ounce-core CLI: one run per exit code that a
//! real program can reach, plus the wall-clock timeout with a check that the child is killed.
//!
//! The programs are small NROs built here (not committed). They use the same layout as the synthetic
//! helper in tests/ounce_core_run.rs. Tests that need ounce-core skip with a SKIPPED line when
//! `OUNCE_CORE_BIN` and PATH give no binary, and fail if `OUNCE_CORE_BIN` is set but unusable. CI sets
//! `OUNCE_CORE_BIN` to the pinned v0.8.0 release (.github/workflows/ounce-core-e2e.yml).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

use ounce_hb::{CORE_BIN_ENV, locate_core};

const MOV_W0_0: u32 = 0x5280_0000; // mov w0, #0
const SVC_EXIT_PROCESS: u32 = 0xD400_00E1; // svc #7 (svcExitProcess)
const SPIN: u32 = 0x1400_0000; // b .

/// An NRO0 whose code is `words` at 0x80. `text_size_override` writes a text size that runs past the
/// end of the file, which ounce-core rejects at load time.
fn nro(words: &[u32], text_size_override: Option<u32>) -> Vec<u8> {
    let text: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let total = 0x80 + text.len();
    let mut b = vec![0u8; total];
    b[0x00..0x04].copy_from_slice(&0x1400_0020u32.to_le_bytes()); // b #0x80
    b[0x10..0x14].copy_from_slice(b"NRO0");
    b[0x18..0x1C].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x24..0x28].copy_from_slice(&(text_size_override.unwrap_or(total as u32)).to_le_bytes());
    b[0x80..].copy_from_slice(&text);
    b
}

/// A fresh directory for one test. ounce-hb stages its private copy under TMPDIR, so the directory
/// name appears in the ounce-core command line, which the kill check relies on.
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

/// Runs `ounce-hb run <nro> [extra]` with ounce-core at `core` and TMPDIR inside `dir`.
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
        .env("TMPDIR", dir.join("tmp"))
        .arg("run")
        .arg(&nro_path)
        .args(extra);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
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
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("[VM] Process exited gracefully (svcExitProcess)."));
    assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
    let _ = fs::remove_dir_all(dir);
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

/// True when a running process's command line mentions `needle`.
fn process_running_with(needle: &str) -> bool {
    let ps = Command::new("ps")
        .args(["-axo", "command="])
        .output()
        .expect("ps must be available");
    text(&ps.stdout).lines().any(|line| line.contains(needle))
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
    use std::os::unix::fs::PermissionsExt;
    let dir = work_dir("hostio");
    // A regular file without the execute bit: found, but the spawn fails.
    let not_exec = dir.join("ounce-core-not-executable");
    fs::write(&not_exec, b"not a program").unwrap();
    fs::set_permissions(&not_exec, fs::Permissions::from_mode(0o644)).unwrap();
    let out = run_hb(
        &dir,
        &nro(&[MOV_W0_0, SVC_EXIT_PROCESS], None),
        &not_exec,
        &[],
        &[],
    );
    expect_stop_refused(&out, 5, "could not run ounce-core");
    let _ = fs::remove_dir_all(dir);
}

/// For refusals and host errors: the exit code and phrase, with no reproduce line required.
fn expect_stop_refused(out: &Output, code: i32, phrase: &str) {
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(code), "stderr:\n{err}");
    assert!(err.contains(phrase), "stderr:\n{err}");
}
