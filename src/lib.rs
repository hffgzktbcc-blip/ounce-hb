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
pub mod report_v1;

pub use backend::{
    BACKEND_ENV, Backend, CORE_BIN_ENV, CpuTier, DEFAULT_TIMEOUT, Engine, HEAD_BYTES,
    MAX_CAPTURED_BYTES, PIPE_CLOSE_GRACE, RUNTIME_BIN_ENV, RunOutput, Staged, Subprocess,
    TAIL_BYTES, find_core, find_runtime, locate_core, truncation_marker, validate_sdmc_dir,
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

/// Checks whether a validated NRO image carries a trailing `ASET` asset section with a non-zero
/// embedded RomFS section size (`u32::from_le_bytes(bytes[0x18..0x1C])` points to `ASET`, with
/// a non-zero RomFS section size at `+0x18..+0x20` or standard `NroAssetHeader` `+0x30..+0x38`).
pub fn has_embedded_romfs(bytes: &[u8]) -> bool {
    if bytes.len() < 0x1C || classify(bytes) != Ok(Kind::Nro) {
        return false;
    }
    let nro_size =
        u32::from_le_bytes([bytes[0x18], bytes[0x19], bytes[0x1A], bytes[0x1B]]) as usize;
    if nro_size < 0x80 {
        return false;
    }
    let Some(aset) = bytes.get(nro_size..) else {
        return false;
    };
    if aset.len() < 0x20 || &aset[0..4] != b"ASET" {
        return false;
    }
    let size_at_18 = u64::from_le_bytes([
        aset[0x18], aset[0x19], aset[0x1A], aset[0x1B], aset[0x1C], aset[0x1D], aset[0x1E],
        aset[0x1F],
    ]);
    if aset.len() >= 0x38 {
        let nacp_size_at_20 = u64::from_le_bytes([
            aset[0x20], aset[0x21], aset[0x22], aset[0x23], aset[0x24], aset[0x25], aset[0x26],
            aset[0x27],
        ]);
        let romfs_size_at_30 = u64::from_le_bytes([
            aset[0x30], aset[0x31], aset[0x32], aset[0x33], aset[0x34], aset[0x35], aset[0x36],
            aset[0x37],
        ]);
        if romfs_size_at_30 != 0 {
            return true;
        }
        if nacp_size_at_20 != 0 {
            return false;
        }
    }
    size_at_18 != 0
}

/// Why a validated image could not be run. The `Display` text is shown to the user verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// The selected backend binary was not found (legacy variant name).
    CoreNotFound(String),
    /// ounce-core ran past the time limit and was killed.
    Timeout(Duration),
    /// A staging, spawn or wait step failed.
    Io(String),
    /// A guest argument starting with '-' that ounce-core would silently drop.
    UnsupportedArg(String),
    UnsupportedKind(Kind),
    /// `OUNCE_HB_TIMEOUT_SECS` is set to something other than a usable number of seconds.
    BadTimeout(String),
    /// `--cpu` was given an unsupported engine tier name.
    BadCpu(String),
    /// `--sdmc` pointed at a missing path or non-directory.
    BadSdmc(String),
    /// The guest arguments together are longer than [`MAX_GUEST_ARGS_BYTES`]. `bytes` is the
    /// measured total, counted as [`guest_args_len`] does.
    GuestArgsTooLong {
        bytes: usize,
    },
}

/// Largest combined guest-argument length ounce-hb passes to ounce-core. Above it, ounce-core v0.8.0
/// writes the argument strings over the program in memory (ounce-core #418: the config block at
/// 0x7FFF0000 runs into the NRO at 0x80000000 once the strings pass about 62,839 bytes). ounce-core
/// v0.8.1 refuses an argument string (program path and guest arguments, joined by spaces) over
/// 62,855 bytes itself with a `[Loader Error]`. This limit stays well below both, so the refusal
/// comes from ounce-hb with a clear message before any ounce-core process starts.
pub const MAX_GUEST_ARGS_BYTES: usize = 16 * 1024;

/// Combined guest-argument length, counted as the bytes of each argument plus one NUL terminator, as
/// argv strings are laid out in memory.
pub fn guest_args_len(args: &[String]) -> usize {
    args.iter().map(|a| a.len() + 1).sum()
}

/// Refuses guest arguments longer than [`MAX_GUEST_ARGS_BYTES`] before any ounce-core process starts.
pub fn check_guest_args(args: &[String]) -> Result<(), RunError> {
    let bytes = guest_args_len(args);
    if bytes > MAX_GUEST_ARGS_BYTES {
        return Err(RunError::GuestArgsTooLong { bytes });
    }
    Ok(())
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::CoreNotFound(why) => write!(
                f,
                "backend binary not found: {why}. Set the selected engine's binary override, or put it on PATH."
            ),
            RunError::Timeout(limit) => write!(
                f,
                "backend did not finish within {} s and was killed. The program may be stuck.",
                limit.as_secs_f64()
            ),
            RunError::Io(why) => write!(f, "could not run backend: {why}"),
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
            RunError::BadTimeout(why) | RunError::BadCpu(why) | RunError::BadSdmc(why) => {
                write!(f, "{why}")
            }
            RunError::GuestArgsTooLong { bytes } => write!(
                f,
                "the guest arguments are {bytes} bytes together, above the {MAX_GUEST_ARGS_BYTES}-byte limit. \
                 ounce-core v0.8.0 overwrites the program in memory with arguments that long (ounce-core #418), \
                 and v0.8.1 refuses an argument string over 62,855 bytes itself, so the run is refused here first. \
                 Shorten the arguments."
            ),
        }
    }
}

impl std::error::Error for RunError {}

/// Parses a `--cpu` tier override (`interp`, `interpreter`, `tier3`, or `tier4`).
pub fn parse_cpu(value: &str) -> Result<CpuTier, RunError> {
    CpuTier::parse(value).ok_or_else(|| {
        RunError::BadCpu(format!(
            "--cpu must be interp, tier3, or tier4, got {value:?}."
        ))
    })
}

/// Environment variable that overrides the wall-clock limit, in seconds. Unset means
/// [`DEFAULT_TIMEOUT`]. A value that is not a number greater than 0 and at most [`MAX_TIMEOUT_SECS`]
/// is refused (exit code 2), it is never silently replaced by the default.
pub const TIMEOUT_ENV: &str = "OUNCE_HB_TIMEOUT_SECS";

/// Largest accepted `OUNCE_HB_TIMEOUT_SECS`: one day.
pub const MAX_TIMEOUT_SECS: f64 = 86_400.0;

/// Reads [`TIMEOUT_ENV`]. Unset means [`DEFAULT_TIMEOUT`].
pub fn timeout_from_env() -> Result<Duration, RunError> {
    match std::env::var(TIMEOUT_ENV) {
        Ok(value) => parse_timeout(&value),
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_TIMEOUT),
        Err(std::env::VarError::NotUnicode(_)) => Err(RunError::BadTimeout(format!(
            "{TIMEOUT_ENV} is not valid text. Set it to a number of seconds greater than 0 and at most {MAX_TIMEOUT_SECS}, or unset it."
        ))),
    }
}

/// Parses a timeout override in seconds. Surrounding spaces are allowed.
pub fn parse_timeout(value: &str) -> Result<Duration, RunError> {
    parse_timeout_named(value, TIMEOUT_ENV)
}

/// Like [`parse_timeout`], naming the setting (`--timeout`, an environment variable) in the error.
pub fn parse_timeout_named(value: &str, name: &str) -> Result<Duration, RunError> {
    match value.trim().parse::<f64>() {
        Ok(secs) if secs.is_finite() && secs > 0.0 && secs <= MAX_TIMEOUT_SECS => {
            Ok(Duration::from_secs_f64(secs))
        }
        _ => Err(RunError::BadTimeout(format!(
            "{name} must be a number of seconds greater than 0 and at most {MAX_TIMEOUT_SECS}, got {value:?}."
        ))),
    }
}

/// Run using `OUNCE_BACKEND`, or ounce-core by default (see [`backend`]). The limit
/// is [`DEFAULT_TIMEOUT`] unless [`TIMEOUT_ENV`] overrides it.
pub fn run(kind: Kind, bytes: &[u8], args: &[String]) -> Result<RunOutput, RunError> {
    let selection = std::env::var_os(BACKEND_ENV).filter(|v| !v.is_empty());
    let engine = selection
        .as_deref()
        .map(Engine::from_selection)
        .unwrap_or(Engine::Core);
    run_engine(&engine, kind, bytes, args)
}

/// Run with an explicit engine, retaining the shared subprocess safety limits.
pub fn run_engine(
    engine: &Engine,
    kind: Kind,
    bytes: &[u8],
    args: &[String],
) -> Result<RunOutput, RunError> {
    run_engine_timeout(engine, kind, bytes, args, None)
}

/// [`run_engine`] with an explicit wall-clock limit. `None` falls back to [`TIMEOUT_ENV`], then
/// [`DEFAULT_TIMEOUT`].
pub fn run_engine_timeout(
    engine: &Engine,
    kind: Kind,
    bytes: &[u8],
    args: &[String],
    limit: Option<Duration>,
) -> Result<RunOutput, RunError> {
    run_engine_opts(engine, kind, bytes, args, limit, None, None)
}

/// [`run_engine_timeout`] with optional `--cpu` tier and `--sdmc` directory overrides.
pub fn run_engine_opts(
    engine: &Engine,
    kind: Kind,
    bytes: &[u8],
    args: &[String],
    limit: Option<Duration>,
    cpu: Option<CpuTier>,
    sdmc: Option<&std::path::Path>,
) -> Result<RunOutput, RunError> {
    if kind != Kind::Nro {
        return Err(RunError::UnsupportedKind(kind));
    }
    check_guest_args(args)?;
    if let Some(dir) = sdmc {
        validate_sdmc_dir(dir)?;
    }
    let timeout = match limit {
        Some(l) => l,
        None => timeout_from_env()?,
    };
    let backend = Subprocess::new(engine.locate()?, timeout)
        .with_cpu(cpu)
        .with_sdmc(sdmc.map(std::path::Path::to_path_buf));
    run_with(&backend, kind, bytes, args)
}

/// [`run_engine_opts`] with `OUNCE_TRACE_SUMMARY` enabled to capture `ounce.trace.v1` output.
pub fn run_engine_trace_opts(
    engine: &Engine,
    kind: Kind,
    bytes: &[u8],
    args: &[String],
    limit: Option<Duration>,
    cpu: Option<CpuTier>,
    sdmc: Option<&std::path::Path>,
) -> Result<RunOutput, RunError> {
    if kind != Kind::Nro {
        return Err(RunError::UnsupportedKind(kind));
    }
    check_guest_args(args)?;
    if let Some(dir) = sdmc {
        validate_sdmc_dir(dir)?;
    }
    let timeout = match limit {
        Some(l) => l,
        None => timeout_from_env()?,
    };
    let backend = Subprocess::new(engine.locate()?, timeout)
        .with_cpu(cpu)
        .with_sdmc(sdmc.map(std::path::Path::to_path_buf))
        .with_trace_summary(true);
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
            check_guest_args(args)?;
            let staged = Staged::create(bytes)?;
            backend.run_nro(&staged, args)
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
    fn timeout_override_accepts_only_positive_seconds_up_to_a_day() {
        assert_eq!(parse_timeout("2"), Ok(Duration::from_secs(2)));
        assert_eq!(parse_timeout(" 1.5 "), Ok(Duration::from_millis(1500)));
        assert_eq!(parse_timeout("86400"), Ok(Duration::from_secs(86_400)));
        for bad in ["0", "-1", "nan", "inf", "1e9", "", "soon", "86400.5"] {
            assert!(
                matches!(parse_timeout(bad), Err(RunError::BadTimeout(m)) if m.contains(TIMEOUT_ENV)),
                "{bad:?} should be refused"
            );
        }
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
            fn run_nro(&self, program: &Staged, args: &[String]) -> Result<RunOutput, RunError> {
                let stdout = format!(
                    "{}|{}",
                    std::fs::read(program.path()).unwrap().len(),
                    args.join(",")
                );
                Ok(RunOutput {
                    exit_code: Some(0),
                    stdout,
                    ..RunOutput::default()
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

    #[test]
    fn detects_embedded_romfs_in_nro_aset_section() {
        let mut plain = with(0x10, b"NRO0", 0x80);
        plain[0x18..0x1C].copy_from_slice(&(0x80u32).to_le_bytes());
        assert!(!has_embedded_romfs(&plain));

        // ASET header at offset 0x80 with non-zero RomFS section size at +0x18..+0x20.
        let mut with_romfs_18 = plain.clone();
        with_romfs_18.extend_from_slice(&[0u8; 0x20]);
        with_romfs_18[0x80..0x84].copy_from_slice(b"ASET");
        with_romfs_18[0x80 + 0x18..0x80 + 0x20].copy_from_slice(&1024u64.to_le_bytes());
        assert!(has_embedded_romfs(&with_romfs_18));

        // Full 0x38-byte ASET header with non-zero RomFS size at +0x30..+0x38.
        let mut with_romfs_30 = plain.clone();
        with_romfs_30.extend_from_slice(&[0u8; 0x38]);
        with_romfs_30[0x80..0x84].copy_from_slice(b"ASET");
        with_romfs_30[0x80 + 0x28..0x80 + 0x30].copy_from_slice(&0x38u64.to_le_bytes());
        with_romfs_30[0x80 + 0x30..0x80 + 0x38].copy_from_slice(&512u64.to_le_bytes());
        assert!(has_embedded_romfs(&with_romfs_30));

        // ASET header with zero RomFS size returns false.
        let mut zero_romfs = plain.clone();
        zero_romfs.extend_from_slice(&[0u8; 0x38]);
        zero_romfs[0x80..0x84].copy_from_slice(b"ASET");
        assert!(!has_embedded_romfs(&zero_romfs));

        // Non-NRO or bad magic at nro_size returns false.
        assert!(!has_embedded_romfs(&with(0, b"NSO0", 0x100)));
        let mut bad_aset = with_romfs_18.clone();
        bad_aset[0x80..0x84].copy_from_slice(b"NOPE");
        assert!(!has_embedded_romfs(&bad_aset));
    }
}
