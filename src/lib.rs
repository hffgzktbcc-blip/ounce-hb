#![forbid(unsafe_code)]

//! `ounce-hb`: runner for Switch homebrew built from source with devkitPro/libnx.
//!
//! M0 scope: classify an input file and reject everything that is not an unencrypted homebrew
//! NRO or NSO. Layout facts come from the public switchbrew wiki (NRO and NSO pages).

use std::fmt;

/// What a file turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Homebrew executable (`NRO0` at offset 0x10).
    Nro,
    /// Homebrew executable module (`NSO0` at offset 0).
    Nso,
}

/// Why an input was refused. The `Display` text is shown to the user verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    TooSmall,
    /// A retail or system container format. Never supported.
    OutOfScopeContainer(&'static str),
    /// Permission metadata, not an executable.
    Npdm,
    Unrecognized,
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const SCOPE: &str = "ounce-hb only runs unencrypted homebrew (NRO/NSO) built from source with devkitPro/libnx";
        match self {
            Rejection::TooSmall => write!(f, "file is too small to be an NRO or NSO. {SCOPE}."),
            Rejection::OutOfScopeContainer(what) => write!(
                f,
                "this looks like {what}, which is out of scope and will never be supported. {SCOPE}."
            ),
            Rejection::Npdm => write!(
                f,
                "this is an NPDM (permission metadata), not an executable. Pass the NRO or NSO instead."
            ),
            Rejection::Unrecognized => {
                write!(f, "not a recognized NRO or NSO (bad magic). {SCOPE}.")
            }
        }
    }
}

impl std::error::Error for Rejection {}

/// Classify `bytes` by magic only. Nothing is parsed beyond identifying the format.
pub fn classify(bytes: &[u8]) -> Result<Kind, Rejection> {
    if bytes.len() < 0x14 {
        return Err(Rejection::TooSmall);
    }
    if &bytes[0x10..0x14] == b"NRO0" {
        return Ok(Kind::Nro);
    }
    match &bytes[0..4] {
        b"NSO0" => return Ok(Kind::Nso),
        b"META" => return Err(Rejection::Npdm),
        b"PFS0" => return Err(Rejection::OutOfScopeContainer("an NSP/PFS0 package")),
        _ => {}
    }
    if bytes.len() >= 0x104 && &bytes[0x100..0x104] == b"HEAD" {
        return Err(Rejection::OutOfScopeContainer("an XCI game card image"));
    }
    if bytes.len() >= 0x204 && matches!(&bytes[0x200..0x204], b"NCA0" | b"NCA2" | b"NCA3") {
        return Err(Rejection::OutOfScopeContainer("an NCA content archive"));
    }
    Err(Rejection::Unrecognized)
}

/// Outcome of asking the core to run a validated image.
#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    /// ounce-core exposes no library API yet. See docs/OUNCE_CORE_API_NEEDS.md.
    CoreApiMissing,
    UnsupportedKind(Kind),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::CoreApiMissing => write!(
                f,
                "input is a valid NRO, but execution is not wired yet: ounce-core has no library target, \
                 so its homebrew loader, MMU, CPU and kernel cannot be called from here. \
                 Missing API is listed in docs/OUNCE_CORE_API_NEEDS.md."
            ),
            RunError::UnsupportedKind(k) => {
                write!(
                    f,
                    "{k:?} execution is not supported in M0; only NRO is planned first."
                )
            }
        }
    }
}

impl std::error::Error for RunError {}

/// Run a validated image. M0 stub: see docs/OUNCE_CORE_API_NEEDS.md.
pub fn run(kind: Kind, _bytes: &[u8], _args: &[String]) -> Result<i32, RunError> {
    match kind {
        Kind::Nro => Err(RunError::CoreApiMissing),
        other => Err(RunError::UnsupportedKind(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(off: usize, magic: &[u8], len: usize) -> Vec<u8> {
        let mut v = vec![0u8; len];
        v[off..off + magic.len()].copy_from_slice(magic);
        v
    }

    #[test]
    fn accepts_nro_and_nso() {
        assert_eq!(classify(&with(0x10, b"NRO0", 0x80)), Ok(Kind::Nro));
        assert_eq!(classify(&with(0, b"NSO0", 0x100)), Ok(Kind::Nso));
    }

    #[test]
    fn rejects_retail_containers() {
        assert!(matches!(
            classify(&with(0, b"PFS0", 0x300)),
            Err(Rejection::OutOfScopeContainer(_))
        ));
        assert!(matches!(
            classify(&with(0x100, b"HEAD", 0x300)),
            Err(Rejection::OutOfScopeContainer(_))
        ));
        assert!(matches!(
            classify(&with(0x200, b"NCA3", 0x300)),
            Err(Rejection::OutOfScopeContainer(_))
        ));
    }

    #[test]
    fn rejects_other_inputs() {
        assert_eq!(classify(&[0; 4]), Err(Rejection::TooSmall));
        assert_eq!(classify(&with(0, b"META", 0x40)), Err(Rejection::Npdm));
        assert_eq!(classify(&[0xAA; 0x200]), Err(Rejection::Unrecognized));
    }

    #[test]
    fn run_stub_names_the_gap() {
        let e = run(Kind::Nro, &[], &[]).unwrap_err();
        assert_eq!(e, RunError::CoreApiMissing);
        assert!(e.to_string().contains("OUNCE_CORE_API_NEEDS"));
    }
}
