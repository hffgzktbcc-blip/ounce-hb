//! Runs a synthetic homebrew NRO through the real ounce-core CLI and checks the exit code.
//!
//! Needs an ounce-core binary: `OUNCE_CORE_BIN` or `ounce-core` on `PATH`. Without one the test
//! prints a SKIPPED line and passes. If `OUNCE_CORE_BIN` is set but wrong, the test fails.

use std::time::Duration;

use ounce_hb::{CORE_BIN_ENV, Kind, Subprocess, locate_core, run_with};

/// A homebrew NRO0 with `text` at offset 0x80, laid out as ounce-core's loader reads it: a branch
/// over the header at 0x00, the `NRO0` magic at 0x10, and the text size at 0x24. Built here, not
/// committed, so there is no binary to register in docs/PROVENANCE.md.
fn synthetic_nro(text: &[u8]) -> Vec<u8> {
    let total = 0x80 + text.len();
    let mut b = vec![0u8; total];
    b[0x00..0x04].copy_from_slice(&0x1400_0020u32.to_le_bytes()); // b #0x80
    b[0x10..0x14].copy_from_slice(b"NRO0");
    b[0x18..0x1C].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x24..0x28].copy_from_slice(&(total as u32).to_le_bytes());
    b[0x80..].copy_from_slice(text);
    b
}

#[test]
fn synthetic_nro_exits_zero_under_ounce_core() {
    let bin = match locate_core() {
        Ok(bin) => bin,
        Err(e) => {
            assert!(
                std::env::var_os(CORE_BIN_ENV).is_none(),
                "{CORE_BIN_ENV} is set but unusable: {e}"
            );
            eprintln!("SKIPPED ounce-core run test: no ounce-core binary available ({e})");
            return;
        }
    };
    // mov w0, #0; ret. The homebrew loader points X30 at an exit stub, so returning exits the process.
    let text: Vec<u8> = [0x5280_0000u32, 0xD65F_03C0]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    let nro = synthetic_nro(&text);

    let backend = Subprocess::new(bin, Duration::from_secs(120));
    let out = run_with(&backend, Kind::Nro, &nro, &[]).expect("ounce-core should run the NRO");
    assert_eq!(out.exit_code, Some(0), "stderr:\n{}", out.stderr);
    assert!(
        out.stdout.contains("exited gracefully"),
        "stdout:\n{}",
        out.stdout
    );
}
