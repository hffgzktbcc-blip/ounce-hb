//! Real libnx NSO fixture; validation and LZ4 decompression always run, execution requires a backend.
use std::process::Command;

const IMAGE: &[u8] = include_bytes!("fixtures/libnx_nso_hello/libnx_nso_hello.nso");

fn runner() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ounce-hb"));
    command.env_remove("OUNCE_BACKEND");
    command.args([
        "run",
        "--backend",
        "core",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/libnx_nso_hello/libnx_nso_hello.nso"
        ),
    ]);
    command
}

#[test]
fn libnx_nso_fixture_validates_and_decompresses_without_backend() {
    assert_eq!(&IMAGE[0..4], b"NSO0");
    assert_eq!(ounce_hb::classify(IMAGE), Ok(ounce_hb::Kind::Nso));

    let decompressed = ounce_hb::nso::decompress_nso(IMAGE).expect("valid elf2nso NSO");
    assert_eq!(&decompressed[0..4], b"NSO0");
    let flags = u32::from_le_bytes([
        decompressed[0x0C],
        decompressed[0x0D],
        decompressed[0x0E],
        decompressed[0x0F],
    ]);
    assert_eq!(
        flags & 0x07,
        0,
        "staged NSO must have compression flags cleared"
    );

    let output = runner()
        .env_remove("OUNCE_CORE_BIN")
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("backend was not found"));
}

#[test]
fn libnx_nso_fixture_refuses_guest_args_before_spawning_backend() {
    let output = runner()
        .env_remove("OUNCE_CORE_BIN")
        .env("PATH", "")
        .args(["--", "extra"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("NSO programs do not accept guest arguments")
    );
}

#[test]
fn libnx_nso_fixture_runs_across_all_three_cpu_engines() {
    let Ok(core) = ounce_hb::locate_core() else {
        eprintln!("SKIPPED libnx NSO execution: no ounce-core binary found");
        return;
    };

    for tier in ["interp", "tier3", "tier4"] {
        let output = runner()
            .env("OUNCE_CORE_BIN", &core)
            .env("OUNCE_HB_TIMEOUT_SECS", "30")
            .args(["--cpu", tier])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(0),
            "tier={tier}\n{stdout}\n{stderr}"
        );
        assert!(
            stdout.contains("Hello NSO from ounce-hb!\n"),
            "tier={tier}\n{stdout}"
        );
        assert!(
            stdout.contains("Process exited gracefully (svcExitProcess)."),
            "tier={tier}\n{stdout}"
        );
    }
}
