#![forbid(unsafe_code)]

//! `ounce-hb`: runner for Switch homebrew built from source with devkitPro/libnx.
//!
//! Scope: classify an input file and reject everything that is not an unencrypted homebrew NRO or
//! NSO, then run NROs through the ounce-core CLI (see [`backend`]). Layout facts come from the
//! public switchbrew wiki (NRO and NSO pages).

use std::fmt;
use std::time::Duration;

pub mod backend;
pub mod diagnose;

use backend::Staged;
pub use backend::{
    Backend, CORE_BIN_ENV, DEFAULT_TIMEOUT, RunOutput, Subprocess, find_core, locate_core,
};

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

/// Why a validated image could not be run. The `Display` text is shown to the user verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// No ounce-core binary was found. See docs/OUNCE_CORE_API_NEEDS.md.
    CoreNotFound(String),
    /// ounce-core ran past the time limit and was killed.
    Timeout(Duration),
    /// A staging, spawn or wait step failed.
    Io(String),
    /// A guest argument starting with '-' that ounce-core would silently drop.
    UnsupportedArg(String),
    UnsupportedKind(Kind),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::CoreNotFound(why) => write!(
                f,
                "ounce-core binary not found: {why}. Set OUNCE_CORE_BIN to the binary, or put it on PATH \
                 (build the ounce-core v0.8.0 CLI). Details: docs/OUNCE_CORE_API_NEEDS.md."
            ),
            RunError::Timeout(limit) => write!(
                f,
                "ounce-core did not finish within {} s and was killed. The program may be stuck.",
                limit.as_secs()
            ),
            RunError::Io(why) => write!(f, "could not run ounce-core: {why}"),
            RunError::UnsupportedArg(arg) => write!(
                f,
                "guest argument {arg:?} starts with '-'. ounce-core run-nro drops such arguments, so it is refused."
            ),
            RunError::UnsupportedKind(k) => {
                write!(
                    f,
                    "{k:?} execution is not supported yet; only NRO is supported."
                )
            }
        }
    }
}

impl std::error::Error for RunError {}

/// Run a validated image with the default backend (the ounce-core CLI, see [`backend`]).
pub fn run(kind: Kind, bytes: &[u8], args: &[String]) -> Result<RunOutput, RunError> {
    if kind != Kind::Nro {
        return Err(RunError::UnsupportedKind(kind));
    }
    let backend = Subprocess::new(locate_core()?, DEFAULT_TIMEOUT);
    run_with(&backend, kind, bytes, args)
}

/// Run a validated image with any [`Backend`]. The bytes are copied to a private file first, so the
/// program that runs is the one that was classified.
pub fn run_with<B: Backend>(
    backend: &B,
    kind: Kind,
    bytes: &[u8],
    args: &[String],
) -> Result<RunOutput, RunError> {
    match kind {
        Kind::Nro => {
            let staged = Staged::create(bytes)?;
            backend.run_nro(staged.path(), args)
        }
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
    fn run_refuses_nso_before_looking_for_core() {
        assert_eq!(
            run(Kind::Nso, &[], &[]),
            Err(RunError::UnsupportedKind(Kind::Nso))
        );
    }

    #[test]
    fn run_with_passes_the_staged_nro_to_the_backend() {
        struct Echo;
        impl Backend for Echo {
            fn run_nro(
                &self,
                nro: &std::path::Path,
                args: &[String],
            ) -> Result<RunOutput, RunError> {
                let stdout = format!("{}|{}", std::fs::read(nro).unwrap().len(), args.join(","));
                Ok(RunOutput {
                    exit_code: Some(0),
                    stdout,
                    stderr: String::new(),
                })
            }
        }
        let bytes = with(0x10, b"NRO0", 0x80);
        let out = run_with(&Echo, Kind::Nro, &bytes, &["x".to_string()]).unwrap();
        assert_eq!(out.stdout, format!("{}|x", bytes.len()));
        assert_eq!(
            run_with(&Echo, Kind::Nso, &bytes, &[]),
            Err(RunError::UnsupportedKind(Kind::Nso))
        );
    }
}
