//! Cross-backend end-to-end parity tests for `--backend core`, `--backend runtime`, and
//! `--backend <path>` on `libnx_hello.nro` and clean-room synthetic NRO0 fixtures.
//!
//! By default, `cargo test` runs self-contained using clean-room stand-in runner executables that
//! enforce ounce-hb's documented `run-nro <staged.nro> --present=headless [guest args...]`
//! subprocess contract and stop-line protocol in `docs/DIAGNOSIS.md`.
//!
//! Opt-in real-binary parity: when `OUNCE_REAL_CORE_BIN` (or `OUNCE_CORE_BIN`) and/or
//! `OUNCE_REAL_RUNTIME_BIN` are set, the opt-in test compares `ounce-hb --backend core` against
//! `ounce-hb --backend runtime` on `libnx_hello.nro` and synthetic clean NROs. Because local
//! `ounce-runtime` v0.4.0 implements `run <file>` rather than `run-nro`, the test wraps
//! `OUNCE_REAL_RUNTIME_BIN` in a small shim that asserts `ounce-hb` sent `run-nro` and translates
//! it to `run`. When those variables are unset, the opt-in test prints a `SKIPPED` line and passes
//! without inspecting external paths.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use ounce_hb::{
    BACKEND_ENV, CORE_BIN_ENV, Engine, MAX_GUEST_ARGS_BYTES, RUNTIME_BIN_ENV, RunError,
};

const LIBNX_HELLO_BYTES: &[u8] = include_bytes!("fixtures/libnx_hello/libnx_hello.nro");
const LIBNX_HELLO_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/libnx_hello/libnx_hello.nro"
);

const REAL_CORE_ENV: &str = "OUNCE_REAL_CORE_BIN";
const REAL_RUNTIME_ENV: &str = "OUNCE_REAL_RUNTIME_BIN";

// AArch64 encodings used to assemble minimal clean-room NRO0 test fixtures (ARMv8-A / switchbrew
// NRO0 specification), matching `tests/ounce_core_e2e.rs`.
const MOV_W0_0: u32 = 0x5280_0000; // mov w0, #0
const SVC_EXIT_PROCESS: u32 = 0xD400_00E1; // svc #7 (svcExitProcess)
const SVC_OUTPUT_DEBUG_STRING: u32 = 0xD400_04E1; // svc #0x27
const SVC_BREAK: u32 = 0xD400_04C1; // svc #0x26
const SVC_UNHANDLED: u32 = 0xD400_0FC1; // svc #0x7E
const LDR_X0_X1: u32 = 0xF940_0020; // ldr x0, [x1]
const BRK_0: u32 = 0xD420_0000; // brk #0
const ADD_X1_SP_0: u32 = 0x9100_03E1; // add x1, sp, #0
const MOV_X2_4: u32 = 0xD280_0082; // mov x2, #4
const SPIN: u32 = 0x1400_0000; // b .

const CLEAN_LINE: &str = "[VM] Process exited gracefully (svcExitProcess).";

static NEXT_HARNESS: AtomicUsize = AtomicUsize::new(0);

fn adr_x0(offset: u32) -> u32 {
    0x1000_0000 | ((offset & 3) << 29) | (((offset >> 2) & 0x7FFFF) << 5)
}

fn mov_x1(imm: u32) -> u32 {
    0xD280_0001 | (imm << 5)
}

fn synthetic_nro(words: &[u32], text_size_override: Option<u32>) -> Vec<u8> {
    synthetic_nro_with_data(words, b"", text_size_override)
}

fn synthetic_nro_with_data(words: &[u32], data: &[u8], text_size_override: Option<u32>) -> Vec<u8> {
    let mut text: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    text.extend_from_slice(data);
    let total = 0x80 + text.len();
    let mut bytes = vec![0u8; total];
    bytes[0x00..0x04].copy_from_slice(&0x1400_0020u32.to_le_bytes()); // b #0x80
    bytes[0x10..0x14].copy_from_slice(b"NRO0");
    bytes[0x18..0x1C].copy_from_slice(&(total as u32).to_le_bytes());
    bytes[0x24..0x28].copy_from_slice(&(text_size_override.unwrap_or(total as u32)).to_le_bytes());
    bytes[0x80..].copy_from_slice(&text);
    bytes
}

fn spoof_nro(message: &[u8], then: &[u32]) -> Vec<u8> {
    let mut words = vec![0, mov_x1(message.len() as u32), SVC_OUTPUT_DEBUG_STRING];
    words.extend_from_slice(then);
    words[0] = adr_x0(4 * words.len() as u32);
    synthetic_nro_with_data(&words, message, None)
}

struct Harness {
    root: PathBuf,
}

impl Harness {
    fn new(tag: &str) -> Self {
        let id = NEXT_HARNESS.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("ounce-hb-rt-e2e-{tag}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("tmp")).unwrap();
        Self { root }
    }

    fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    fn write_file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, bytes).unwrap();
        path
    }

    fn hb(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ounce-hb"));
        let tmp = self.tmp_dir();
        cmd.current_dir(&self.root)
            .env_remove(BACKEND_ENV)
            .env_remove(CORE_BIN_ENV)
            .env_remove(RUNTIME_BIN_ENV)
            .env_remove(ounce_hb::TIMEOUT_ENV)
            .env("TMPDIR", &tmp)
            .env("TMP", &tmp)
            .env("TEMP", &tmp);
        cmd
    }

    fn stand_in(&self, rel: &str, tag: &str, spec: &RunnerSpec<'_>) -> StandIn {
        let argv_file = self.root.join(format!("{tag}.argv"));
        let staged_copy = self.root.join(format!("{tag}.staged.nro"));
        let _ = fs::remove_file(&argv_file);
        let _ = fs::remove_file(&staged_copy);
        let bin = write_stand_in(&self.root, rel, &argv_file, &staged_copy, spec);
        StandIn {
            bin,
            argv_file,
            staged_copy,
            tmp_dir: self.tmp_dir(),
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Default)]
struct RunnerSpec<'a> {
    stdout_lines: &'a [&'a str],
    stderr_lines: &'a [&'a str],
    echo_libnx_hello: bool,
    exit_code: i32,
    sleep_secs: Option<u32>,
    marker: Option<&'a Path>,
}

struct StandIn {
    bin: PathBuf,
    argv_file: PathBuf,
    staged_copy: PathBuf,
    tmp_dir: PathBuf,
}

impl StandIn {
    fn recorded_argv(&self) -> Vec<String> {
        let raw = fs::read(&self.argv_file)
            .unwrap_or_else(|e| panic!("missing argv capture {}: {e}", self.argv_file.display()));
        #[cfg(unix)]
        {
            raw.split(|&b| b == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .collect()
        }
        #[cfg(not(unix))]
        {
            String::from_utf8_lossy(&raw)
                .lines()
                .map(|line| line.trim_end_matches('\r').to_string())
                .collect()
        }
    }

    fn assert_contract(&self, expected_nro: &[u8], expected_guest_args: &[&str]) {
        let argv = self.recorded_argv();
        assert!(
            argv.len() >= 3,
            "expected at least 3 runner args, got {argv:?}"
        );
        assert_eq!(argv[0], "run-nro", "argv={argv:?}");
        let staged_path = Path::new(&argv[1]);
        assert_eq!(
            staged_path.file_name(),
            Some(OsStr::new("program.nro")),
            "expected staged program.nro path, got {argv:?}"
        );
        assert!(
            staged_path.starts_with(&self.tmp_dir),
            "expected staged path inside {}, got {}",
            self.tmp_dir.display(),
            staged_path.display()
        );
        assert_eq!(argv[2], "--present=headless", "argv={argv:?}");
        let actual_guest: Vec<&str> = argv[3..].iter().map(String::as_str).collect();
        assert_eq!(actual_guest, expected_guest_args, "argv={argv:?}");

        let staged_bytes = fs::read(&self.staged_copy).unwrap();
        assert_eq!(
            staged_bytes, expected_nro,
            "staged copy bytes differed from input NRO"
        );
    }
}

#[cfg(unix)]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn cmd_escape_echo(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        match ch {
            '%' => out.push_str("%%"),
            '^' | '&' | '<' | '>' | '|' => {
                out.push('^');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(unix)]
fn write_stand_in(
    root: &Path,
    rel: &str,
    argv_file: &Path,
    staged_copy: &Path,
    spec: &RunnerSpec<'_>,
) -> PathBuf {
    use std::fmt::Write as _;
    use std::os::unix::fs::PermissionsExt;

    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    let mut script = String::from("#!/bin/sh\n");
    script.push_str("[ \"$1\" = run-nro ] || exit 90\n");
    script.push_str("[ -f \"$2\" ] || exit 91\n");
    script.push_str("[ \"$3\" = --present=headless ] || exit 92\n");
    let _ = writeln!(
        script,
        "if [ -x /bin/cp ]; then CP=/bin/cp; elif [ -x /usr/bin/cp ]; then CP=/usr/bin/cp; else CP=cp; fi\n\
         \"$CP\" \"$2\" {} || exit 93",
        sh_quote(&staged_copy.to_string_lossy())
    );
    let _ = writeln!(
        script,
        "printf '%s\\0' \"$@\" > {} || exit 94",
        sh_quote(&argv_file.to_string_lossy())
    );
    if let Some(marker) = spec.marker {
        let _ = writeln!(script, ": > {}", sh_quote(&marker.to_string_lossy()));
    }
    if let Some(secs) = spec.sleep_secs {
        let _ = writeln!(script, "exec /bin/sleep {secs}");
    }
    if spec.echo_libnx_hello {
        script.push_str(
            "printf 'Hello from ounce-hb!\\n'\n\
             shift 3\n\
             idx=1\n\
             for arg in \"$@\"; do\n\
               printf 'arg[%d]: %s\\n' \"$idx\" \"$arg\"\n\
               idx=$((idx + 1))\n\
             done\n",
        );
    }
    for line in spec.stdout_lines {
        let _ = writeln!(script, "printf '%s\\n' {}", sh_quote(line));
    }
    for line in spec.stderr_lines {
        let _ = writeln!(script, "printf '%s\\n' {} >&2", sh_quote(line));
    }
    let _ = writeln!(script, "exit {}", spec.exit_code);

    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(windows)]
fn write_stand_in(
    root: &Path,
    rel: &str,
    argv_file: &Path,
    staged_copy: &Path,
    spec: &RunnerSpec<'_>,
) -> PathBuf {
    use std::fmt::Write as _;

    let rel_cmd = if rel.ends_with(".cmd") {
        rel.to_string()
    } else {
        format!("{rel}.cmd")
    };
    let path = root.join(rel_cmd);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    let mut script = String::from("@echo off\r\n");
    script.push_str("if not \"%~1\"==\"run-nro\" exit /b 90\r\n");
    script.push_str("if not exist \"%~2\" exit /b 91\r\n");
    script.push_str("if not \"%~3\"==\"--present=headless\" exit /b 92\r\n");
    let _ = writeln!(
        script,
        "copy /y \"%~2\" \"{}\" >nul\r\nif errorlevel 1 exit /b 93\r",
        staged_copy.display()
    );
    let _ = writeln!(
        script,
        ">\"{}\" echo %~1\r\n>>\"{}\" echo %~2\r\n>>\"{}\" echo %~3\r",
        argv_file.display(),
        argv_file.display(),
        argv_file.display()
    );
    if let Some(marker) = spec.marker {
        let _ = writeln!(script, "type nul > \"{}\"\r", marker.display());
    }
    script.push_str("shift\r\nshift\r\nshift\r\nset idx=0\r\n:argloop\r\nif \"%~1\"==\"\" goto argdone\r\nset /a idx+=1\r\n");
    let _ = writeln!(script, ">>\"{}\" echo %~1\r", argv_file.display());
    if spec.echo_libnx_hello {
        script.push_str("call :emit_guest_arg \"%~1\"\r\n");
    }
    script.push_str("shift\r\ngoto argloop\r\n:argdone\r\n");
    if spec.echo_libnx_hello {
        script.push_str("echo Hello from ounce-hb!\r\n");
        // Re-echo guest args in order after the greeting by shifting would be lost, so emit them
        // after printing the greeting in a small buffered temp file if needed; simpler: read from
        // argv_file lines > 3 or emit greeting before loop! We instead print greeting before loop
        // when echo_libnx_hello is set.
    }
    if let Some(secs) = spec.sleep_secs {
        let _ = writeln!(script, "ping -n {} 127.0.0.1 >nul\r", secs + 1);
    }
    for line in spec.stdout_lines {
        if line.is_empty() {
            script.push_str("echo.\r\n");
        } else {
            let _ = writeln!(script, "echo {}\r", cmd_escape_echo(line));
        }
    }
    for line in spec.stderr_lines {
        if line.is_empty() {
            script.push_str(">&2 echo.\r\n");
        } else {
            let _ = writeln!(script, ">&2 echo {}\r", cmd_escape_echo(line));
        }
    }
    let _ = writeln!(script, "exit /b {}\r", spec.exit_code);
    if spec.echo_libnx_hello {
        script.push_str(":emit_guest_arg\r\necho arg[%idx%]: %~1\r\nexit /b 0\r\n");
    }

    // On Windows, move `Hello from ounce-hb!` before `:argloop` so greeting precedes `arg[i]: ...`.
    if spec.echo_libnx_hello {
        let marker = "set idx=0\r\n";
        script = script
            .replacen("echo Hello from ounce-hb!\r\n", "", 1)
            .replacen(marker, "echo Hello from ounce-hb!\r\nset idx=0\r\n", 1);
    }

    fs::write(&path, script).unwrap();
    path
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn assert_clean_libnx_stdout(stdout: &str, guest_args: &[&str]) {
    let norm = normalize_newlines(stdout);
    assert!(norm.contains("Hello from ounce-hb!\n"), "stdout:\n{stdout}");
    assert!(
        norm.contains(&format!("{CLEAN_LINE}\n")),
        "stdout:\n{stdout}"
    );
    if guest_args.is_empty() {
        assert!(!norm.contains("arg[1]:"), "stdout:\n{stdout}");
    } else {
        for (i, arg) in guest_args.iter().enumerate() {
            let expected = format!("arg[{}]: {arg}\n", i + 1);
            assert!(
                norm.contains(&expected),
                "missing {expected:?} in:\n{stdout}"
            );
        }
    }
}

#[test]
fn libnx_hello_parity_across_core_runtime_and_custom_path() {
    let h = Harness::new("libnx-parity");
    let spec = RunnerSpec {
        stdout_lines: &[CLEAN_LINE],
        echo_libnx_hello: true,
        exit_code: 0,
        ..Default::default()
    };

    for guest_args in [&[][..], &["test", "alpha"][..], &["hello world"][..]] {
        let core_bin = h.stand_in("core-runner", &format!("core-{}", guest_args.len()), &spec);
        let rt_bin = h.stand_in("runtime-runner", &format!("rt-{}", guest_args.len()), &spec);
        let custom_bin = h.stand_in(
            "custom dir/my runtime",
            &format!("custom-{}", guest_args.len()),
            &spec,
        );

        let run_mode = |mode: &[&str], env_vars: &[(&str, &OsStr)]| -> Output {
            let mut cmd = h.hb();
            cmd.arg("run").arg(LIBNX_HELLO_PATH).args(mode);
            if !guest_args.is_empty() {
                cmd.arg("--").args(guest_args);
            }
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
            cmd.output().unwrap()
        };

        let out_core = run_mode(
            &["--backend", "core"],
            &[(CORE_BIN_ENV, core_bin.bin.as_os_str())],
        );
        let out_runtime = run_mode(
            &["--backend", "runtime"],
            &[(RUNTIME_BIN_ENV, rt_bin.bin.as_os_str())],
        );
        // Also verify option placement before the NRO path (`ounce-hb run --backend runtime <nro>`).
        let mut before_cmd = h.hb();
        before_cmd.env(RUNTIME_BIN_ENV, &rt_bin.bin).args([
            "run",
            "--backend",
            "runtime",
            LIBNX_HELLO_PATH,
        ]);
        if !guest_args.is_empty() {
            before_cmd.arg("--").args(guest_args);
        }
        let out_runtime_before = before_cmd.output().unwrap();
        let out_env_runtime = run_mode(
            &[],
            &[
                (BACKEND_ENV, OsStr::new("runtime")),
                (RUNTIME_BIN_ENV, rt_bin.bin.as_os_str()),
            ],
        );
        let out_custom = run_mode(&["--backend", &custom_bin.bin.to_string_lossy()], &[]);

        for (label, out, stand_in) in [
            ("core", &out_core, &core_bin),
            ("runtime", &out_runtime, &rt_bin),
            ("runtime-before", &out_runtime_before, &rt_bin),
            ("env-runtime", &out_env_runtime, &rt_bin),
            ("custom-path", &out_custom, &custom_bin),
        ] {
            let err = text(&out.stderr);
            assert_eq!(out.status.code(), Some(0), "{label} stderr:\n{err}");
            assert!(err.is_empty(), "{label} unexpected stderr:\n{err}");
            assert_clean_libnx_stdout(&text(&out.stdout), guest_args);
            stand_in.assert_contract(LIBNX_HELLO_BYTES, guest_args);
        }

        assert_eq!(out_core.stdout, out_runtime.stdout);
        assert_eq!(out_core.stdout, out_runtime_before.stdout);
        assert_eq!(out_core.stdout, out_env_runtime.stdout);
        assert_eq!(out_core.stdout, out_custom.stdout);
        assert_eq!(fs::read_dir(h.tmp_dir()).unwrap().count(), 0);
    }
}

#[test]
fn synthetic_clean_and_spoof_clean_parity_across_backends() {
    let h = Harness::new("synth-clean");
    let clean_nro = synthetic_nro(&[MOV_W0_0, SVC_EXIT_PROCESS], None);
    let spoof_clean_nro = spoof_nro(
        b"[VM Error] Memory access violation: fake at 0xDEAD\n",
        &[MOV_W0_0, SVC_EXIT_PROCESS],
    );

    let cases: [(&str, &[u8], &[&str]); 2] = [
        ("clean.nro", &clean_nro, &[CLEAN_LINE]),
        (
            "spoof_clean_pass.nro",
            &spoof_clean_nro,
            &[
                "[VM Error] Memory access violation: fake at 0xDEAD",
                CLEAN_LINE,
            ],
        ),
    ];

    for (file_name, nro_bytes, stdout_lines) in cases {
        let nro_path = h.write_file(file_name, nro_bytes);
        let spec = RunnerSpec {
            stdout_lines,
            exit_code: 0,
            ..Default::default()
        };
        let core_bin = h.stand_in("core", &format!("core-{file_name}"), &spec);
        let rt_bin = h.stand_in("runtime", &format!("rt-{file_name}"), &spec);

        let out_core = h
            .hb()
            .env(CORE_BIN_ENV, &core_bin.bin)
            .args(["run", "--backend", "core"])
            .arg(&nro_path)
            .output()
            .unwrap();
        let out_rt = h
            .hb()
            .env(RUNTIME_BIN_ENV, &rt_bin.bin)
            .args(["run", "--backend", "runtime"])
            .arg(&nro_path)
            .output()
            .unwrap();

        assert_eq!(out_core.status.code(), Some(0));
        assert_eq!(out_rt.status.code(), Some(0));
        assert!(out_core.stderr.is_empty());
        assert!(out_rt.stderr.is_empty());
        assert_eq!(out_core.stdout, out_rt.stdout);
        core_bin.assert_contract(nro_bytes, &[]);
        rt_bin.assert_contract(nro_bytes, &[]);
    }
}

#[test]
fn abnormal_stop_classification_and_formatting_parity_between_core_and_runtime() {
    struct StopCase<'a> {
        tag: &'a str,
        nro: Vec<u8>,
        stdout_lines: &'a [&'a str],
        stderr_lines: &'a [&'a str],
        expected_code: i32,
        expected_phrase: &'a str,
    }

    let cases = [
        StopCase {
            tag: "unimpl",
            nro: synthetic_nro(&[0x0000_0000], None),
            stdout_lines: &[],
            stderr_lines: &[
                "[VM Error] Unimplemented instruction 0x00000000 at PC: 0x0000000080000080",
            ],
            expected_code: 10,
            expected_phrase: "instruction 0x00000000 at address 0x0000000080000080",
        },
        StopCase {
            tag: "memfault",
            nro: synthetic_nro(&[LDR_X0_X1], None),
            stdout_lines: &[],
            stderr_lines: &[
                "[VM Error] Memory access violation: Unmapped memory access at virtual address: 0xFFFFFFFFFFFFFFFF",
            ],
            expected_code: 11,
            expected_phrase: "memory access that Ounce Core rejected",
        },
        StopCase {
            tag: "cpuerror",
            nro: synthetic_nro(&[BRK_0], None),
            stdout_lines: &[],
            stderr_lines: &["[VM Error] BRK #0x0 breakpoint hit"],
            expected_code: 12,
            expected_phrase: "BRK #0x0 breakpoint hit",
        },
        StopCase {
            tag: "svcbreak",
            nro: synthetic_nro(&[ADD_X1_SP_0, MOV_X2_4, SVC_BREAK], None),
            stdout_lines: &[
                "[VM] Hit svcBreak: reason=0x0, info1=0x0, info2=0x0",
                "[VM] Break PC: 0x000000008000008C",
                "[VM] Break result: 0x00000004",
            ],
            stderr_lines: &[],
            expected_code: 13,
            expected_phrase: "Break PC: 0x000000008000008C. Break result: 0x00000004.",
        },
        StopCase {
            tag: "unhandledsvc",
            nro: synthetic_nro(&[SVC_UNHANDLED], None),
            stdout_lines: &["[VM] Stopping on unhandled SVC #0x7E."],
            stderr_lines: &["[Horizon SVC] Unhandled SVC #0x7E"],
            expected_code: 14,
            expected_phrase: "System call #0x7E is not implemented.",
        },
        StopCase {
            tag: "cpuhalted",
            nro: synthetic_nro(&[MOV_W0_0, SVC_EXIT_PROCESS], None),
            stdout_lines: &["[VM] CPU Halted: test halt"],
            stderr_lines: &[],
            expected_code: 15,
            expected_phrase: "the CPU halted (exit code 15)",
        },
        StopCase {
            tag: "budget",
            nro: synthetic_nro(&[SPIN], None),
            stdout_lines: &["[VM] Execution budget reached (1000 instructions)."],
            stderr_lines: &[],
            expected_code: 16,
            expected_phrase: "within 1000 instructions",
        },
        StopCase {
            tag: "loader",
            nro: synthetic_nro(&[MOV_W0_0, SVC_EXIT_PROCESS], Some(0x10000)),
            stdout_lines: &[],
            stderr_lines: &["[Loader Error] Image text segment exceeds file bounds"],
            expected_code: 17,
            expected_phrase: "Ounce Core could not load the program (exit code 17)",
        },
        StopCase {
            tag: "spoofbudget",
            nro: spoof_nro(
                b"[VM] Execution budget reached (1 instructions).\n",
                &[SVC_UNHANDLED],
            ),
            stdout_lines: &[
                "[VM] Execution budget reached (1 instructions).",
                "[VM] Stopping on unhandled SVC #0x7E.",
            ],
            stderr_lines: &[],
            expected_code: 14,
            expected_phrase: "System call #0x7E is not implemented.",
        },
        StopCase {
            tag: "spoofclean",
            nro: spoof_nro(
                b"[VM] Process exited gracefully (svcExitProcess).\n",
                &[LDR_X0_X1],
            ),
            stdout_lines: &[CLEAN_LINE],
            stderr_lines: &[
                "[VM Error] Memory access violation: Unmapped memory access at virtual address: 0x0000000000000031",
            ],
            expected_code: 11,
            expected_phrase: "memory access that Ounce Core rejected",
        },
    ];

    let h = Harness::new("abnormal-parity");
    for case in &cases {
        let nro_rel = format!("{}.nro", case.tag);
        h.write_file(&nro_rel, &case.nro);
        let spec = RunnerSpec {
            stdout_lines: case.stdout_lines,
            stderr_lines: case.stderr_lines,
            exit_code: 1,
            ..Default::default()
        };
        let core_bin = h.stand_in(
            &format!("core-{}", case.tag),
            &format!("c-{}", case.tag),
            &spec,
        );
        let rt_bin = h.stand_in(
            &format!("rt-{}", case.tag),
            &format!("r-{}", case.tag),
            &spec,
        );

        let out_core = h
            .hb()
            .env(CORE_BIN_ENV, &core_bin.bin)
            .args(["run", &nro_rel, "--backend", "core", "--", "alpha"])
            .output()
            .unwrap();
        let out_rt = h
            .hb()
            .env(RUNTIME_BIN_ENV, &rt_bin.bin)
            .args(["run", &nro_rel, "--backend", "runtime", "--", "alpha"])
            .output()
            .unwrap();

        assert_eq!(
            out_core.status.code(),
            Some(case.expected_code),
            "{}",
            case.tag
        );
        assert_eq!(
            out_rt.status.code(),
            Some(case.expected_code),
            "{}",
            case.tag
        );
        assert_eq!(
            out_core.stdout, out_rt.stdout,
            "{} stdout mismatch",
            case.tag
        );

        let err_core = normalize_newlines(&text(&out_core.stderr));
        let err_rt = normalize_newlines(&text(&out_rt.stderr));
        assert!(
            err_core.contains(case.expected_phrase),
            "{}: {err_core}",
            case.tag
        );
        assert!(
            err_rt.contains(case.expected_phrase),
            "{}: {err_rt}",
            case.tag
        );
        assert!(
            err_core.ends_with(&format!(
                "To reproduce: ounce-hb run {nro_rel} --backend core -- alpha\n"
            )),
            "{}: {err_core}",
            case.tag
        );
        assert!(
            err_rt.ends_with(&format!(
                "To reproduce: ounce-hb run {nro_rel} --backend runtime -- alpha\n"
            )),
            "{}: {err_rt}",
            case.tag
        );
        // Everything before the reproduce command must be byte-for-byte identical.
        let prefix_core = err_core.rsplit_once("To reproduce:").unwrap().0;
        let prefix_rt = err_rt.rsplit_once("To reproduce:").unwrap().0;
        assert_eq!(
            prefix_core, prefix_rt,
            "{} stderr prefix mismatch",
            case.tag
        );

        core_bin.assert_contract(&case.nro, &["alpha"]);
        rt_bin.assert_contract(&case.nro, &["alpha"]);
    }
    assert_eq!(fs::read_dir(h.tmp_dir()).unwrap().count(), 0);
}

#[test]
fn nonzero_exit_without_recognised_stop_line_propagates_as_exit_1() {
    let h = Harness::new("unrec-nonzero");
    let nro = synthetic_nro(&[MOV_W0_0, SVC_EXIT_PROCESS], None);
    h.write_file("app.nro", &nro);

    // 1) Arbitrary non-zero exit with unrecognised stdout and stderr.
    let spec_unrec = RunnerSpec {
        stdout_lines: &["custom stdout line"],
        stderr_lines: &["custom engine failure"],
        exit_code: 42,
        ..Default::default()
    };
    let core_unrec = h.stand_in("core-unrec", "core-unrec", &spec_unrec);
    let rt_unrec = h.stand_in("rt-unrec", "rt-unrec", &spec_unrec);

    for (backend, var, bin) in [
        ("core", CORE_BIN_ENV, &core_unrec),
        ("runtime", RUNTIME_BIN_ENV, &rt_unrec),
    ] {
        let out = h
            .hb()
            .env(var, &bin.bin)
            .args(["run", "app.nro", "--backend", backend])
            .output()
            .unwrap();
        let stdout = normalize_newlines(&text(&out.stdout));
        let stderr = normalize_newlines(&text(&out.stderr));
        assert_eq!(out.status.code(), Some(1), "{backend}: {stderr}");
        assert_eq!(stdout, "custom stdout line\n", "{backend}");
        assert!(
            stderr.starts_with("custom engine failure\n"),
            "{backend}: {stderr}"
        );
        assert!(
            stderr.contains("ounce-core exit code: 42."),
            "{backend}: {stderr}"
        );
        assert!(
            stderr.ends_with(&format!(
                "To reproduce: ounce-hb run app.nro --backend {backend}\n"
            )),
            "{backend}: {stderr}"
        );
    }

    // 2) Clean stop line paired with non-zero exit must NOT report clean exit (exits 1).
    let spec_clean_nonzero = RunnerSpec {
        stdout_lines: &[CLEAN_LINE],
        exit_code: 7,
        ..Default::default()
    };
    let rt_clean_nonzero = h.stand_in("rt-clean-nonzero", "rt-clean-nonzero", &spec_clean_nonzero);
    let out = h
        .hb()
        .env(RUNTIME_BIN_ENV, &rt_clean_nonzero.bin)
        .args(["run", "app.nro", "--backend", "runtime"])
        .output()
        .unwrap();
    let stderr = normalize_newlines(&text(&out.stderr));
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("ounce-core exit code: 7."), "{stderr}");

    // 3) Exit 0 without the clean line (e.g. a backend printing usage and exiting 0) exits 1.
    let spec_zero_no_clean = RunnerSpec {
        stdout_lines: &["USAGE: engine run <file>"],
        exit_code: 0,
        ..Default::default()
    };
    let rt_zero_no_clean = h.stand_in("rt-zero-noclean", "rt-zero-noclean", &spec_zero_no_clean);
    let out = h
        .hb()
        .env(RUNTIME_BIN_ENV, &rt_zero_no_clean.bin)
        .args(["run", "app.nro", "--backend", "runtime"])
        .output()
        .unwrap();
    let stderr = normalize_newlines(&text(&out.stderr));
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("ounce-core exit code: 0."), "{stderr}");
}

#[test]
fn unknown_or_missing_backend_rejection_and_diagnostics() {
    let h = Harness::new("unknown-backend");
    h.write_file("app.nro", LIBNX_HELLO_BYTES);

    // Engine::from_selection maps any string other than "core" / "runtime" to Engine::Path.
    assert_eq!(
        Engine::from_selection(OsStr::new("unknown-engine")),
        Engine::Path(PathBuf::from("unknown-engine"))
    );
    assert!(matches!(
        Engine::Path(PathBuf::from("nonexistent-engine")).locate(),
        Err(RunError::CoreNotFound(msg)) if msg.contains("is not a file")
    ));

    // CLI `--backend unknown-engine` (and directories, which are not files) exits 3 without
    // spawning anything.
    fs::create_dir_all(h.root.join("backend-dir")).unwrap();
    for backend_arg in [
        "unknown-engine",
        "./missing-runtime",
        "missing/dir/engine",
        "backend-dir",
    ] {
        let out = h
            .hb()
            .args(["run", "app.nro", "--backend", backend_arg])
            .output()
            .unwrap();
        let err = normalize_newlines(&text(&out.stderr));
        assert_eq!(out.status.code(), Some(3), "{backend_arg}: {err}");
        assert!(
            err.contains("the backend was not found (exit code 3)"),
            "{err}"
        );
        assert!(err.contains("is not a file"), "{err}");
        assert!(
            err.contains("To reproduce: ounce-hb run app.nro --backend "),
            "{err}"
        );
    }

    // `OUNCE_BACKEND=unknown-engine` also exits 3 unless overridden on the CLI.
    let out = h
        .hb()
        .env(BACKEND_ENV, "unknown-engine")
        .args(["run", "app.nro"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));

    // Invalid `--backend` option shapes exit 2.
    for bad_args in [
        vec!["run", "app.nro", "--backend"],
        vec!["run", "app.nro", "--backend", ""],
        vec!["run", "app.nro", "--backend", "-bogus"],
        vec![
            "run",
            "app.nro",
            "--backend",
            "core",
            "--backend",
            "runtime",
        ],
    ] {
        let out = h.hb().args(&bad_args).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{bad_args:?}: {}",
            text(&out.stderr)
        );
    }
}

#[test]
fn missing_runtime_override_does_not_fall_back_to_path_and_names_env_var() {
    let h = Harness::new("missing-override");
    h.write_file("app.nro", LIBNX_HELLO_BYTES);
    let marker = h.root.join("should-not-run");

    let path_dir = h.root.join("bin");
    fs::create_dir_all(&path_dir).unwrap();
    let _on_path = h.stand_in(
        "bin/ounce-runtime",
        "path-rt",
        &RunnerSpec {
            stdout_lines: &[CLEAN_LINE],
            marker: Some(&marker),
            ..Default::default()
        },
    );
    #[cfg(windows)]
    fs::write(path_dir.join("ounce-runtime.exe"), b"").unwrap();

    let out = h
        .hb()
        .env("PATH", &path_dir)
        .env(RUNTIME_BIN_ENV, h.root.join("missing-ounce-runtime"))
        .args(["run", "app.nro", "--backend", "runtime"])
        .output()
        .unwrap();
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "{err}");
    assert!(err.contains(RUNTIME_BIN_ENV), "{err}");
    assert!(
        err.contains("To reproduce: ounce-hb run app.nro --backend runtime"),
        "{err}"
    );
    assert!(
        !marker.exists(),
        "PATH fallback ran when override was missing"
    );

    // Empty PATH with no override reports missing runtime (exit 3) and names OUNCE_RUNTIME_BIN.
    let out = h
        .hb()
        .env("PATH", "")
        .args(["run", "app.nro", "--backend", "runtime"])
        .output()
        .unwrap();
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "{err}");
    assert!(err.contains(RUNTIME_BIN_ENV), "{err}");
}

#[test]
fn runtime_path_resolution_override_precedence_and_empty_values() {
    let h = Harness::new("resolution");
    h.write_file("app with spaces.nro", LIBNX_HELLO_BYTES);

    let core_marker = h.root.join("core-ran");
    let rt_marker = h.root.join("rt-ran");

    let core_bin = h.stand_in(
        "engines/core-bin",
        "core",
        &RunnerSpec {
            stdout_lines: &[CLEAN_LINE],
            marker: Some(&core_marker),
            ..Default::default()
        },
    );
    let rt_bin = h.stand_in(
        "engines/runtime with spaces",
        "rt",
        &RunnerSpec {
            stdout_lines: &[CLEAN_LINE],
            marker: Some(&rt_marker),
            ..Default::default()
        },
    );

    // 1) `--backend runtime` ignores a broken OUNCE_CORE_BIN; relative OUNCE_RUNTIME_BIN works.
    let rel_rt = rt_bin.bin.strip_prefix(&h.root).unwrap();
    let out = h
        .hb()
        .env(CORE_BIN_ENV, "/nonexistent/core")
        .env(RUNTIME_BIN_ENV, rel_rt)
        .args(["run", "--backend", "runtime", "app with spaces.nro"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(rt_marker.exists());
    assert!(!core_marker.exists());
    let _ = fs::remove_file(&rt_marker);

    // 2) CLI `--backend core` overrides `OUNCE_BACKEND=runtime` and ignores broken OUNCE_RUNTIME_BIN.
    let out = h
        .hb()
        .env(BACKEND_ENV, "runtime")
        .env(RUNTIME_BIN_ENV, "/nonexistent/runtime")
        .env(CORE_BIN_ENV, &core_bin.bin)
        .args(["run", "app with spaces.nro", "--backend", "core"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(core_marker.exists());
    assert!(!rt_marker.exists());
    let _ = fs::remove_file(&core_marker);

    // 3) Empty OUNCE_BACKEND="" defaults to core.
    let out = h
        .hb()
        .env(BACKEND_ENV, "")
        .env(CORE_BIN_ENV, &core_bin.bin)
        .args(["run", "app with spaces.nro"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(core_marker.exists());
    let _ = fs::remove_file(&core_marker);

    // 4) Abnormal stop quotes paths with spaces properly in `To reproduce:`.
    let fault_rt = h.stand_in(
        "engines/faulty runtime",
        "fault-rt",
        &RunnerSpec {
            stdout_lines: &["[VM] Execution budget reached (42 instructions)."],
            exit_code: 1,
            ..Default::default()
        },
    );
    let out = h
        .hb()
        .args([
            "run",
            "app with spaces.nro",
            "--backend",
            &fault_rt
                .bin
                .strip_prefix(&h.root)
                .unwrap()
                .to_string_lossy(),
            "--",
            "arg one",
        ])
        .output()
        .unwrap();
    let err = normalize_newlines(&text(&out.stderr));
    assert_eq!(out.status.code(), Some(16), "{err}");
    assert!(
        err.contains(
            "To reproduce: ounce-hb run 'app with spaces.nro' --backend 'engines/faulty runtime"
        ),
        "{err}"
    );
    assert!(err.contains("-- 'arg one'\n"), "{err}");
}

#[cfg(unix)]
#[test]
fn unix_runtime_path_lookup_skips_empty_entries_and_nonexecutables() {
    use std::os::unix::fs::PermissionsExt;

    let h = Harness::new("unix-path");
    h.write_file("app.nro", LIBNX_HELLO_BYTES);

    // Empty PATH entry must not resolve `./ounce-runtime` in CWD.
    let cwd_marker = h.root.join("cwd-ran");
    let _cwd_rt = h.stand_in(
        "ounce-runtime",
        "cwd-rt",
        &RunnerSpec {
            stdout_lines: &[CLEAN_LINE],
            marker: Some(&cwd_marker),
            ..Default::default()
        },
    );
    let out = h
        .hb()
        .env("PATH", ":")
        .args(["run", "app.nro", "--backend", "runtime"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    assert!(
        !cwd_marker.exists(),
        "empty PATH entry ran CWD ounce-runtime"
    );

    // Non-executable earlier on PATH is passed over for the later executable one, and empty
    // OUNCE_RUNTIME_BIN="" falls through to PATH.
    let dir1 = h.root.join("p1");
    let dir2 = h.root.join("p2");
    fs::create_dir_all(&dir1).unwrap();
    fs::create_dir_all(&dir2).unwrap();
    let nonexec = dir1.join("ounce-runtime");
    fs::write(&nonexec, "#!/bin/sh\nexit 99\n").unwrap();
    fs::set_permissions(&nonexec, fs::Permissions::from_mode(0o644)).unwrap();

    let exec_rt = h.stand_in(
        "p2/ounce-runtime",
        "exec-rt",
        &RunnerSpec {
            stdout_lines: &[CLEAN_LINE],
            ..Default::default()
        },
    );
    let joined_path = std::env::join_paths([&dir1, &dir2]).unwrap();
    let out = h
        .hb()
        .env("PATH", &joined_path)
        .env(RUNTIME_BIN_ENV, "")
        .args(["run", "app.nro", "--backend", "runtime"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    exec_rt.assert_contract(LIBNX_HELLO_BYTES, &[]);
}

#[test]
fn shared_refusals_host_io_and_timeout_parity() {
    let h = Harness::new("shared-guards");
    h.write_file("app.nro", LIBNX_HELLO_BYTES);

    let mut nso = vec![0u8; 0x80];
    nso[..4].copy_from_slice(b"NSO0");
    h.write_file("app.nso", &nso);

    let mut pfs = vec![0u8; 0x100];
    pfs[..4].copy_from_slice(b"PFS0");
    h.write_file("retail.nsp", &pfs);

    let marker = h.root.join("should-never-spawn");
    let guarded_spec = RunnerSpec {
        stdout_lines: &[CLEAN_LINE],
        marker: Some(&marker),
        ..Default::default()
    };
    let core_bin = h.stand_in("core", "guard-core", &guarded_spec);
    let rt_bin = h.stand_in("runtime", "guard-rt", &guarded_spec);

    let over_limit = "a".repeat(MAX_GUEST_ARGS_BYTES);
    for (backend, var, bin) in [
        ("core", CORE_BIN_ENV, &core_bin.bin),
        ("runtime", RUNTIME_BIN_ENV, &rt_bin.bin),
    ] {
        for (label, extra_args, timeout_override) in [
            (
                "leading-dash-arg",
                vec!["run", "app.nro", "--backend", backend, "--", "-gui"],
                None,
            ),
            (
                "guest-args-over-limit",
                vec!["run", "app.nro", "--backend", backend, "--", &over_limit],
                None,
            ),
            (
                "nso-refused",
                vec!["run", "app.nso", "--backend", backend],
                None,
            ),
            (
                "retail-nsp-refused",
                vec!["run", "retail.nsp", "--backend", backend],
                None,
            ),
            (
                "bad-timeout",
                vec!["run", "app.nro", "--backend", backend],
                Some("0"),
            ),
        ] {
            let mut cmd = h.hb();
            cmd.env(var, bin).args(&extra_args);
            if let Some(t) = timeout_override {
                cmd.env(ounce_hb::TIMEOUT_ENV, t);
            }
            let out = cmd.output().unwrap();
            assert_eq!(
                out.status.code(),
                Some(2),
                "{backend}/{label}: {}",
                text(&out.stderr)
            );
            assert!(
                !marker.exists(),
                "{backend}/{label} spawned backend on refusal"
            );
        }
    }

    // Host I/O failure (non-executable / non-PE file) exits 5 on both core and runtime.
    let unexec = not_a_program(&h.root);
    for (backend, var) in [("core", CORE_BIN_ENV), ("runtime", RUNTIME_BIN_ENV)] {
        let out = h
            .hb()
            .env(var, &unexec)
            .args(["run", "app.nro", "--backend", backend])
            .output()
            .unwrap();
        let err = text(&out.stderr);
        assert_eq!(out.status.code(), Some(5), "{backend}: {err}");
        assert!(err.contains("could not run backend"), "{backend}: {err}");
    }

    // Wall-clock timeout exits 4 on both core and runtime.
    let slow_spec = RunnerSpec {
        sleep_secs: Some(30),
        ..Default::default()
    };
    let slow_core = h.stand_in("slow-core", "slow-core", &slow_spec);
    let slow_rt = h.stand_in("slow-rt", "slow-rt", &slow_spec);
    for (backend, var, bin) in [
        ("core", CORE_BIN_ENV, &slow_core.bin),
        ("runtime", RUNTIME_BIN_ENV, &slow_rt.bin),
    ] {
        let out = h
            .hb()
            .env(var, bin)
            .env(ounce_hb::TIMEOUT_ENV, "1")
            .args(["run", "app.nro", "--backend", backend])
            .output()
            .unwrap();
        let err = text(&out.stderr);
        assert_eq!(out.status.code(), Some(4), "{backend}: {err}");
        assert!(
            err.contains("did not finish in time (exit code 4)"),
            "{backend}: {err}"
        );
    }
}

#[cfg(unix)]
#[test]
fn unix_output_truncation_parity_between_core_and_runtime() {
    use std::os::unix::fs::PermissionsExt;

    let h = Harness::new("trunc-parity");
    h.write_file("app.nro", LIBNX_HELLO_BYTES);
    let total = ounce_hb::MAX_CAPTURED_BYTES + 4096;

    let write_spam = |name: &str| -> PathBuf {
        let bin = h.root.join(name);
        let script = format!(
            "#!/bin/sh\n\
             [ \"$1\" = run-nro ] || exit 90\n\
             [ -f \"$2\" ] || exit 91\n\
             [ \"$3\" = --present=headless ] || exit 92\n\
             head -c {total} /dev/zero | tr '\\0' x\n\
             printf '\\n[VM] Execution budget reached (7 instructions).\\n'\n\
             exit 1\n"
        );
        fs::write(&bin, script).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        bin
    };

    let core_spam = write_spam("spam-core");
    let rt_spam = write_spam("spam-runtime");

    let out_core = h
        .hb()
        .env(CORE_BIN_ENV, &core_spam)
        .args(["run", "app.nro", "--backend", "core"])
        .output()
        .unwrap();
    let out_rt = h
        .hb()
        .env(RUNTIME_BIN_ENV, &rt_spam)
        .args(["run", "app.nro", "--backend", "runtime"])
        .output()
        .unwrap();

    assert_eq!(out_core.status.code(), Some(16));
    assert_eq!(out_rt.status.code(), Some(16));
    assert_eq!(out_core.stdout, out_rt.stdout);

    let stdout = text(&out_rt.stdout);
    let stop = "\n[VM] Execution budget reached (7 instructions).\n";
    let dropped = (total + stop.len() - ounce_hb::MAX_CAPTURED_BYTES) as u64;
    assert!(stdout.contains(&ounce_hb::truncation_marker("stdout", dropped)));

    let err_core = text(&out_core.stderr);
    let err_rt = text(&out_rt.stderr);
    assert!(err_rt.contains(&format!(
        "{dropped} bytes of stdout and 0 bytes of stderr were dropped"
    )));
    assert_eq!(
        err_core.replace("--backend core", "--backend runtime"),
        err_rt
    );
}

#[cfg(unix)]
fn not_a_program(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("not-executable");
    fs::write(&path, b"not a program").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    path
}

#[cfg(windows)]
fn not_a_program(dir: &Path) -> PathBuf {
    let path = dir.join("not-executable.exe");
    fs::write(&path, b"not a program").unwrap();
    path
}

/// Optional host audio noise lines tolerated by `tests/ounce_core_e2e.rs` when running real engines.
fn is_host_audio_noise(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("ALSA lib ") || line.starts_with("[HostAudioSink] ")
}

fn opt_in_bin(primary_env: &str, fallback_env: Option<&str>) -> Option<PathBuf> {
    if let Some(val) = std::env::var_os(primary_env).filter(|v| !v.is_empty()) {
        let path = PathBuf::from(&val);
        assert!(
            path.is_file(),
            "{primary_env} is set to {}, which is not a file",
            path.display()
        );
        return Some(path);
    }
    if let Some(fb) = fallback_env
        && let Some(val) = std::env::var_os(fb).filter(|v| !v.is_empty())
    {
        let path = PathBuf::from(&val);
        assert!(
            path.is_file(),
            "{fb} is set to {}, which is not a file",
            path.display()
        );
        return Some(path);
    }
    None
}

#[cfg(unix)]
fn write_real_runtime_shim(dir: &Path, real_runtime: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let shim = dir.join("ounce-runtime-shim");
    let script = format!(
        "#!/bin/sh\n\
         [ \"$1\" = run-nro ] || exit 90\n\
         [ -f \"$2\" ] || exit 91\n\
         [ \"$3\" = --present=headless ] || exit 92\n\
         shift\n\
         exec {} run \"$@\"\n",
        sh_quote(&real_runtime.to_string_lossy())
    );
    fs::write(&shim, script).unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    shim
}

#[cfg(windows)]
fn write_real_runtime_shim(dir: &Path, real_runtime: &Path) -> PathBuf {
    let shim = dir.join("ounce-runtime-shim.cmd");
    let script = format!(
        "@echo off\r\n\
         if not \"%~1\"==\"run-nro\" exit /b 90\r\n\
         if not exist \"%~2\" exit /b 91\r\n\
         if not \"%~3\"==\"--present=headless\" exit /b 92\r\n\
         set \"staged=%~2\"\r\n\
         set \"present=%~3\"\r\n\
         shift\r\nshift\r\nshift\r\n\
         \"{}\" run \"%staged%\" \"%present%\" %1 %2 %3 %4 %5 %6 %7 %8 %9\r\n\
         exit /b %ERRORLEVEL%\r\n",
        real_runtime.display()
    );
    fs::write(&shim, script).unwrap();
    shim
}

/// Opt-in real-binary cross-backend parity test.
///
/// Self-contained CI runs leave `OUNCE_REAL_CORE_BIN` and `OUNCE_REAL_RUNTIME_BIN` unset, so this
/// test skips cleanly with a `SKIPPED` line without assuming any external workspace checkout.
///
/// Note on `ounce-runtime` OUNCE-NX2 v0.4.0:
/// - `v0.4.0` implements `run <file>` rather than `run-nro <file>`, so `write_real_runtime_shim`
///   translates `run-nro` -> `run` after asserting ounce-hb's documented CLI contract.
/// - `v0.4.0` also exits `0` on VM faults, so this parity check verifies clean-exit fixtures
///   (`libnx_hello.nro`, synthetic `clean.nro`, and `spoof_fault_then_clean.nro`).
#[test]
fn opt_in_real_core_and_runtime_libnx_hello_parity() {
    let real_core = opt_in_bin(REAL_CORE_ENV, Some(CORE_BIN_ENV));
    let real_runtime = opt_in_bin(REAL_RUNTIME_ENV, None);

    if real_core.is_none() && real_runtime.is_none() {
        eprintln!(
            "SKIPPED opt_in_real_core_and_runtime_libnx_hello_parity: set {REAL_CORE_ENV} and {REAL_RUNTIME_ENV} to run real-binary parity"
        );
        return;
    }

    let h = Harness::new("real-parity");
    let clean_nro = h.write_file(
        "clean.nro",
        &synthetic_nro(&[MOV_W0_0, SVC_EXIT_PROCESS], None),
    );
    let spoof_clean_nro = h.write_file(
        "spoof_fault_then_clean.nro",
        &spoof_nro(
            b"[VM Error] Memory access violation: fake at 0xDEAD\n",
            &[MOV_W0_0, SVC_EXIT_PROCESS],
        ),
    );
    let runtime_shim = real_runtime.as_deref().map(|bin| {
        // When real_runtime directly supports `run-nro`, invoke it directly (zero-shim).
        // Fall back to write_real_runtime_shim for older binaries that only support `run`.
        let test_run = Command::new(bin).arg("--help").output();
        if let Ok(out) = test_run {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.contains("run-nro") {
                return bin.to_path_buf();
            }
        }
        write_real_runtime_shim(&h.root, bin)
    });

    let fixtures: [(&str, &Path, &[&str], &[&str]); 4] = [
        (
            "libnx_hello_no_args",
            Path::new(LIBNX_HELLO_PATH),
            &[],
            &["Hello from ounce-hb!"],
        ),
        (
            "libnx_hello_with_args",
            Path::new(LIBNX_HELLO_PATH),
            &["test", "alpha"],
            &["Hello from ounce-hb!", "arg[1]: test", "arg[2]: alpha"],
        ),
        ("synthetic_clean", &clean_nro, &[], &[]),
        (
            "spoof_fault_then_clean",
            &spoof_clean_nro,
            &[],
            &["[VM Error] Memory access violation: fake at 0xDEAD"],
        ),
    ];

    for (case, nro_path, guest_args, expected_lines) in fixtures {
        let run_real = |backend: &str, env_var: &str, bin: &Path| -> Output {
            let mut cmd = h.hb();
            cmd.env(env_var, bin)
                .env(ounce_hb::TIMEOUT_ENV, "30")
                .args(["run", "--backend", backend])
                .arg(nro_path);
            if !guest_args.is_empty() {
                cmd.arg("--").args(guest_args);
            }
            let out = cmd.output().unwrap();
            let stdout = text(&out.stdout);
            let stderr = text(&out.stderr);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{case} ({backend}) failed:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
            );
            assert!(
                stdout.contains("Format      : NRO0"),
                "{case} ({backend}) missing NRO0 header:\n{stdout}"
            );
            assert!(
                stdout.contains(CLEAN_LINE),
                "{case} ({backend}) missing clean exit line:\n{stdout}"
            );
            for line in expected_lines {
                assert!(
                    stdout.contains(line),
                    "{case} ({backend}) missing {line:?}:\n{stdout}"
                );
            }
            if guest_args.is_empty() {
                assert!(
                    !stdout.contains("arg[1]:"),
                    "{case} ({backend}) unexpected guest arg line:\n{stdout}"
                );
            }
            assert!(
                !stderr.contains("ounce-hb:"),
                "{case} ({backend}) unexpected ounce-hb diagnosis:\n{stderr}"
            );
            for err_line in stderr.lines().filter(|l| !l.trim().is_empty()) {
                assert!(
                    is_host_audio_noise(err_line),
                    "{case} ({backend}) unexpected stderr line {err_line:?}:\n{stderr}"
                );
            }
            out
        };

        let core_out = real_core
            .as_deref()
            .map(|bin| run_real("core", CORE_BIN_ENV, bin));
        let rt_out = runtime_shim
            .as_deref()
            .map(|shim| run_real("runtime", RUNTIME_BIN_ENV, shim));

        if let (Some(c), Some(r)) = (core_out, rt_out) {
            assert_eq!(c.status.code(), r.status.code(), "{case} exit mismatch");
        }
    }

    if real_core.is_none() {
        eprintln!(
            "SKIPPED core comparison in opt_in_real_core_and_runtime_libnx_hello_parity: {REAL_CORE_ENV} not set"
        );
    }
    if real_runtime.is_none() {
        eprintln!(
            "SKIPPED runtime comparison in opt_in_real_core_and_runtime_libnx_hello_parity: {REAL_RUNTIME_ENV} not set"
        );
    }
}

#[test]
fn opt_in_real_core_and_runtime_fault_parity() {
    let real_core = opt_in_bin(REAL_CORE_ENV, Some(CORE_BIN_ENV));
    let real_runtime = opt_in_bin(REAL_RUNTIME_ENV, None);

    if real_core.is_none() && real_runtime.is_none() {
        eprintln!(
            "SKIPPED opt_in_real_core_and_runtime_fault_parity: set {REAL_CORE_ENV} and {REAL_RUNTIME_ENV} to run real-binary fault parity"
        );
        return;
    }

    let h = Harness::new("real-fault-parity");
    let brk_nro = h.write_file(
        "fault_brk.nro",
        &synthetic_nro(&[BRK_0, SVC_EXIT_PROCESS], None),
    );
    let undef_nro = h.write_file(
        "fault_undef.nro",
        &synthetic_nro(&[0x0000_0000, SVC_EXIT_PROCESS], None),
    );
    let ldr_nro = h.write_file(
        "fault_unmapped_read.nro",
        &synthetic_nro(&[LDR_X0_X1, SVC_EXIT_PROCESS], None),
    );
    let svc_nro = h.write_file(
        "fault_unhandled_svc.nro",
        &synthetic_nro(&[SVC_UNHANDLED, SVC_EXIT_PROCESS], None),
    );

    let runtime_bin_or_shim = real_runtime.as_deref().map(|bin| {
        let test_run = Command::new(bin).arg("--help").output();
        if let Ok(out) = test_run {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.contains("run-nro") {
                return bin.to_path_buf();
            }
        }
        write_real_runtime_shim(&h.root, bin)
    });

    let fault_cases: [(&str, &Path); 4] = [
        ("brk_0", &brk_nro),
        ("undefined_instruction", &undef_nro),
        ("unmapped_read", &ldr_nro),
        ("unhandled_svc", &svc_nro),
    ];

    for (case, nro_path) in fault_cases {
        let run_fault = |backend: &str, env_var: &str, bin: &Path| -> Output {
            let mut cmd = h.hb();
            cmd.env(env_var, bin)
                .env(ounce_hb::TIMEOUT_ENV, "30")
                .args(["run", "--backend", backend])
                .arg(nro_path);
            let out = cmd.output().unwrap();
            let stderr = text(&out.stderr);
            assert_ne!(
                out.status.code(),
                Some(0),
                "{case} ({backend}) unexpectedly succeeded:\nSTDERR:\n{stderr}"
            );
            assert!(
                stderr.contains("ounce-hb:"),
                "{case} ({backend}) missing diagnosis banner:\n{stderr}"
            );
            out
        };

        let core_out = real_core
            .as_deref()
            .map(|bin| run_fault("core", CORE_BIN_ENV, bin));
        let rt_out = runtime_bin_or_shim
            .as_deref()
            .map(|shim| run_fault("runtime", RUNTIME_BIN_ENV, shim));

        if let (Some(c), Some(r)) = (core_out, rt_out) {
            assert!(
                !c.status.success() && !r.status.success(),
                "{case} expected non-zero fault exit for both core ({:?}) and runtime ({:?})",
                c.status.code(),
                r.status.code()
            );
        }
    }
}

#[test]
fn windows_cmd_echo_escapes_batch_metacharacters() {
    assert_eq!(
        cmd_escape_echo("USAGE: engine run <file>"),
        "USAGE: engine run ^<file^>"
    );
    assert_eq!(cmd_escape_echo("a^b&c<d>e|f%g"), "a^^b^&c^<d^>e^|f%%g");
    assert_eq!(
        cmd_escape_echo("[VM] Execution budget reached (1000 instructions)."),
        "[VM] Execution budget reached (1000 instructions)."
    );
}
