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

#[cfg(unix)]
#[test]
fn doctor_reports_discovered_engines_in_json() {
    let ws = TempWorkspace::new("doctor");
    let core = ws.write_script(
        "fake-core",
        "if [ \"$1\" = \"--version\" ]; then echo 'ounce-core 0.8.2'; exit 0; fi",
    );
    let rt = ws.write_script(
        "fake-rt",
        "if [ \"$1\" = \"--version\" ]; then echo 'ounce-runtime 0.5.0'; exit 0; fi",
    );

    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .args(["doctor", "--json"])
        .env("OUNCE_CORE_BIN", &core)
        .env("OUNCE_RUNTIME_BIN", &rt)
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"schema\": \"ounce.doctor.v1\""),
        "{stdout}"
    );
    assert!(
        stdout.contains("\"version\": \"ounce-core 0.8.2\""),
        "{stdout}"
    );
    assert!(
        stdout.contains("\"version\": \"ounce-runtime 0.5.0\""),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn auto_escalates_on_runtime_fault_and_writes_repro_on_divergence() {
    let ws = TempWorkspace::new("auto-diverge");
    let rt = ws.write_script(
        "rt",
        r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-runtime","engine_version":"0.5.0","tier":"tier4","silent_fallback":false,"stop_kind":"cpu_error","exit_code":12,"pc":2147483648,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":null,"insns_executed":100,"message":"jit fault"}
EOF
exit 1"#,
    );
    let core = ws.write_script(
        "core",
        r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-core","engine_version":"0.8.2","tier":"interpreter","silent_fallback":false,"stop_kind":"clean_exit","exit_code":0,"pc":null,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":null,"insns_executed":150,"message":null}
EOF
echo "core ok"
exit 0"#,
    );

    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .current_dir(&ws.0)
        .args(["run", "--auto", "app.nro"])
        .env("OUNCE_CORE_BIN", &core)
        .env("OUNCE_RUNTIME_BIN", &rt)
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("attribution = Runtime Divergence"),
        "{stderr}"
    );
    let repro = fs::read_to_string(ws.0.join("ounce-repro.json")).expect("repro.json written");
    assert!(repro.contains("\"schema\": \"ounce.repro.v1\""), "{repro}");
    assert!(repro.contains("\"stop_kind\": \"cpu_error\""), "{repro}");
    assert!(repro.contains("\"stop_kind\": \"clean_exit\""), "{repro}");
}

#[cfg(unix)]
#[test]
fn auto_attributes_guest_bug_when_both_fail_identically() {
    let ws = TempWorkspace::new("auto-guest-bug");
    let report = r#"{"schema":"ounce.run.v1","engine":"engine","engine_version":"1.0","tier":"interpreter","silent_fallback":false,"stop_kind":"svc_break","exit_code":13,"pc":2147483660,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":305419896,"insns_executed":40,"message":null}"#;
    let rt = ws.write_script(
        "rt",
        &format!("cat > \"$OUNCE_REPORT_JSON\" <<'EOF'\n{report}\nEOF\nexit 1"),
    );
    let core = ws.write_script(
        "core",
        &format!("cat > \"$OUNCE_REPORT_JSON\" <<'EOF'\n{report}\nEOF\nexit 1"),
    );

    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .current_dir(&ws.0)
        .args(["run", "--auto", "app.nro"])
        .env("OUNCE_CORE_BIN", &core)
        .env("OUNCE_RUNTIME_BIN", &rt)
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(13));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("attribution = Guest Bug"), "{stderr}");
    assert!(!ws.0.join("ounce-repro.json").exists());
}
