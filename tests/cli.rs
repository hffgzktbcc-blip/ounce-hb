use std::process::Command;

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ounce-hb"));
    // Point at a path that is never a file so the outcome does not depend on the host's PATH.
    c.env("OUNCE_CORE_BIN", "/nonexistent/ounce-core-for-cli-test");
    c
}

fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("ounce-hb-{}-{name}", std::process::id()));
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn rejects_non_homebrew_with_message() {
    let mut pfs = vec![0u8; 0x300];
    pfs[..4].copy_from_slice(b"PFS0");
    let p = tmp("a.nsp", &pfs);
    let out = bin().arg("run").arg(&p).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("out of scope"));
    let _ = std::fs::remove_file(p);
}

#[test]
fn valid_nro_without_core_binary_reports_it() {
    let mut nro = vec![0u8; 0x80];
    nro[0x10..0x14].copy_from_slice(b"NRO0");
    let p = tmp("a.nro", &nro);
    let out = bin().arg("run").arg(&p).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("OUNCE_CORE_BIN"));
    let _ = std::fs::remove_file(p);
}

#[test]
fn usage_without_args() {
    assert_eq!(bin().output().unwrap().status.code(), Some(2));
}
