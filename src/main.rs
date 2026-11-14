#![forbid(unsafe_code)]

use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage:
  ounce-hb run [--backend <core|runtime|auto|path>] [--auto] <file.nro> [-- guest args...]
  ounce-hb repro [--backend <core|runtime|path>] <bundle.json|dir>
  ounce-hb doctor [--json]";

enum Action<'a> {
    Doctor {
        json: bool,
    },
    Repro {
        path: &'a str,
        backend: Option<&'a str>,
    },
    Run {
        path: &'a str,
        guest: &'a [String],
        backend: Option<&'a str>,
        auto: bool,
    },
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Action::Doctor { json }) => run_doctor(json),
        Ok(Action::Repro { path, backend }) => run_repro(path, backend),
        Ok(Action::Run {
            path,
            guest,
            backend,
            auto,
        }) => run(path, guest, backend, auto),
        Err(message) => {
            say_err(&format!("{message}\n"));
            ExitCode::from(2)
        }
    }
}

fn parse(args: &[String]) -> Result<Action<'_>, String> {
    match args.first().map(String::as_str) {
        Some("doctor") => {
            let mut json = false;
            for arg in &args[1..] {
                match arg.as_str() {
                    "--json" => json = true,
                    other => {
                        return Err(format!("ounce-hb: unexpected argument {other:?}.\n{USAGE}"));
                    }
                }
            }
            return Ok(Action::Doctor { json });
        }
        Some("repro") => {
            let mut path = None;
            let mut backend = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--backend" => {
                        if backend.is_some() {
                            return Err(format!("ounce-hb: duplicate --backend.\n{USAGE}"));
                        }
                        i += 1;
                        backend = args
                            .get(i)
                            .map(String::as_str)
                            .filter(|v| !v.is_empty() && !v.starts_with('-'));
                        if backend.is_none() {
                            return Err(format!(
                                "ounce-hb: --backend requires core, runtime, auto, or a path.\n{USAGE}"
                            ));
                        }
                    }
                    value if path.is_none() && !value.starts_with('-') => path = Some(value),
                    other => {
                        return Err(format!("ounce-hb: unexpected argument {other:?}.\n{USAGE}"));
                    }
                }
                i += 1;
            }
            return path
                .map(|p| Action::Repro { path: p, backend })
                .ok_or_else(|| USAGE.to_string());
        }
        Some("run") => {}
        _ => return Err(USAGE.to_string()),
    }

    let mut path = None;
    let mut backend = None;
    let mut auto = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--" => {
                return path
                    .map(|p| Action::Run {
                        path: p,
                        guest: &args[i + 1..],
                        backend,
                        auto,
                    })
                    .ok_or_else(|| USAGE.to_string());
            }
            "--auto" => auto = true,
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
                        "ounce-hb: --backend requires core, runtime, auto, or a path.\n{USAGE}"
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
    path.map(|p| Action::Run {
        path: p,
        guest: &args[args.len()..],
        backend,
        auto,
    })
    .ok_or_else(|| USAGE.to_string())
}

fn probe_version(bin: &Path) -> Option<String> {
    let out = Command::new(bin).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| !l.trim().is_empty())?.trim();
    Some(line.to_string())
}

fn run_doctor(json: bool) -> ExitCode {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let hw_replay = arch == "aarch64" && matches!(os, "macos" | "linux");

    let core_path = ounce_hb::locate_core().ok();
    let core_ver = core_path.as_deref().and_then(probe_version);

    let rt_path = ounce_hb::find_runtime(
        std::env::var_os(ounce_hb::RUNTIME_BIN_ENV),
        std::env::var_os("PATH"),
    )
    .ok();
    let rt_ver = rt_path.as_deref().and_then(probe_version);

    if json {
        let esc = ounce_hb::report_v1::escape_json;
        let core_p = core_path
            .as_ref()
            .map(|p| format!("\"{}\"", esc(&p.display().to_string())))
            .unwrap_or_else(|| "null".to_string());
        let core_v = core_ver
            .as_ref()
            .map(|v| format!("\"{}\"", esc(v)))
            .unwrap_or_else(|| "null".to_string());
        let rt_p = rt_path
            .as_ref()
            .map(|p| format!("\"{}\"", esc(&p.display().to_string())))
            .unwrap_or_else(|| "null".to_string());
        let rt_v = rt_ver
            .as_ref()
            .map(|v| format!("\"{}\"", esc(v)))
            .unwrap_or_else(|| "null".to_string());
        say_out(&format!(
            "{{\n  \"schema\": \"ounce.doctor.v1\",\n  \"os\": \"{os}\",\n  \"arch\": \"{arch}\",\n  \"aarch64_hw_replay\": {hw_replay},\n  \"ounce_core\": {{\"found\": {}, \"path\": {core_p}, \"version\": {core_v}}},\n  \"ounce_runtime\": {{\"found\": {}, \"path\": {rt_p}, \"version\": {rt_v}}}\n}}\n",
            core_path.is_some(),
            rt_path.is_some()
        ));
    } else {
        say_out(&format!(
            "ounce-hb doctor (v{})\n  host              : {os}/{arch}\n  aarch64 hw replay : {}\n  ounce-core        : {}\n  ounce-runtime     : {}\n",
            env!("CARGO_PKG_VERSION"),
            if hw_replay {
                "available"
            } else {
                "unavailable (non-aarch64 host)"
            },
            match (&core_path, &core_ver) {
                (Some(p), Some(v)) => format!("{} ({v})", p.display()),
                (Some(p), None) => p.display().to_string(),
                (None, _) => "not found (set OUNCE_CORE_BIN or add to PATH)".to_string(),
            },
            match (&rt_path, &rt_ver) {
                (Some(p), Some(v)) => format!("{} ({v})", p.display()),
                (Some(p), None) => p.display().to_string(),
                (None, _) => "not found (optional; set OUNCE_RUNTIME_BIN)".to_string(),
            }
        ));
    }

    if core_path.is_some() || rt_path.is_some() {
        ExitCode::from(0)
    } else {
        ExitCode::from(3)
    }
}

fn run_repro(path: &str, backend: Option<&str>) -> ExitCode {
    let root = Path::new(path);
    let mut manifests = Vec::new();
    if root.is_dir() {
        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(e) => {
                say_err(&format!("ounce-hb: cannot read {path}: {e}\n"));
                return ExitCode::from(2);
            }
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".repro.json") || n == "ounce-repro.json")
            {
                manifests.push(p);
            }
        }
        manifests.sort();
    } else {
        manifests.push(root.to_path_buf());
    }

    if manifests.is_empty() {
        say_err(&format!("ounce-hb: no repro manifests found at {path}\n"));
        return ExitCode::from(2);
    }

    let selection = backend
        .map(std::ffi::OsString::from)
        .or_else(|| std::env::var_os(ounce_hb::BACKEND_ENV).filter(|v| !v.is_empty()));
    let engine = selection
        .as_deref()
        .map(ounce_hb::Engine::from_selection)
        .unwrap_or(ounce_hb::Engine::Core);

    let mut passed = 0usize;
    let mut failed = 0usize;
    for manifest_path in &manifests {
        let raw = match std::fs::read_to_string(manifest_path) {
            Ok(r) => r,
            Err(e) => {
                say_err(&format!(
                    "ounce-hb: failed to read {}: {e}\n",
                    manifest_path.display()
                ));
                failed += 1;
                continue;
            }
        };
        let Some(manifest) = ounce_hb::report_v1::ReproManifest::parse(&raw) else {
            say_err(&format!(
                "ounce-hb: {} is not a valid ounce.repro.v1 manifest\n",
                manifest_path.display()
            ));
            failed += 1;
            continue;
        };

        let prog = if Path::new(&manifest.program_path).is_absolute() {
            std::path::PathBuf::from(&manifest.program_path)
        } else {
            manifest_path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .join(&manifest.program_path)
        };
        let bytes = match std::fs::read(&prog) {
            Ok(b) => b,
            Err(e) => {
                say_err(&format!(
                    "ounce-hb: cannot read program {}: {e}\n",
                    prog.display()
                ));
                failed += 1;
                continue;
            }
        };
        let kind = match ounce_hb::classify(&bytes) {
            Ok(k) => k,
            Err(r) => {
                say_err(&format!("ounce-hb: rejected {}: {r}\n", prog.display()));
                failed += 1;
                continue;
            }
        };
        let out = match ounce_hb::run_engine(&engine, kind, &bytes, &manifest.guest_args) {
            Ok(o) => o,
            Err(e) => {
                say_err(&format!("ounce-hb: repro execution failed: {e}\n"));
                failed += 1;
                continue;
            }
        };
        let parsed_report = out
            .report_json
            .as_deref()
            .and_then(ounce_hb::report_v1::ReportV1::parse);
        let actual_kind = parsed_report
            .as_ref()
            .map(|r| r.stop_kind.as_str())
            .unwrap_or("");
        let actual_code = parsed_report
            .as_ref()
            .map(|r| r.exit_code)
            .or(out.exit_code);

        let kind_ok = manifest
            .expected_stop_kind
            .as_deref()
            .is_none_or(|exp| exp == actual_kind);
        let code_ok = manifest
            .expected_exit_code
            .is_none_or(|exp| Some(exp) == actual_code);

        if kind_ok && code_ok {
            passed += 1;
            println!(
                "[repro] PASS {} (stop_kind={}, exit_code={:?})",
                manifest_path.display(),
                actual_kind,
                actual_code
            );
        } else {
            failed += 1;
            say_err(&format!(
                "[repro] FAIL {}: expected stop_kind={:?} exit_code={:?}, got stop_kind={:?} exit_code={:?}\n",
                manifest_path.display(),
                manifest.expected_stop_kind,
                manifest.expected_exit_code,
                actual_kind,
                actual_code
            ));
        }
    }

    if failed > 0 {
        say_err(&format!(
            "ounce-hb repro: {passed} passed, {failed} failed.\n"
        ));
        ExitCode::from(1)
    } else {
        println!("ounce-hb repro: {passed} passed, 0 failed.");
        ExitCode::from(0)
    }
}

fn run(path: &str, guest: &[String], backend: Option<&str>, auto: bool) -> ExitCode {
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

    if auto || backend == Some("auto") {
        return run_auto(path, kind, &bytes, guest);
    }

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

fn run_auto(path: &str, kind: ounce_hb::Kind, bytes: &[u8], guest: &[String]) -> ExitCode {
    let runtime_available = ounce_hb::Engine::Runtime.locate().is_ok();
    if !runtime_available {
        let core_sel = Some(std::ffi::OsString::from("core"));
        let reason = match ounce_hb::run_engine(&ounce_hb::Engine::Core, kind, bytes, guest) {
            Ok(out) => {
                say_out(&out.stdout);
                say_err(&out.stderr);
                ounce_hb::diagnose::from_output(&out)
            }
            Err(e) => ounce_hb::diagnose::from_error(&e),
        };
        say_err(&reason.explain_backend(path, guest, core_sel.as_deref()));
        return ExitCode::from(reason.exit_code());
    }

    let rt_res = ounce_hb::run_engine(&ounce_hb::Engine::Runtime, kind, bytes, guest);
    let rt_out = match rt_res {
        Ok(out) => out,
        Err(e) => {
            let reason = ounce_hb::diagnose::from_error(&e);
            say_err(&reason.explain_backend(path, guest, Some(std::ffi::OsStr::new("runtime"))));
            return ExitCode::from(reason.exit_code());
        }
    };
    let rt_report = rt_out
        .report_json
        .as_deref()
        .and_then(ounce_hb::report_v1::ReportV1::parse);
    let rt_reason = ounce_hb::diagnose::from_output(&rt_out);
    let needs_escalation = rt_report.as_ref().is_some_and(|r| r.silent_fallback)
        || matches!(rt_reason.exit_code(), 10..=15);

    if !needs_escalation {
        say_out(&rt_out.stdout);
        say_err(&rt_out.stderr);
        say_err(&rt_reason.explain_backend(path, guest, Some(std::ffi::OsStr::new("runtime"))));
        return ExitCode::from(rt_reason.exit_code());
    }

    // Escalate to ounce-core for deterministic attribution (Guest Bug vs Runtime Divergence).
    say_err(&format!(
        "ounce-hb: runtime stopped abnormally (code {}); escalating to ounce-core for deterministic attribution...\n",
        rt_reason.exit_code()
    ));
    let core_out = match ounce_hb::run_engine(&ounce_hb::Engine::Core, kind, bytes, guest) {
        Ok(out) => out,
        Err(e) => {
            let reason = ounce_hb::diagnose::from_error(&e);
            say_err(&reason.explain_backend(path, guest, Some(std::ffi::OsStr::new("core"))));
            return ExitCode::from(reason.exit_code());
        }
    };
    let core_report = core_out
        .report_json
        .as_deref()
        .and_then(ounce_hb::report_v1::ReportV1::parse);
    let core_reason = ounce_hb::diagnose::from_output(&core_out);

    say_out(&core_out.stdout);
    say_err(&core_out.stderr);

    if core_reason == rt_reason {
        say_err("ounce-hb: attribution = Guest Bug (runtime and ounce-core failed identically).\n");
        say_err(&core_reason.explain_backend(path, guest, Some(std::ffi::OsStr::new("core"))));
        return ExitCode::from(core_reason.exit_code());
    }

    say_err(&format!(
        "ounce-hb: attribution = Runtime Divergence (runtime exit {} vs ounce-core exit {}).\n",
        rt_reason.exit_code(),
        core_reason.exit_code()
    ));
    let bundle = ounce_hb::report_v1::format_repro_bundle(
        path,
        bytes.len(),
        guest,
        rt_report.as_ref(),
        core_report.as_ref(),
        rt_out.exit_code,
        core_out.exit_code,
    );
    if std::fs::write("ounce-repro.json", &bundle).is_ok() {
        say_err("ounce-hb: wrote repro bundle to ounce-repro.json\n");
    }
    say_err(&format!(
        "ounce-hb: next: run `ounce-core diverge {path}` on an AArch64 host to pinpoint the diverging instruction.\n"
    ));
    ExitCode::from(core_reason.exit_code())
}

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

fn say_err(text: &str) {
    let mut stderr = io::stderr().lock();
    let _ = stderr.write_all(text.as_bytes());
    let _ = stderr.flush();
}
