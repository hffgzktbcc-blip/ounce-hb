//! Dispatch contracts use executable shell stand-ins, not engine compatibility claims.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ounce-hb-selection-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut nro = vec![0; 0x80];
        nro[0x10..0x14].copy_from_slice(b"NRO0");
        fs::write(dir.join("app.nro"), nro).unwrap();
        Self(dir)
    }
    fn engine(&self, name: &str, body: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, format!("#!/bin/sh\n[ \"$1\" = run-nro ] || exit 90\n[ -f \"$2\" ] || exit 91\n[ \"$3\" = --present=headless ] || exit 92\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
    fn cmd(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ounce-hb"));
        cmd.current_dir(&self.0)
            .env_remove("OUNCE_BACKEND")
            .env_remove("OUNCE_CORE_BIN")
            .env_remove("OUNCE_RUNTIME_BIN")
            .arg("run")
            .arg("app.nro");
        cmd
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn code(out: &Output, expected: i32) {
    assert_eq!(
        out.status.code(),
        Some(expected),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
const CLEAN: &str = "echo '[VM] Process exited gracefully (svcExitProcess).'";

#[test]
fn named_backends_use_their_own_override_and_pass_headless_and_guest_args() {
    for (name, variable) in [("core", "OUNCE_CORE_BIN"), ("runtime", "OUNCE_RUNTIME_BIN")] {
        let f = Fixture::new();
        let engine = f.engine(
            name,
            &format!("[ \"$4\" = 'hello world' ] || exit 93\n{CLEAN}"),
        );
        let out = f
            .cmd()
            .env(variable, engine)
            .env(
                "OUNCE_CORE_BIN",
                if name == "runtime" {
                    PathBuf::from("/missing/core")
                } else {
                    f.0.join(name)
                },
            )
            .args(["--backend", name, "--", "hello world"])
            .output()
            .unwrap();
        code(&out, 0);
    }
}

#[test]
fn default_is_core_and_cli_overrides_environment() {
    let f = Fixture::new();
    let core = f.engine("core", CLEAN);
    code(&f.cmd().env("OUNCE_CORE_BIN", &core).output().unwrap(), 0);
    code(
        &f.cmd()
            .env("OUNCE_BACKEND", "runtime")
            .env("OUNCE_CORE_BIN", core)
            .args(["--backend", "core"])
            .output()
            .unwrap(),
        0,
    );
}

#[test]
fn environment_selects_runtime_and_explicit_relative_path() {
    let f = Fixture::new();
    let runtime = f.engine("custom engine", CLEAN);
    code(
        &f.cmd()
            .env("OUNCE_BACKEND", "runtime")
            .env("OUNCE_RUNTIME_BIN", &runtime)
            .output()
            .unwrap(),
        0,
    );
    code(
        &f.cmd()
            .env("OUNCE_BACKEND", "./custom engine")
            .output()
            .unwrap(),
        0,
    );
    code(
        &f.cmd()
            .args(["--backend", "custom engine"])
            .output()
            .unwrap(),
        0,
    );
    code(&f.cmd().arg("--backend").arg(&runtime).output().unwrap(), 0);
    code(
        &f.cmd()
            .env("OUNCE_RUNTIME_BIN", "/missing")
            .args(["--backend", "./custom engine"])
            .output()
            .unwrap(),
        0,
    );
}

#[test]
fn named_backends_search_path_and_report_missing_override() {
    for (name, variable) in [("core", "OUNCE_CORE_BIN"), ("runtime", "OUNCE_RUNTIME_BIN")] {
        let f = Fixture::new();
        f.engine(&format!("ounce-{name}"), CLEAN);
        code(
            &f.cmd()
                .env("PATH", &f.0)
                .args(["--backend", name])
                .output()
                .unwrap(),
            0,
        );
        let out = f
            .cmd()
            .env("PATH", &f.0)
            .env(variable, "/missing/engine")
            .args(["--backend", name])
            .output()
            .unwrap();
        code(&out, 3);
        assert!(String::from_utf8_lossy(&out.stderr).contains(variable));
    }
}

#[test]
fn backend_before_program_and_invalid_options() {
    let f = Fixture::new();
    let runtime = f.engine("runtime", CLEAN);
    code(
        &Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
            .env("OUNCE_RUNTIME_BIN", runtime)
            .args(["run", "--backend", "runtime"])
            .arg(f.0.join("app.nro"))
            .output()
            .unwrap(),
        0,
    );
    for args in [
        vec!["--backend"],
        vec!["--backend", ""],
        vec!["--backend", "core", "--backend", "runtime"],
    ] {
        code(&f.cmd().args(args).output().unwrap(), 2);
    }
    code(
        &f.cmd().args(["--backend", "./missing"]).output().unwrap(),
        3,
    );
}

#[test]
fn all_backends_preserve_fault_classification() {
    for name in ["core", "runtime", "./custom"] {
        let f = Fixture::new();
        let engine = f.engine("custom", "echo '[VM Error] Memory access violation: Unmapped memory access at virtual address: 0x10' >&2\nexit 1");
        let out = f
            .cmd()
            .env("OUNCE_CORE_BIN", &engine)
            .env("OUNCE_RUNTIME_BIN", &engine)
            .args(["--backend", name])
            .output()
            .unwrap();
        code(&out, 11);
    }
}

#[test]
fn runtime_timeout_and_output_cap_use_shared_runner() {
    let f = Fixture::new();
    let slow = f.engine("slow", "exec /bin/sleep 30");
    code(
        &f.cmd()
            .env("OUNCE_RUNTIME_BIN", slow)
            .env("OUNCE_HB_TIMEOUT_SECS", "1")
            .args(["--backend", "runtime"])
            .output()
            .unwrap(),
        4,
    );
    let spam = f.engine("spam", &format!("head -c {} /dev/zero | tr '\\0' x\necho\necho '[VM] Execution budget reached (7 instructions).'\nexit 1", 3 * ounce_hb::MAX_CAPTURED_BYTES));
    let out = f
        .cmd()
        .env("OUNCE_RUNTIME_BIN", spam)
        .args(["--backend", "runtime"])
        .output()
        .unwrap();
    code(&out, 16);
    assert!(out.stdout.len() < ounce_hb::MAX_CAPTURED_BYTES + 1024);
    assert!(String::from_utf8_lossy(&out.stdout).contains("were dropped"));
}

#[test]
fn engine_selection_distinguishes_names_from_paths() {
    use ounce_hb::Engine;
    use std::ffi::OsStr;
    assert_eq!(Engine::from_selection(OsStr::new("core")), Engine::Core);
    assert_eq!(
        Engine::from_selection(OsStr::new("runtime")),
        Engine::Runtime
    );
    for path in ["./runtime", "core-custom", "/tmp/custom engine"] {
        assert_eq!(
            Engine::from_selection(OsStr::new(path)),
            Engine::Path(path.into())
        );
    }
}

#[test]
fn runtime_resolver_skips_nonexecutables_and_empty_path_entries() {
    use ounce_hb::find_runtime;
    let f = Fixture::new();
    let first = f.0.join("first");
    let second = f.0.join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(first.join("ounce-runtime"), "not executable").unwrap();
    let executable = second.join("ounce-runtime");
    fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths([&first, &second]).unwrap();
    assert_eq!(find_runtime(None, Some(path.clone())).unwrap(), executable);
    assert_eq!(
        find_runtime(Some("".into()), Some(path)).unwrap(),
        executable
    );
    assert!(find_runtime(None, Some(":".into())).is_err());
    assert!(find_runtime(Some(f.0.clone().into_os_string()), None).is_err());
}

#[test]
fn runtime_fault_reproduction_keeps_backend_selection() {
    let f = Fixture::new();
    let engine = f.engine(
        "runtime",
        "echo '[VM] Execution budget reached (7 instructions).'\nexit 1",
    );
    let out = f
        .cmd()
        .env("OUNCE_BACKEND", "runtime")
        .env("OUNCE_RUNTIME_BIN", engine)
        .output()
        .unwrap();
    code(&out, 16);
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("To reproduce: ounce-hb run app.nro --backend runtime")
    );
}
