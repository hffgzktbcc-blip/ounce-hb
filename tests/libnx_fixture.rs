//! Real libnx fixture; validation always runs, execution requires an explicit backend.
use std::process::Command;

const IMAGE: &[u8] = include_bytes!("fixtures/libnx_hello/libnx_hello.nro");

fn runner() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ounce-hb"));
    command.env_remove("OUNCE_BACKEND");
    command.args([
        "run",
        "--backend",
        "core",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/libnx_hello/libnx_hello.nro"
        ),
    ]);
    command
}

#[test]
fn libnx_fixture_validates_without_backend() {
    assert_eq!(&IMAGE[0x10..0x14], b"NRO0");
    assert_eq!(ounce_hb::classify(IMAGE), Ok(ounce_hb::Kind::Nro));
    // There is no CLI dry-run mode: exercise validation and the missing-backend diagnostic.
    let output = runner()
        .env_remove("OUNCE_CORE_BIN")
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("backend was not found"));
}

#[test]
fn libnx_fixture_exits_zero_and_echoes_arguments() {
    let Some(core) = std::env::var_os("OUNCE_CORE_BIN") else {
        eprintln!("SKIPPED libnx execution: set OUNCE_CORE_BIN; validation still runs");
        return;
    };
    for args in [vec![], vec!["--", "test", "alpha"]] {
        let output = runner()
            .env("OUNCE_CORE_BIN", &core)
            .env("OUNCE_HB_TIMEOUT_SECS", "30")
            .args(&args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(0), "{stdout}\n{stderr}");
        assert!(stdout.contains("Hello from ounce-hb!\n"), "{stdout}");
        assert!(stdout.contains("Process exited gracefully (svcExitProcess)."));
        if args.is_empty() {
            assert!(!stdout.contains("arg[1]:"), "{stdout}");
        } else {
            assert!(stdout.contains("arg[1]: test\narg[2]: alpha\n"), "{stdout}");
        }
    }
}
