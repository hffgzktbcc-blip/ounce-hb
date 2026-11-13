#![forbid(unsafe_code)]

use std::io::{self, Write};
use std::process::ExitCode;

const USAGE: &str =
    "usage: ounce-hb run [--backend <core|runtime|path>] <file.nro> [-- guest args...]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok((path, guest, backend)) => run(path, guest, backend),
        Err(message) => {
            say_err(&format!("{message}\n"));
            ExitCode::from(2)
        }
    }
}

/// Parse runner options on either side of the program path, stopping at the guest separator.
type Parsed<'a> = (&'a str, &'a [String], Option<&'a str>);

fn parse(args: &[String]) -> Result<Parsed<'_>, String> {
    if args.first().map(String::as_str) != Some("run") {
        return Err(USAGE.to_string());
    }
    let mut path = None;
    let mut backend = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--" => {
                return path
                    .map(|p| (p, &args[i + 1..], backend))
                    .ok_or_else(|| USAGE.to_string());
            }
            "--backend" => {
                if backend.is_some() {
                    return Err(format!("ounce-hb: duplicate --backend.\n{USAGE}"));
                }
                i += 1;
                backend = args
                    .get(i)
                    .map(String::as_str)
                    .filter(|value| !value.is_empty() && !value.starts_with('-'));
                if backend.is_none() {
                    return Err(format!(
                        "ounce-hb: --backend requires core, runtime, or a path.\n{USAGE}"
                    ));
                }
            }
            value if path.is_none() && !value.starts_with('-') => path = Some(value),
            extra => {
                return Err(format!(
                    "ounce-hb: unexpected argument {extra:?}; guest arguments go after '--'.\n{USAGE}"
                ));
            }
        }
        i += 1;
    }
    path.map(|p| (p, &args[args.len()..], backend))
        .ok_or_else(|| USAGE.to_string())
}

fn run(path: &str, guest: &[String], backend: Option<&str>) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            say_err(&format!("ounce-hb: cannot read {path}: {e}\n"));
            return ExitCode::from(2);
        }
    };
    let kind = match ounce_hb::classify(&bytes) {
        Ok(k) => k,
        Err(r) => {
            say_err(&format!("ounce-hb: rejected {path}: {r}\n"));
            return ExitCode::from(2);
        }
    };
    // The guest's own output is passed through unchanged. Anything abnormal is explained after it,
    // and the exit code says which category it was (docs/DIAGNOSIS.md).
    let mut note = None;
    let selection = backend
        .map(std::ffi::OsString::from)
        .or_else(|| std::env::var_os(ounce_hb::BACKEND_ENV).filter(|v| !v.is_empty()));
    let engine = selection
        .as_deref()
        .map(ounce_hb::Engine::from_selection)
        .unwrap_or(ounce_hb::Engine::Core);
    let reason = match ounce_hb::run_engine(&engine, kind, &bytes, guest) {
        Ok(out) => {
            say_out(&out.stdout);
            say_err(&out.stderr);
            note = ounce_hb::diagnose::truncation_note(&out);
            ounce_hb::diagnose::from_output(&out)
        }
        Err(e) => ounce_hb::diagnose::from_error(&e),
    };
    say_err(&reason.explain_backend(path, guest, selection.as_deref()));
    if let Some(note) = note {
        say_err(&note);
    }
    ExitCode::from(reason.exit_code())
}

/// Writes to stdout. A write that fails, as it does when the reader of a pipe has gone away, is
/// reported on stderr once and otherwise ignored: the exit code is the diagnosis, and it must not
/// turn into a panic (exit 101) because nobody was listening.
fn say_out(text: &str) {
    let mut stdout = io::stdout().lock();
    if let Err(e) = stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        && e.kind() != io::ErrorKind::BrokenPipe
    {
        say_err(&format!("ounce-hb: could not write to stdout: {e}\n"));
    }
}

/// Writes to stderr, ignoring a failed write for the same reason as [`say_out`].
fn say_err(text: &str) {
    let mut stderr = io::stderr().lock();
    let _ = stderr.write_all(text.as_bytes());
    let _ = stderr.flush();
}
