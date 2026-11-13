#![forbid(unsafe_code)]

use std::io::{self, Write};
use std::process::ExitCode;

const USAGE: &str = "usage: ounce-hb run <file.nro> [-- guest args...]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok((path, guest)) => run(path, guest),
        Err(message) => {
            say_err(&format!("{message}\n"));
            ExitCode::from(2)
        }
    }
}

/// Splits the command line into the program path and the guest arguments. Only `run <path>` and
/// `run <path> -- <guest args...>` are accepted: anything else between the path and `--` is an
/// error, not something to drop quietly (an option meant for ounce-hb would otherwise vanish).
fn parse(args: &[String]) -> Result<(&str, &[String]), String> {
    match args {
        [command, path, rest @ ..] if command == "run" => match rest {
            [] => Ok((path, &[])),
            [separator, guest @ ..] if separator == "--" => Ok((path, guest)),
            [extra, ..] => Err(format!(
                "ounce-hb: unexpected argument {extra:?} after the program path. ounce-hb run takes no options; guest arguments go after '--'.\n{USAGE}"
            )),
        },
        _ => Err(USAGE.to_string()),
    }
}

fn run(path: &str, guest: &[String]) -> ExitCode {
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
    let reason = match ounce_hb::run(kind, &bytes, guest) {
        Ok(out) => {
            say_out(&out.stdout);
            say_err(&out.stderr);
            note = ounce_hb::diagnose::truncation_note(&out);
            ounce_hb::diagnose::from_output(&out)
        }
        Err(e) => ounce_hb::diagnose::from_error(&e),
    };
    say_err(&reason.explain(path, guest));
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
