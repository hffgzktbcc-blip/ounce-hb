#![forbid(unsafe_code)]

use std::process::ExitCode;

const USAGE: &str = "usage: ounce-hb run <file.nro> [-- guest args...]";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("run"), Some(path)) => {
            let guest: Vec<String> = args.skip_while(|a| a != "--").skip(1).collect();
            run(&path, &guest)
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run(path: &str, guest: &[String]) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("ounce-hb: cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let kind = match ounce_hb::classify(&bytes) {
        Ok(k) => k,
        Err(r) => {
            eprintln!("ounce-hb: rejected {path}: {r}");
            return ExitCode::from(2);
        }
    };
    match ounce_hb::run(kind, &bytes, guest) {
        Ok(out) => {
            print!("{}", out.stdout);
            eprint!("{}", out.stderr);
            match out.exit_code {
                Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
                None => {
                    eprintln!("ounce-hb: ounce-core was killed by a signal");
                    ExitCode::from(1)
                }
            }
        }
        Err(e) => {
            eprintln!("ounce-hb: {e}");
            ExitCode::from(3)
        }
    }
}
