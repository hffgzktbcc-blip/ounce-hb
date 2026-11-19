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

#[test]
fn repro_replays_bundle_and_verifies_stop_kind() {
    let ws = TempWorkspace::new("repro-replay");
    let core = ws.write_script(
        "core",
        r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-core","engine_version":"0.8.2","tier":"interpreter","silent_fallback":false,"stop_kind":"clean_exit","exit_code":0,"pc":null,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":null,"insns_executed":10,"message":null}
EOF
exit 0"#,
    );
    let manifest = ws.0.join("case.repro.json");
    fs::write(
        &manifest,
        r#"{
  "schema": "ounce.repro.v1",
  "program_path": "app.nro",
  "program_bytes": 128,
  "guest_args": ["hello"],
  "runtime": { "tier": "tier3", "stop_kind": "cpu_error", "exit_code": 12 },
  "core": { "stop_kind": "clean_exit", "exit_code": 0 }
}"#,
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .args(["repro", ws.0.to_str().unwrap()])
        .env("OUNCE_CORE_BIN", &core)
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1 passed, 0 failed"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn auto_detects_framebuffer_hash_divergence_on_clean_exit_and_writes_repro() {
    let ws = TempWorkspace::new("auto-hash-clean-diverge");
    let rt = ws.write_script(
        "rt",
        r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-runtime","engine_version":"0.6.0","tier":"tier4","silent_fallback":false,"stop_kind":"frame_limit","exit_code":0,"pc":null,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":null,"insns_executed":100,"frames_presented":5,"framebuffer_hash":"fnv1a64:1111111111111111","message":null}
EOF
exit 0"#,
    );
    let core = ws.write_script(
        "core",
        r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-core","engine_version":"0.9.1","tier":"interpreter","silent_fallback":false,"stop_kind":"frame_limit","exit_code":0,"pc":null,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":null,"insns_executed":100,"frames_presented":5,"framebuffer_hash":"fnv1a64:2222222222222222","message":null}
EOF
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
    assert!(stderr.contains("fnv1a64:1111111111111111"), "{stderr}");
    assert!(stderr.contains("fnv1a64:2222222222222222"), "{stderr}");
    let repro = fs::read_to_string(ws.0.join("ounce-repro.json")).expect("repro.json written");
    assert!(repro.contains("\"schema\": \"ounce.repro.v1\""), "{repro}");
    assert!(
        repro.contains("\"framebuffer_hash\": \"fnv1a64:1111111111111111\""),
        "{repro}"
    );
    assert!(
        repro.contains("\"framebuffer_hash\": \"fnv1a64:2222222222222222\""),
        "{repro}"
    );
}

#[cfg(unix)]
#[test]
fn auto_reports_runtime_divergence_instead_of_guest_bug_when_hashes_differ() {
    let ws = TempWorkspace::new("auto-hash-fault-diverge");
    let rt = ws.write_script(
        "rt",
        r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-runtime","engine_version":"0.6.0","tier":"tier4","silent_fallback":false,"stop_kind":"svc_break","exit_code":13,"pc":2147483660,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":305419896,"insns_executed":40,"frames_presented":2,"framebuffer_hash":"fnv1a64:aaaaaaaaaaaaaaaa","message":null}
EOF
exit 1"#,
    );
    let core = ws.write_script(
        "core",
        r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-core","engine_version":"0.9.1","tier":"interpreter","silent_fallback":false,"stop_kind":"svc_break","exit_code":13,"pc":2147483660,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":305419896,"insns_executed":40,"frames_presented":2,"framebuffer_hash":"fnv1a64:bbbbbbbbbbbbbbbb","message":null}
EOF
exit 1"#,
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
    assert!(
        stderr.contains("attribution = Runtime Divergence"),
        "{stderr}"
    );
    assert!(!stderr.contains("attribution = Guest Bug"), "{stderr}");
    let repro = fs::read_to_string(ws.0.join("ounce-repro.json")).expect("repro.json written");
    assert!(
        repro.contains("\"framebuffer_hash\": \"fnv1a64:aaaaaaaaaaaaaaaa\""),
        "{repro}"
    );
    assert!(
        repro.contains("\"framebuffer_hash\": \"fnv1a64:bbbbbbbbbbbbbbbb\""),
        "{repro}"
    );
}

#[cfg(unix)]
#[test]
fn auto_does_not_write_repro_when_framebuffer_hashes_match_or_one_is_null() {
    for (tag, core_hash) in [("match", "\"fnv1a64:1111111111111111\""), ("null", "null")] {
        let ws = TempWorkspace::new(&format!("auto-hash-{tag}"));
        let rt = ws.write_script(
            "rt",
            r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-runtime","engine_version":"0.6.0","tier":"tier4","silent_fallback":false,"stop_kind":"frame_limit","exit_code":0,"pc":null,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":null,"insns_executed":100,"frames_presented":5,"framebuffer_hash":"fnv1a64:1111111111111111","message":null}
EOF
exit 0"#,
        );
        let core = ws.write_script(
            "core",
            &format!(
                r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{{"schema":"ounce.run.v1","engine":"ounce-core","engine_version":"0.9.1","tier":"interpreter","silent_fallback":false,"stop_kind":"frame_limit","exit_code":0,"pc":null,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":null,"insns_executed":100,"frames_presented":5,"framebuffer_hash":{core_hash},"message":null}}
EOF
exit 0"#
            ),
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
            !stderr.contains("Runtime Divergence") && !stderr.contains("Guest Bug"),
            "{tag}: {stderr}"
        );
        assert!(
            !ws.0.join("ounce-repro.json").exists(),
            "{tag}: unexpected ounce-repro.json"
        );
    }
}

#[cfg(unix)]
#[test]
fn bisect_maps_pass_fail_and_infrastructure_exits_to_0_1_and_125() {
    let ws = TempWorkspace::new("bisect-codes");
    let pass_core = ws.write_script(
        "pass-core",
        r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-core","engine_version":"0.10.0","tier":"interpreter","silent_fallback":false,"stop_kind":"clean_exit","exit_code":0,"pc":null,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":null,"insns_executed":10,"message":null}
EOF
exit 0"#,
    );
    let fail_core = ws.write_script(
        "fail-core",
        r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-core","engine_version":"0.10.0","tier":"interpreter","silent_fallback":false,"stop_kind":"cpu_error","exit_code":12,"pc":2147483648,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":null,"insns_executed":10,"message":"bad"}
EOF
exit 1"#,
    );
    let slow_core = ws.write_script("slow-core", "sleep 30");
    let nonexec_core = ws.0.join("nonexec-core");
    fs::write(&nonexec_core, "#!/bin/sh\nexit 0\n").unwrap();

    let manifest = ws.0.join("case.repro.json");
    fs::write(
        &manifest,
        r#"{
  "schema": "ounce.repro.v1",
  "program_path": "app.nro",
  "guest_args": [],
  "core": { "stop_kind": "clean_exit", "exit_code": 0 }
}"#,
    )
    .unwrap();

    // Pass -> 0
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .args(["bisect", manifest.to_str().unwrap()])
        .env("OUNCE_CORE_BIN", &pass_core)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Manifest mismatch -> 1
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .args(["bisect", manifest.to_str().unwrap()])
        .env("OUNCE_CORE_BIN", &fail_core)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Refusal (exit 2: bad --sdmc, missing manifest) -> 125
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .args([
            "bisect",
            "--sdmc",
            ws.0.join("missing-sdmc").to_str().unwrap(),
            manifest.to_str().unwrap(),
        ])
        .env("OUNCE_CORE_BIN", &pass_core)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(125),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .args(["bisect", ws.0.join("missing.repro.json").to_str().unwrap()])
        .env("OUNCE_CORE_BIN", &pass_core)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(125),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // CoreMissing (exit 3) -> 125
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .args(["bisect", manifest.to_str().unwrap()])
        .env("OUNCE_CORE_BIN", ws.0.join("no-such-core"))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(125),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Timeout (exit 4) -> 125
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .args(["bisect", manifest.to_str().unwrap()])
        .env("OUNCE_CORE_BIN", &slow_core)
        .env("OUNCE_HB_TIMEOUT_SECS", "0.2")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(125),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // HostIo (exit 5: non-executable core binary) -> 125
    let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .args(["bisect", manifest.to_str().unwrap()])
        .env("OUNCE_CORE_BIN", &nonexec_core)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(125),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[cfg(unix)]
#[test]
fn git_bisect_run_finds_planted_regression_and_skips_broken_commit() {
    let ws = TempWorkspace::new("bisect-git");
    let repo = ws.0.join("repo");
    fs::create_dir_all(&repo).unwrap();

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed:\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    git(&["init", "-q"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Test"]);

    let nro_bytes = fs::read(ws.0.join("app.nro")).unwrap();
    let manifest_json = r#"{
  "schema": "ounce.repro.v1",
  "program_path": "app.nro",
  "guest_args": [],
  "core": { "stop_kind": "clean_exit", "exit_code": 0 }
}"#;
    fs::write(repo.join("case.repro.json"), manifest_json).unwrap();

    // Stand-in core binary outside the repo that inspects `repo/state.txt`.
    let state_path = repo.join("state.txt");
    let core = ws.write_script(
        "bisect-core",
        &format!(
            r#"STATE=$(cat "{}")
if [ "$STATE" = "good" ]; then
  cat > "$OUNCE_REPORT_JSON" <<'EOF'
{{"schema":"ounce.run.v1","engine":"ounce-core","engine_version":"0.10.0","tier":"interpreter","silent_fallback":false,"stop_kind":"clean_exit","exit_code":0,"pc":null,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":null,"insns_executed":10,"message":null}}
EOF
  exit 0
else
  cat > "$OUNCE_REPORT_JSON" <<'EOF'
{{"schema":"ounce.run.v1","engine":"ounce-core","engine_version":"0.10.0","tier":"interpreter","silent_fallback":false,"stop_kind":"cpu_error","exit_code":12,"pc":2147483648,"opcode":null,"fault_addr":null,"svc_id":null,"break_result":null,"insns_executed":10,"message":"planted regression"}}
EOF
  exit 1
fi"#,
            state_path.display()
        ),
    );

    // Commit 1: good
    fs::write(repo.join("app.nro"), &nro_bytes).unwrap();
    fs::write(&state_path, "good").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "c1: good"]);
    let good_sha = git(&["rev-parse", "HEAD"]).trim().to_string();

    // Commits 2 & 3: skipped (missing app.nro -> exit 2 -> mapped to 125).
    // Placing both skipped commits before c4 (good) lets git bisect visit 125
    // and still uniquely identify c5 as the first bad commit.
    fs::remove_file(repo.join("app.nro")).unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "c2: unbuilt skip 1"]);
    fs::write(repo.join("skip_note.txt"), "still unbuilt\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "c3: unbuilt skip 2"]);

    // Commit 4: still good
    fs::write(repo.join("app.nro"), &nro_bytes).unwrap();
    fs::write(&state_path, "good").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "c4: still good"]);

    // Commit 5: first bad commit (planted regression)
    fs::write(&state_path, "bad").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "c5: planted bad"]);
    let bad_sha = git(&["rev-parse", "HEAD"]).trim().to_string();

    // Commit 6: still bad
    fs::write(repo.join("extra.txt"), "more changes\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "c6: later bad"]);

    git(&["bisect", "start", "HEAD", &good_sha]);
    let bisect_out = Command::new("git")
        .current_dir(&repo)
        .args([
            "bisect",
            "run",
            env!("CARGO_BIN_EXE_ounce-hb"),
            "bisect",
            "case.repro.json",
        ])
        .env("OUNCE_CORE_BIN", &core)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&bisect_out.stdout);
    let stderr = String::from_utf8_lossy(&bisect_out.stderr);
    assert!(
        bisect_out.status.success(),
        "git bisect run failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("mapping to 125 (git bisect skip)"),
        "expected git bisect to hit a skipped commit:\n{stderr}"
    );
    assert!(
        stdout.contains(&format!("{bad_sha} is the first bad commit"))
            || stdout.contains(&format!("{bad_sha} is the first 'bad' commit")),
        "expected {bad_sha} as first bad commit:\n{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn diff_trace_compares_ounce_trace_v1_summaries_and_reports_first_divergence() {
    let ws = TempWorkspace::new("diff-trace");
    let matching_trace = r#"{"schema":"ounce.trace.v1","engine":"engine","steps_executed":2,"stop_kind":"clean_exit","exit_code":0,"steps":[{"i":0,"pc":"0x0000000080000000","opcode":"0x14000020","regs":{}},{"i":1,"pc":"0x0000000080000080","opcode":"0x52800000","regs":{"x0":"0x0000000000000000"}}]}"#;
    let diverging_trace = r#"{"schema":"ounce.trace.v1","engine":"ounce-runtime","insns_executed":2,"stop_kind":"clean_exit","exit_code":0,"steps":[{"i":0,"pc":"0x0000000080000000","opcode":"0x14000020","regs":{}},{"i":1,"pc":"0x0000000080000080","opcode":"0x52800000","regs":{"x0":"0x0000000000000099"}}]}"#;

    let core = ws.write_script(
        "core-trace",
        &format!(
            r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{{"schema":"ounce.run.v1","engine":"ounce-core","engine_version":"0.10.0","tier":"interpreter","silent_fallback":false,"stop_kind":"clean_exit","exit_code":0,"insns_executed":2,"frames_presented":0,"framebuffer_hash":null,"message":null}}
EOF
cat > "$OUNCE_TRACE_SUMMARY" <<'EOF'
{matching_trace}
EOF
exit 0"#
        ),
    );
    let rt_match = ws.write_script(
        "rt-match",
        &format!(
            r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{{"schema":"ounce.run.v1","engine":"ounce-runtime","engine_version":"0.6.0","tier":"tier4","silent_fallback":false,"stop_kind":"clean_exit","exit_code":0,"insns_executed":2,"frames_presented":0,"framebuffer_hash":null,"message":null}}
EOF
cat > "$OUNCE_TRACE_SUMMARY" <<'EOF'
{matching_trace}
EOF
exit 0"#
        ),
    );
    let rt_diverge = ws.write_script(
        "rt-diverge",
        &format!(
            r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{{"schema":"ounce.run.v1","engine":"ounce-runtime","engine_version":"0.6.0","tier":"tier4","silent_fallback":false,"stop_kind":"clean_exit","exit_code":0,"insns_executed":2,"frames_presented":1,"framebuffer_hash":"fnv1a64:1111111111111111","message":null}}
EOF
cat > "$OUNCE_TRACE_SUMMARY" <<'EOF'
{diverging_trace}
EOF
exit 0"#
        ),
    );

    // Matching traces -> exit 0
    let ok = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .current_dir(&ws.0)
        .args(["diff-trace", "--cpu", "tier4", "app.nro"])
        .env("OUNCE_CORE_BIN", &core)
        .env("OUNCE_RUNTIME_BIN", &rt_match)
        .output()
        .unwrap();
    assert_eq!(
        ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert!(
        String::from_utf8_lossy(&ok.stdout).contains("traces agree"),
        "{}",
        String::from_utf8_lossy(&ok.stdout)
    );

    // Diverging register at step 1 -> exit 1 + $GITHUB_STEP_SUMMARY written
    let summary_path = ws.0.join("step_summary.md");
    let bad = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .current_dir(&ws.0)
        .args(["diff-trace", "app.nro"])
        .env("OUNCE_CORE_BIN", &core)
        .env("OUNCE_RUNTIME_BIN", &rt_diverge)
        .env("GITHUB_STEP_SUMMARY", &summary_path)
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(stderr.contains("trace divergence"), "{stderr}");
    assert!(stderr.contains("step 1"), "{stderr}");
    assert!(stderr.contains("register x0 mismatch"), "{stderr}");

    let summary = fs::read_to_string(&summary_path).expect("step summary written");
    for col in [
        "Program",
        "Stop Kind",
        "Exit Code",
        "Instructions",
        "Frames",
        "Framebuffer Hash",
        "stop_kind",
        "insns_executed",
        "frames_presented",
        "framebuffer_hash",
        "register x0 mismatch",
    ] {
        assert!(summary.contains(col), "missing {col:?} in:\n{summary}");
    }

    // Missing engine -> exit 3
    let missing = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .current_dir(&ws.0)
        .args(["diff-trace", "app.nro"])
        .env("OUNCE_CORE_BIN", &core)
        .env("OUNCE_RUNTIME_BIN", ws.0.join("no-such-rt"))
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(3));
}

#[cfg(unix)]
#[test]
fn github_step_summary_records_table_on_auto_divergence_and_repro_failure() {
    let ws = TempWorkspace::new("step-summary");
    let summary_path = ws.0.join("summary.md");
    let rt = ws.write_script(
        "rt",
        r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-runtime","engine_version":"0.6.0","tier":"tier4","silent_fallback":false,"stop_kind":"frame_limit","exit_code":0,"insns_executed":250,"frames_presented":5,"framebuffer_hash":"fnv1a64:aaaaaaaaaaaaaaaa","message":null}
EOF
exit 0"#,
    );
    let core = ws.write_script(
        "core",
        r#"cat > "$OUNCE_REPORT_JSON" <<'EOF'
{"schema":"ounce.run.v1","engine":"ounce-core","engine_version":"0.10.0","tier":"interpreter","silent_fallback":false,"stop_kind":"frame_limit","exit_code":0,"insns_executed":250,"frames_presented":5,"framebuffer_hash":"fnv1a64:bbbbbbbbbbbbbbbb","message":null}
EOF
exit 0"#,
    );

    let auto_out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .current_dir(&ws.0)
        .args(["run", "--auto", "app.nro"])
        .env("OUNCE_CORE_BIN", &core)
        .env("OUNCE_RUNTIME_BIN", &rt)
        .env("GITHUB_STEP_SUMMARY", &summary_path)
        .output()
        .unwrap();
    assert_eq!(auto_out.status.code(), Some(0));

    // Replaying the written ounce-repro.json against `rt` fails and appends a second table.
    let repro_out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
        .current_dir(&ws.0)
        .args(["repro", "--backend", "runtime", "ounce-repro.json"])
        .env("OUNCE_RUNTIME_BIN", &rt)
        .env("GITHUB_STEP_SUMMARY", &summary_path)
        .output()
        .unwrap();
    assert_eq!(repro_out.status.code(), Some(1));

    let summary = fs::read_to_string(&summary_path).expect("summary file written");
    assert!(
        summary.contains("ounce-hb run --auto divergence"),
        "{summary}"
    );
    assert!(summary.contains("ounce-hb repro failure"), "{summary}");
    assert!(summary.contains("fnv1a64:aaaaaaaaaaaaaaaa"), "{summary}");
    assert!(summary.contains("fnv1a64:bbbbbbbbbbbbbbbb"), "{summary}");
}
