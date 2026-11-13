//! The guest-argument length limit (ounce-core #418). An argument set just under the limit must run
//! through the real ounce-core. One just over must be refused, and no ounce-core process may start.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use ounce_hb::{CORE_BIN_ENV, MAX_GUEST_ARGS_BYTES, guest_args_len, locate_core};

const MOV_W0_0: u32 = 0x5280_0000; // mov w0, #0
const SVC_EXIT_PROCESS: u32 = 0xD400_00E1; // svc #7 (svcExitProcess)

fn nro(words: &[u32]) -> Vec<u8> {
    let text: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let total = 0x80 + text.len();
    let mut b = vec![0u8; total];
    b[0x00..0x04].copy_from_slice(&0x1400_0020u32.to_le_bytes()); // b #0x80
    b[0x10..0x14].copy_from_slice(b"NRO0");
    b[0x18..0x1C].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x24..0x28].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x80..].copy_from_slice(&text);
    b
}

fn work_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ounce-hb-args-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("tmp")).unwrap();
    dir
}

/// One argument whose length makes the combined total exactly `total` bytes, as the guard counts it.
fn arg_with_total(total: usize) -> String {
    "a".repeat(total - 1)
}

#[test]
fn argument_just_under_the_limit_runs_for_real() {
    let core = match locate_core() {
        Ok(bin) => bin,
        Err(e) => {
            assert!(
                std::env::var_os(CORE_BIN_ENV).is_none(),
                "{CORE_BIN_ENV} is set but unusable: {e}"
            );
            eprintln!(
                "SKIPPED argument_just_under_the_limit_runs_for_real: no ounce-core binary ({e})"
            );
            return;
        }
    };
    let dir = work_dir("under");
    let nro_path = dir.join("program.nro");
    fs::write(&nro_path, nro(&[MOV_W0_0, SVC_EXIT_PROCESS])).unwrap();
    let arg = arg_with_total(MAX_GUEST_ARGS_BYTES - 1);
    assert_eq!(
        guest_args_len(std::slice::from_ref(&arg)),
        MAX_GUEST_ARGS_BYTES - 1
    );

    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .env(CORE_BIN_ENV, &core)
        .env("TMPDIR", dir.join("tmp"))
        .arg("run")
        .arg(&nro_path)
        .arg("--")
        .arg(&arg)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "stderr:\n{err}");
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("[VM] Process exited gracefully (svcExitProcess)."),
        "the program did not reach its exit"
    );
    let _ = fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn argument_over_the_limit_is_refused_before_ounce_core_starts() {
    use std::os::unix::fs::PermissionsExt;
    let dir = work_dir("over");
    let nro_path = dir.join("program.nro");
    fs::write(&nro_path, nro(&[MOV_W0_0, SVC_EXIT_PROCESS])).unwrap();

    // A stand-in ounce-core that leaves a marker file if it is ever started.
    let marker = dir.join("ounce-core-was-started");
    let core = dir.join("ounce-core-marker");
    fs::write(
        &core,
        format!("#!/bin/sh\ntouch '{}'\nexit 0\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&core, fs::Permissions::from_mode(0o755)).unwrap();

    let arg = arg_with_total(MAX_GUEST_ARGS_BYTES + 1);
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .env(CORE_BIN_ENV, &core)
        .env("TMPDIR", dir.join("tmp"))
        .arg("run")
        .arg(&nro_path)
        .arg("--")
        .arg(&arg)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(2), "stderr:\n{err}");
    assert!(err.contains("(exit code 2)"), "{err}");
    assert!(
        err.contains(&format!("{}-byte limit", MAX_GUEST_ARGS_BYTES)),
        "{err}"
    );
    assert!(err.contains("ounce-core #418"), "{err}");
    assert!(
        !marker.exists(),
        "ounce-core was started for an over-limit request"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn limit_is_counted_per_argument_with_its_terminator() {
    // Many short arguments count the same as one long one: bytes plus one NUL each.
    let args: Vec<String> = (0..4).map(|_| "ab".to_string()).collect();
    assert_eq!(guest_args_len(&args), 12);
    assert_eq!(guest_args_len(&[]), 0);
}
