#![forbid(unsafe_code)]
#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempWorkspace(PathBuf);

impl TempWorkspace {
    fn new(prefix: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ounce-hb-{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut nro = vec![0u8; 0x80];
        nro[0x10..0x14].copy_from_slice(b"NRO0");
        fs::write(dir.join("app.nro"), nro).unwrap();
        Self(dir)
    }

    #[cfg(unix)]
    fn write_script(&self, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = self.0.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn hb(ws: &TempWorkspace) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ounce-hb"));
    c.current_dir(&ws.0);
    c
}

#[test]
fn timeout_flag_kills_a_slow_engine_with_exit_4() {
    let ws = TempWorkspace::new("timeout-flag");
    let core = ws.write_script("core", "sleep 30");
    let started = std::time::Instant::now();
    let out = hb(&ws)
        .args(["run", "--timeout", "0.5", "app.nro"])
        .env("OUNCE_CORE_BIN", &core)
        .env_remove("OUNCE_HB_TIMEOUT_SECS")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(started.elapsed().as_secs() < 15, "did not stop promptly");
}

#[test]
fn timeout_flag_overrides_the_environment_variable() {
    let ws = TempWorkspace::new("timeout-env");
    let core = ws.write_script("core", "sleep 30");
    let out = hb(&ws)
        .args(["run", "--timeout", "0.5", "app.nro"])
        .env("OUNCE_CORE_BIN", &core)
        .env("OUNCE_HB_TIMEOUT_SECS", "600")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
}

#[test]
fn bad_timeout_values_are_refused_with_exit_2() {
    let ws = TempWorkspace::new("timeout-bad");
    for bad in ["0", "-1", "abc", "999999"] {
        let out = hb(&ws)
            .args(["run", "--timeout", bad, "app.nro"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "value {bad}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("--timeout"),
            "value {bad}"
        );
    }
    let out = hb(&ws).args(["run", "--timeout"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn json_prints_one_object_and_nothing_else_on_stdout() {
    let ws = TempWorkspace::new("json-ok");
    let core = ws.write_script(
        "core",
        "echo 'hello \"guest\"'; echo '[VM] Process exited gracefully (svcExitProcess).'; exit 0",
    );
    let out = hb(&ws)
        .args(["run", "--json", "app.nro"])
        .env("OUNCE_CORE_BIN", &core)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    assert!(
        stdout.starts_with("{\"schema\": \"ounce.hb.run.v1\""),
        "{stdout}"
    );
    assert!(stdout.contains("\"exit_code\": 0"), "{stdout}");
    assert!(stdout.contains("\"stop\": \"clean_exit\""), "{stdout}");
    assert!(stdout.contains("hello \\\"guest\\\""), "{stdout}");
    assert!(stdout.contains("\"truncated\": false"), "{stdout}");
}

#[test]
fn json_reports_failures_and_timeouts_with_the_same_exit_code() {
    let ws = TempWorkspace::new("json-fail");
    let core = ws.write_script("core", "sleep 30");
    let out = hb(&ws)
        .args(["run", "--json", "--timeout", "0.5", "app.nro"])
        .env("OUNCE_CORE_BIN", &core)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"exit_code\": 4"), "{stdout}");
    assert!(stdout.contains("\"stop\": \"timeout\""), "{stdout}");
    assert!(stdout.contains("\"timeout_secs\": 0.5"), "{stdout}");

    let out = hb(&ws)
        .args(["run", "--json", "missing.nro"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"stop\": \"refused\""), "{stdout}");
}

#[test]
fn json_with_auto_is_refused_rather_than_mixed_output() {
    let ws = TempWorkspace::new("json-auto");
    let out = hb(&ws)
        .args(["run", "--json", "--auto", "app.nro"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

#[test]
fn doctor_reports_homebrew_toolchain_in_json() {
    let ws = TempWorkspace::new("doctor-tc");
    let root = ws.0.join("dkp");
    fs::create_dir_all(root.join("devkitA64/bin")).unwrap();
    fs::write(root.join("devkitA64/bin/aarch64-none-elf-gcc"), "").unwrap();
    let bin = ws.0.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("cargo-nx"), "").unwrap();

    let out = hb(&ws)
        .args(["doctor", "--json"])
        .env("DEVKITPRO", &root)
        .env("PATH", &bin)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"DEVKITPRO\": {\"ok\": true"), "{stdout}");
    assert!(stdout.contains("\"devkitA64\": {\"ok\": true"), "{stdout}");
    assert!(stdout.contains("\"cargo-nx\": {\"ok\": true"), "{stdout}");

    let out = hb(&ws)
        .args(["doctor", "--json"])
        .env_remove("DEVKITPRO")
        .env("PATH", ws.0.join("empty"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"DEVKITPRO\": {\"ok\": false, \"detail\": \"not set\"}"),
        "{stdout}"
    );
    assert!(stdout.contains("\"cargo-nx\": {\"ok\": false"), "{stdout}");
}

#[test]
fn doctor_text_names_each_missing_tool_with_a_fix() {
    let ws = TempWorkspace::new("doctor-text");
    let out = hb(&ws)
        .arg("doctor")
        .env_remove("DEVKITPRO")
        .env("PATH", ws.0.join("empty"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    for needle in [
        "DEVKITPRO  MISSING",
        "devkitA64  MISSING",
        "cargo-nx   MISSING",
        "cargo install cargo-nx",
    ] {
        assert!(stdout.contains(needle), "missing {needle:?} in {stdout}");
    }
}
