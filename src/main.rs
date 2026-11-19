#![forbid(unsafe_code)]

use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Duration;

const USAGE: &str = "\
usage:
  ounce-hb run [--backend <core|runtime|auto|path>] [--cpu <interp|tier3|tier4>] [--sdmc <dir>] [--auto] [--timeout <secs>] [--json] [--expect-hash <hash>] [--expect-stdout <text>] <file.nro> [-- guest args...]
  ounce-hb repro [--backend <core|runtime|path>] [--cpu <interp|tier3|tier4>] [--sdmc <dir>] <bundle.json|dir>
  ounce-hb bisect [--backend <core|runtime|path>] [--cpu <interp|tier3|tier4>] [--sdmc <dir>] <bundle.json|dir>
  ounce-hb doctor [--json]";

enum Action<'a> {
    Doctor {
        json: bool,
    },
    Repro {
        path: &'a str,
        backend: Option<&'a str>,
        cpu: Option<ounce_hb::CpuTier>,
        sdmc: Option<&'a str>,
    },
    Bisect {
        path: &'a str,
        backend: Option<&'a str>,
        cpu: Option<ounce_hb::CpuTier>,
        sdmc: Option<&'a str>,
    },
    Run {
        path: &'a str,
        guest: &'a [String],
        backend: Option<&'a str>,
        auto: bool,
        opts: RunOpts<'a>,
    },
}

/// Options of `ounce-hb run` that do not select the engine.
#[derive(Clone, Copy, Default)]
struct RunOpts<'a> {
    /// `--timeout <secs>`; `None` defers to `OUNCE_HB_TIMEOUT_SECS`, then the default.
    timeout: Option<Duration>,
    /// `--json`: print one `ounce.hb.run.v1` object on stdout instead of the guest's output.
    json: bool,
    /// `--cpu <interp|tier3|tier4>`: forwarded as `--cpu=<tier>`.
    cpu: Option<ounce_hb::CpuTier>,
    /// `--sdmc <dir>`: host directory mounted at `sdmc:/`.
    sdmc: Option<&'a str>,
    /// `--expect-hash <hash>`: require `ReportV1::framebuffer_hash` to match this value.
    expect_hash: Option<&'a str>,
    /// `--expect-stdout <substring>`: require captured stdout to contain this substring.
    expect_stdout: Option<&'a str>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Action::Doctor { json }) => run_doctor(json),
        Ok(Action::Repro {
            path,
            backend,
            cpu,
            sdmc,
        }) => run_repro(path, backend, cpu, sdmc),
        Ok(Action::Bisect {
            path,
            backend,
            cpu,
            sdmc,
        }) => run_bisect(path, backend, cpu, sdmc),
        Ok(Action::Run {
            path,
            guest,
            backend,
            auto,
            opts,
        }) => run(path, guest, backend, auto, opts),
        Err(message) => {
            say_err(&format!("{message}\n"));
            ExitCode::from(2)
        }
    }
}

fn parse_cpu_arg(
    arg: &str,
    args: &[String],
    i: &mut usize,
    cpu: &mut Option<ounce_hb::CpuTier>,
) -> Result<(), String> {
    if cpu.is_some() {
        return Err(format!("ounce-hb: duplicate --cpu.\n{USAGE}"));
    }
    let value = if let Some(v) = arg.strip_prefix("--cpu=") {
        v
    } else {
        *i += 1;
        args.get(*i).map(String::as_str).unwrap_or("")
    };
    *cpu = Some(ounce_hb::parse_cpu(value).map_err(|e| format!("ounce-hb: {e}\n{USAGE}"))?);
    Ok(())
}

fn parse_sdmc_arg<'a>(
    arg: &'a str,
    args: &'a [String],
    i: &mut usize,
    sdmc: &mut Option<&'a str>,
) -> Result<(), String> {
    if sdmc.is_some() {
        return Err(format!("ounce-hb: duplicate --sdmc.\n{USAGE}"));
    }
    let value = if let Some(v) = arg.strip_prefix("--sdmc=") {
        Some(v).filter(|v| !v.is_empty())
    } else {
        *i += 1;
        args.get(*i)
            .map(String::as_str)
            .filter(|v| !v.is_empty() && !v.starts_with('-'))
    };
    if value.is_none() {
        return Err(format!(
            "ounce-hb: --sdmc requires a directory path.\n{USAGE}"
        ));
    }
    *sdmc = value;
    Ok(())
}

fn parse_value_arg<'a>(
    flag: &str,
    arg: &'a str,
    args: &'a [String],
    i: &mut usize,
    target: &mut Option<&'a str>,
    allow_leading_dash: bool,
) -> Result<(), String> {
    if target.is_some() {
        return Err(format!("ounce-hb: duplicate {flag}.\n{USAGE}"));
    }
    let prefix = format!("{flag}=");
    let value = if let Some(v) = arg.strip_prefix(&prefix) {
        Some(v).filter(|v| !v.is_empty())
    } else {
        *i += 1;
        args.get(*i)
            .map(String::as_str)
            .filter(|v| !v.is_empty() && (allow_leading_dash || !v.starts_with('-')))
    };
    if value.is_none() {
        return Err(format!("ounce-hb: {flag} requires a value.\n{USAGE}"));
    }
    *target = value;
    Ok(())
}

struct ReproArgs<'a> {
    path: &'a str,
    backend: Option<&'a str>,
    cpu: Option<ounce_hb::CpuTier>,
    sdmc: Option<&'a str>,
}

fn parse_repro_args(args: &[String]) -> Result<ReproArgs<'_>, String> {
    let mut path = None;
    let mut backend = None;
    let mut cpu = None;
    let mut sdmc = None;
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
            a if a == "--cpu" || a.starts_with("--cpu=") => {
                parse_cpu_arg(a, args, &mut i, &mut cpu)?;
            }
            a if a == "--sdmc" || a.starts_with("--sdmc=") => {
                parse_sdmc_arg(a, args, &mut i, &mut sdmc)?;
            }
            value if path.is_none() && !value.starts_with('-') => path = Some(value),
            other => {
                return Err(format!("ounce-hb: unexpected argument {other:?}.\n{USAGE}"));
            }
        }
        i += 1;
    }
    path.map(|path| ReproArgs {
        path,
        backend,
        cpu,
        sdmc,
    })
    .ok_or_else(|| USAGE.to_string())
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
            let parsed = parse_repro_args(args)?;
            return Ok(Action::Repro {
                path: parsed.path,
                backend: parsed.backend,
                cpu: parsed.cpu,
                sdmc: parsed.sdmc,
            });
        }
        Some("bisect") => {
            let parsed = parse_repro_args(args)?;
            return Ok(Action::Bisect {
                path: parsed.path,
                backend: parsed.backend,
                cpu: parsed.cpu,
                sdmc: parsed.sdmc,
            });
        }
        Some("run") => {}
        _ => return Err(USAGE.to_string()),
    }

    let mut path = None;
    let mut backend = None;
    let mut auto = false;
    let mut opts = RunOpts::default();
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
                        opts,
                    })
                    .ok_or_else(|| USAGE.to_string());
            }
            "--auto" => auto = true,
            "--json" => opts.json = true,
            "--timeout" => {
                if opts.timeout.is_some() {
                    return Err(format!("ounce-hb: duplicate --timeout.\n{USAGE}"));
                }
                i += 1;
                let value = args.get(i).map(String::as_str).unwrap_or("");
                opts.timeout = Some(
                    ounce_hb::parse_timeout_named(value, "--timeout")
                        .map_err(|e| format!("ounce-hb: {e}\n{USAGE}"))?,
                );
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
                        "ounce-hb: --backend requires core, runtime, auto, or a path.\n{USAGE}"
                    ));
                }
            }
            a if a == "--cpu" || a.starts_with("--cpu=") => {
                parse_cpu_arg(a, args, &mut i, &mut opts.cpu)?;
            }
            a if a == "--sdmc" || a.starts_with("--sdmc=") => {
                parse_sdmc_arg(a, args, &mut i, &mut opts.sdmc)?;
            }
            a if a == "--expect-hash" || a.starts_with("--expect-hash=") => {
                parse_value_arg(
                    "--expect-hash",
                    a,
                    args,
                    &mut i,
                    &mut opts.expect_hash,
                    false,
                )?;
            }
            a if a == "--expect-stdout" || a.starts_with("--expect-stdout=") => {
                parse_value_arg(
                    "--expect-stdout",
                    a,
                    args,
                    &mut i,
                    &mut opts.expect_stdout,
                    true,
                )?;
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
        opts,
    })
    .ok_or_else(|| USAGE.to_string())
}

fn probe_version(bin: &Path) -> Option<String> {
    let out = Command::new(bin).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| !l.trim().is_empty())?.trim();
    Some(line.to_string())
}

/// One homebrew-toolchain check shown by `doctor`. These are needed to *build* homebrew, not to run it.
struct ToolCheck {
    name: &'static str,
    ok: bool,
    detail: String,
    fix: &'static str,
}

fn toolchain_checks() -> Vec<ToolCheck> {
    let devkitpro = std::env::var_os("DEVKITPRO").filter(|v| !v.is_empty());
    let root = devkitpro.as_ref().map(std::path::PathBuf::from);
    let root_ok = root.as_deref().is_some_and(Path::is_dir);
    let gcc = root
        .as_ref()
        .map(|r| r.join("devkitA64/bin/aarch64-none-elf-gcc"))
        .filter(|p| p.is_file());
    let nx = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|d| d.join("cargo-nx"))
        .find(|p| p.is_file());
    vec![
        ToolCheck {
            name: "DEVKITPRO",
            ok: root_ok,
            detail: match (&root, root_ok) {
                (Some(r), true) => r.display().to_string(),
                (Some(r), false) => format!("set to {} but that is not a directory", r.display()),
                (None, _) => "not set".to_string(),
            },
            fix: "install devkitPro and export DEVKITPRO=/opt/devkitpro",
        },
        ToolCheck {
            name: "devkitA64",
            ok: gcc.is_some(),
            detail: gcc.map_or_else(
                || "aarch64-none-elf-gcc not found under $DEVKITPRO/devkitA64/bin".to_string(),
                |p| p.display().to_string(),
            ),
            fix: "dkp-pacman -S switch-dev",
        },
        ToolCheck {
            name: "cargo-nx",
            ok: nx.is_some(),
            detail: nx.map_or_else(
                || "not found on PATH".to_string(),
                |p| p.display().to_string(),
            ),
            fix: "cargo install cargo-nx",
        },
    ]
}

fn run_doctor(json: bool) -> ExitCode {
    let tools = toolchain_checks();
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
            "{{\n  \"schema\": \"ounce.doctor.v1\",\n  \"os\": \"{os}\",\n  \"arch\": \"{arch}\",\n  \"aarch64_hw_replay\": {hw_replay},\n  \"ounce_core\": {{\"found\": {}, \"path\": {core_p}, \"version\": {core_v}}},\n  \"ounce_runtime\": {{\"found\": {}, \"path\": {rt_p}, \"version\": {rt_v}}},\n  \"toolchain\": {{{}}}\n}}\n",
            core_path.is_some(),
            rt_path.is_some(),
            tools
                .iter()
                .map(|t| format!(
                    "\"{}\": {{\"ok\": {}, \"detail\": \"{}\"}}",
                    t.name,
                    t.ok,
                    esc(&t.detail)
                ))
                .collect::<Vec<_>>()
                .join(", ")
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
        say_out("  homebrew toolchain (only needed to build homebrew):\n");
        for t in &tools {
            if t.ok {
                say_out(&format!("    {:<10} ok: {}\n", t.name, t.detail));
            } else {
                say_out(&format!(
                    "    {:<10} MISSING: {} (fix: {})\n",
                    t.name, t.detail, t.fix
                ));
            }
        }
    }

    if core_path.is_some() || rt_path.is_some() {
        ExitCode::from(0)
    } else {
        ExitCode::from(3)
    }
}

fn resolve_manifest_path(manifest_path: &Path, target: &str) -> std::path::PathBuf {
    if Path::new(target).is_absolute() {
        std::path::PathBuf::from(target)
    } else {
        manifest_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join(target)
    }
}

struct ReproOutcome {
    passed: usize,
    failed: usize,
    infra_exit: Option<u8>,
}

fn execute_repro(
    path: &str,
    backend: Option<&str>,
    cpu: Option<ounce_hb::CpuTier>,
    sdmc: Option<&str>,
) -> ReproOutcome {
    if let Some(dir) = sdmc
        && let Err(e) = ounce_hb::validate_sdmc_dir(Path::new(dir))
    {
        say_err(&format!("ounce-hb: {e}\n"));
        return ReproOutcome {
            passed: 0,
            failed: 0,
            infra_exit: Some(2),
        };
    }

    let root = Path::new(path);
    let mut manifests = Vec::new();
    if root.is_dir() {
        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(e) => {
                say_err(&format!("ounce-hb: cannot read {path}: {e}\n"));
                return ReproOutcome {
                    passed: 0,
                    failed: 0,
                    infra_exit: Some(2),
                };
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
        return ReproOutcome {
            passed: 0,
            failed: 0,
            infra_exit: Some(2),
        };
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
    let mut infra_exit: Option<u8> = None;
    for manifest_path in &manifests {
        let raw = match std::fs::read_to_string(manifest_path) {
            Ok(r) => r,
            Err(e) => {
                say_err(&format!(
                    "ounce-hb: failed to read {}: {e}\n",
                    manifest_path.display()
                ));
                infra_exit.get_or_insert(2);
                failed += 1;
                continue;
            }
        };
        let Some(manifest) = ounce_hb::report_v1::ReproManifest::parse(&raw) else {
            say_err(&format!(
                "ounce-hb: {} is not a valid ounce.repro.v1 manifest\n",
                manifest_path.display()
            ));
            infra_exit.get_or_insert(2);
            failed += 1;
            continue;
        };

        let prog = resolve_manifest_path(manifest_path, &manifest.program_path);
        let bytes = match std::fs::read(&prog) {
            Ok(b) => b,
            Err(e) => {
                say_err(&format!(
                    "ounce-hb: cannot read program {}: {e}\n",
                    prog.display()
                ));
                infra_exit.get_or_insert(2);
                failed += 1;
                continue;
            }
        };
        let kind = match ounce_hb::classify(&bytes) {
            Ok(k) => k,
            Err(r) => {
                say_err(&format!("ounce-hb: rejected {}: {r}\n", prog.display()));
                infra_exit.get_or_insert(2);
                failed += 1;
                continue;
            }
        };
        let effective_cpu = cpu.or(manifest.cpu);
        let effective_sdmc = if let Some(dir) = sdmc {
            Some(std::path::PathBuf::from(dir))
        } else if let Some(ref m_sdmc) = manifest.sdmc {
            let resolved = resolve_manifest_path(manifest_path, m_sdmc);
            if let Err(e) = ounce_hb::validate_sdmc_dir(&resolved) {
                say_err(&format!("ounce-hb: {e}\n"));
                infra_exit.get_or_insert(2);
                failed += 1;
                continue;
            }
            Some(resolved)
        } else {
            None
        };
        let out = match ounce_hb::run_engine_opts(
            &engine,
            kind,
            &bytes,
            &manifest.guest_args,
            None,
            effective_cpu,
            effective_sdmc.as_deref(),
        ) {
            Ok(o) => o,
            Err(e) => {
                let code = ounce_hb::diagnose::from_error(&e).exit_code();
                say_err(&format!("ounce-hb: repro execution failed: {e}\n"));
                infra_exit.get_or_insert(code);
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

        // Only compared when the manifest recorded a hash; a missing actual hash is a failure, not a pass.
        let actual_hash = parsed_report
            .as_ref()
            .and_then(|r| r.framebuffer_hash.as_deref());
        let hash_ok = manifest
            .expected_framebuffer_hash
            .as_deref()
            .is_none_or(|exp| Some(exp) == actual_hash);
        let actual_frames = parsed_report.as_ref().map(|r| r.frames_presented);
        let frames_ok = manifest
            .expected_frames_presented
            .is_none_or(|exp| Some(exp) == actual_frames);

        if kind_ok && code_ok && hash_ok && frames_ok {
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
            if !hash_ok || !frames_ok {
                say_err(&format!(
                    "[repro]   framebuffer_hash expected {:?}, got {:?}; frames_presented expected {:?}, got {:?}\n",
                    manifest.expected_framebuffer_hash,
                    actual_hash,
                    manifest.expected_frames_presented,
                    actual_frames
                ));
            }
        }
    }

    ReproOutcome {
        passed,
        failed,
        infra_exit,
    }
}

fn run_repro(
    path: &str,
    backend: Option<&str>,
    cpu: Option<ounce_hb::CpuTier>,
    sdmc: Option<&str>,
) -> ExitCode {
    let outcome = execute_repro(path, backend, cpu, sdmc);
    if outcome.passed == 0 && outcome.failed == 0 {
        return ExitCode::from(outcome.infra_exit.unwrap_or(2));
    }
    if outcome.failed > 0 {
        say_err(&format!(
            "ounce-hb repro: {} passed, {} failed.\n",
            outcome.passed, outcome.failed
        ));
        ExitCode::from(1)
    } else {
        println!("ounce-hb repro: {} passed, 0 failed.", outcome.passed);
        ExitCode::from(0)
    }
}

fn run_bisect(
    path: &str,
    backend: Option<&str>,
    cpu: Option<ounce_hb::CpuTier>,
    sdmc: Option<&str>,
) -> ExitCode {
    let outcome = execute_repro(path, backend, cpu, sdmc);
    if let Some(code) = outcome.infra_exit
        && (2..=5).contains(&code)
    {
        say_err(&format!(
            "ounce-hb bisect: infrastructure/refusal error (exit {code}); mapping to 125 (git bisect skip).\n"
        ));
        return ExitCode::from(125);
    }
    if outcome.failed > 0 {
        say_err(&format!(
            "ounce-hb bisect: {} passed, {} failed (exit 1).\n",
            outcome.passed, outcome.failed
        ));
        ExitCode::from(1)
    } else {
        println!("ounce-hb bisect: {} passed, 0 failed.", outcome.passed);
        ExitCode::from(0)
    }
}

/// Prints the `--json` result of `ounce-hb run` (and nothing else) on stdout.
fn emit_run_json(
    path: &str,
    code: u8,
    stop: &str,
    out: Option<&ounce_hb::RunOutput>,
    opts: RunOpts<'_>,
) {
    let esc = ounce_hb::report_v1::escape_json;
    let guest_code = out
        .and_then(|o| o.exit_code)
        .map_or_else(|| "null".to_string(), |c| c.to_string());
    let timeout = opts
        .timeout
        .map_or_else(|| "null".to_string(), |d| d.as_secs_f64().to_string());
    say_out(&format!(
        "{{\"schema\": \"ounce.hb.run.v1\", \"path\": \"{}\", \"exit_code\": {code}, \"stop\": \"{stop}\", \"guest_exit_code\": {guest_code}, \"timeout_secs\": {timeout}, \"stdout\": \"{}\", \"stderr\": \"{}\", \"truncated\": {}}}\n",
        esc(path),
        esc(out.map_or("", |o| o.stdout.as_str())),
        esc(out.map_or("", |o| o.stderr.as_str())),
        out.is_some_and(ounce_hb::RunOutput::truncated),
    ));
}

fn run(
    path: &str,
    guest: &[String],
    backend: Option<&str>,
    auto: bool,
    opts: RunOpts<'_>,
) -> ExitCode {
    let wants_auto = auto || backend == Some("auto");
    if opts.json && wants_auto {
        say_err(
            "ounce-hb: --json is not supported with --auto yet (auto escalation prints several runs).\n",
        );
        return ExitCode::from(2);
    }
    if let Some(dir) = opts.sdmc
        && let Err(e) = ounce_hb::validate_sdmc_dir(Path::new(dir))
    {
        say_err(&format!("ounce-hb: {e}\n"));
        if opts.json {
            emit_run_json(path, 2, "refused", None, opts);
        }
        return ExitCode::from(2);
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            say_err(&format!("ounce-hb: cannot read {path}: {e}\n"));
            if opts.json {
                emit_run_json(path, 2, "refused", None, opts);
            }
            return ExitCode::from(2);
        }
    };
    let kind = match ounce_hb::classify(&bytes) {
        Ok(k) => k,
        Err(r) => {
            say_err(&format!("ounce-hb: rejected {path}: {r}\n"));
            if opts.json {
                emit_run_json(path, 2, "refused", None, opts);
            }
            return ExitCode::from(2);
        }
    };

    if wants_auto {
        return run_auto(path, kind, &bytes, guest, opts);
    }

    let mut note = None;
    let selection = backend
        .map(std::ffi::OsString::from)
        .or_else(|| std::env::var_os(ounce_hb::BACKEND_ENV).filter(|v| !v.is_empty()));
    let engine = selection
        .as_deref()
        .map(ounce_hb::Engine::from_selection)
        .unwrap_or(ounce_hb::Engine::Core);
    let mut captured = None;
    let mut captured_report = None;
    let reason = match ounce_hb::run_engine_opts(
        &engine,
        kind,
        &bytes,
        guest,
        opts.timeout,
        opts.cpu,
        opts.sdmc.map(Path::new),
    ) {
        Ok(out) => {
            if !opts.json {
                say_out(&out.stdout);
                say_err(&out.stderr);
            }
            note = ounce_hb::diagnose::truncation_note(&out);
            let reason = ounce_hb::diagnose::from_output(&out);
            captured_report = out
                .report_json
                .as_deref()
                .and_then(ounce_hb::report_v1::ReportV1::parse);
            captured = Some(out);
            reason
        }
        Err(e) => ounce_hb::diagnose::from_error(&e),
    };
    say_err(&reason.explain_backend(path, guest, selection.as_deref()));
    if let Some(note) = note {
        say_err(&note);
    }
    let expectations_ok = captured
        .as_ref()
        .is_none_or(|out| check_expectations(opts, out, captured_report.as_ref()));
    let (final_code, final_stop) = if reason.exit_code() == 0 && !expectations_ok {
        (1, "assertion_failed")
    } else {
        (reason.exit_code(), reason.label())
    };
    if opts.json {
        emit_run_json(path, final_code, final_stop, captured.as_ref(), opts);
    }
    ExitCode::from(final_code)
}

fn check_expectations(
    opts: RunOpts<'_>,
    out: &ounce_hb::RunOutput,
    report: Option<&ounce_hb::report_v1::ReportV1>,
) -> bool {
    let mut ok = true;
    if let Some(expected_hash) = opts.expect_hash {
        let actual_hash = report.and_then(|r| r.framebuffer_hash.as_deref());
        if actual_hash != Some(expected_hash) {
            say_err(&format!(
                "ounce-hb: assertion failed: expected framebuffer_hash {:?}, got {}.\n",
                expected_hash,
                actual_hash.map_or_else(|| "none".to_string(), |h| format!("{h:?}"))
            ));
            ok = false;
        }
    }
    if let Some(expected_stdout) = opts.expect_stdout
        && !out.stdout.contains(expected_stdout)
    {
        say_err(&format!(
            "ounce-hb: assertion failed: expected stdout to contain {expected_stdout:?}.\n"
        ));
        ok = false;
    }
    ok
}

fn finalize_auto_exit(
    opts: RunOpts<'_>,
    reason_code: u8,
    out: &ounce_hb::RunOutput,
    report: Option<&ounce_hb::report_v1::ReportV1>,
) -> ExitCode {
    let ok = check_expectations(opts, out, report);
    if reason_code == 0 && !ok {
        ExitCode::from(1)
    } else {
        ExitCode::from(reason_code)
    }
}

fn framebuffer_hashes_differ(
    rt_report: Option<&ounce_hb::report_v1::ReportV1>,
    core_report: Option<&ounce_hb::report_v1::ReportV1>,
) -> bool {
    match (
        rt_report.and_then(|r| r.framebuffer_hash.as_deref()),
        core_report.and_then(|r| r.framebuffer_hash.as_deref()),
    ) {
        (Some(rt_hash), Some(core_hash)) => rt_hash != core_hash,
        _ => false,
    }
}

fn run_auto(
    path: &str,
    kind: ounce_hb::Kind,
    bytes: &[u8],
    guest: &[String],
    opts: RunOpts<'_>,
) -> ExitCode {
    let runtime_available = ounce_hb::Engine::Runtime.locate().is_ok();
    if !runtime_available {
        let core_sel = Some(std::ffi::OsString::from("core"));
        let mut captured = None;
        let mut captured_report = None;
        let reason = match ounce_hb::run_engine_opts(
            &ounce_hb::Engine::Core,
            kind,
            bytes,
            guest,
            opts.timeout,
            opts.cpu,
            opts.sdmc.map(Path::new),
        ) {
            Ok(out) => {
                say_out(&out.stdout);
                say_err(&out.stderr);
                let reason = ounce_hb::diagnose::from_output(&out);
                captured_report = out
                    .report_json
                    .as_deref()
                    .and_then(ounce_hb::report_v1::ReportV1::parse);
                captured = Some(out);
                reason
            }
            Err(e) => ounce_hb::diagnose::from_error(&e),
        };
        say_err(&reason.explain_backend(path, guest, core_sel.as_deref()));
        return match captured {
            Some(ref out) => {
                finalize_auto_exit(opts, reason.exit_code(), out, captured_report.as_ref())
            }
            None => ExitCode::from(reason.exit_code()),
        };
    }

    let rt_res = ounce_hb::run_engine_opts(
        &ounce_hb::Engine::Runtime,
        kind,
        bytes,
        guest,
        opts.timeout,
        opts.cpu,
        opts.sdmc.map(Path::new),
    );
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
    let rt_silent_fallback = rt_report.as_ref().is_some_and(|r| r.silent_fallback);
    let rt_abnormal = matches!(rt_reason.exit_code(), 10..=15);
    let rt_has_hash = rt_report
        .as_ref()
        .is_some_and(|r| r.framebuffer_hash.is_some());
    let needs_escalation = rt_silent_fallback
        || rt_abnormal
        || (rt_has_hash && ounce_hb::Engine::Core.locate().is_ok());

    if !needs_escalation {
        say_out(&rt_out.stdout);
        say_err(&rt_out.stderr);
        say_err(&rt_reason.explain_backend(path, guest, Some(std::ffi::OsStr::new("runtime"))));
        return finalize_auto_exit(opts, rt_reason.exit_code(), &rt_out, rt_report.as_ref());
    }

    // Escalate to ounce-core for deterministic attribution (Guest Bug vs Runtime Divergence).
    if rt_silent_fallback || rt_abnormal {
        say_err(&format!(
            "ounce-hb: runtime stopped abnormally (code {}); escalating to ounce-core for deterministic attribution...\n",
            rt_reason.exit_code()
        ));
    } else {
        say_err("ounce-hb: runtime emitted framebuffer_hash; cross-checking with ounce-core...\n");
    }
    let core_out = match ounce_hb::run_engine_opts(
        &ounce_hb::Engine::Core,
        kind,
        bytes,
        guest,
        opts.timeout,
        opts.cpu,
        opts.sdmc.map(Path::new),
    ) {
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

    let hash_diverged = framebuffer_hashes_differ(rt_report.as_ref(), core_report.as_ref());
    if !hash_diverged && core_reason == rt_reason {
        if matches!(core_reason, ounce_hb::diagnose::StopReason::CleanExit) {
            return finalize_auto_exit(opts, 0, &core_out, core_report.as_ref());
        }
        say_err("ounce-hb: attribution = Guest Bug (runtime and ounce-core failed identically).\n");
        say_err(&core_reason.explain_backend(path, guest, Some(std::ffi::OsStr::new("core"))));
        return finalize_auto_exit(
            opts,
            core_reason.exit_code(),
            &core_out,
            core_report.as_ref(),
        );
    }

    let rt_hash = rt_report
        .as_ref()
        .and_then(|r| r.framebuffer_hash.as_deref())
        .unwrap_or("none");
    let core_hash = core_report
        .as_ref()
        .and_then(|r| r.framebuffer_hash.as_deref())
        .unwrap_or("none");
    if hash_diverged && core_reason == rt_reason {
        say_err(&format!(
            "ounce-hb: attribution = Runtime Divergence (framebuffer_hash {rt_hash:?} vs {core_hash:?}).\n"
        ));
    } else if hash_diverged {
        say_err(&format!(
            "ounce-hb: attribution = Runtime Divergence (runtime exit {} vs ounce-core exit {}, framebuffer_hash {rt_hash:?} vs {core_hash:?}).\n",
            rt_reason.exit_code(),
            core_reason.exit_code()
        ));
    } else {
        say_err(&format!(
            "ounce-hb: attribution = Runtime Divergence (runtime exit {} vs ounce-core exit {}).\n",
            rt_reason.exit_code(),
            core_reason.exit_code()
        ));
    }
    let bundle = ounce_hb::report_v1::format_repro_bundle_with_opts(
        path,
        bytes.len(),
        guest,
        opts.cpu,
        opts.sdmc,
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
    finalize_auto_exit(
        opts,
        core_reason.exit_code(),
        &core_out,
        core_report.as_ref(),
    )
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
