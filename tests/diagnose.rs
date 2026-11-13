//! Stop classification and the `ounce-hb run` exit codes, tested against real ounce-core output
//! (v0.8.0 for the plain cases, v0.8.1 for the spoof cases). The fixtures in
//! tests/fixtures/ounce-core-output are captures; NOTES.md there says how each was produced.

#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Command;
use std::time::Duration;

use ounce_hb::RunError;
use ounce_hb::RunOutput;
use ounce_hb::diagnose::{EXCERPT_BYTES, StopReason, from_error, from_output};

/// One capture: stdout, stderr and the exit status ounce-core returned.
struct Capture {
    stdout: &'static str,
    stderr: &'static str,
    exit: i32,
}

macro_rules! capture {
    ($name:literal) => {
        Capture {
            stdout: include_str!(concat!("fixtures/ounce-core-output/", $name, ".stdout")),
            stderr: include_str!(concat!("fixtures/ounce-core-output/", $name, ".stderr")),
            exit: include_str!(concat!("fixtures/ounce-core-output/", $name, ".exit"))
                .trim()
                .parse()
                .unwrap(),
        }
    };
}

fn classify(c: &Capture) -> StopReason {
    from_output(&RunOutput {
        exit_code: Some(c.exit),
        stdout: c.stdout.to_string(),
        stderr: c.stderr.to_string(),
        ..RunOutput::default()
    })
}

#[test]
fn clean_exit_is_clean() {
    assert_eq!(classify(&capture!("clean")), StopReason::CleanExit);
}

#[test]
fn unimplemented_instruction_gives_word_and_address() {
    assert_eq!(
        classify(&capture!("unimpl")),
        StopReason::UnimplementedInstruction {
            word: 0x0000_0000,
            pc: 0x0000_0000_8000_0080,
        }
    );
}

#[test]
fn memory_fault_keeps_message_and_address() {
    assert_eq!(
        classify(&capture!("memfault")),
        StopReason::MemoryFault {
            message: "Unmapped memory access at virtual address: 0xFFFFFFFFFFFFFFFF".to_string(),
            address: Some(u64::MAX),
        }
    );
}

#[test]
fn exhausted_budget_is_reported_with_its_count() {
    assert_eq!(
        classify(&capture!("budget")),
        StopReason::BudgetExhausted { instructions: 1000 }
    );
}

#[test]
fn svc_break_reports_pc_and_leaves_unreadable_result_empty() {
    assert_eq!(
        classify(&capture!("svc_break")),
        StopReason::Break {
            pc: Some(0x0000_0000_8000_008C),
            result: None,
        }
    );
}

#[test]
fn unhandled_svc_gives_the_call_number() {
    assert_eq!(
        classify(&capture!("unhandled_svc")),
        StopReason::UnhandledSvc { id: 0x7E }
    );
}

#[test]
fn loader_error_keeps_ounce_core_wording() {
    assert_eq!(
        classify(&capture!("loader_error")),
        StopReason::LoaderError {
            message: "Image text segment exceeds file bounds (offset 0x0, size 0x10000, available 0x88 bytes)"
                .to_string(),
        }
    );
}

#[test]
fn output_that_is_not_a_stop_line_is_unrecognised_and_kept_raw() {
    // ounce-core's own usage message for `run-nro` with no path. ounce-hb never runs that, but the
    // parser must not turn it into a guess.
    let c = capture!("usage");
    assert_eq!(
        classify(&c),
        StopReason::Unrecognised {
            exit_code: Some(2),
            stdout_tail: c.stdout.to_string(),
            stderr_tail: c.stderr.to_string(),
        }
    );
}

/// Finding 5: the unrecognised reason keeps a bounded excerpt, not a copy of the whole output.
#[test]
fn unrecognised_keeps_only_the_end_of_each_stream() {
    let stdout = "x".repeat(3 * EXCERPT_BYTES) + "END-OUT";
    let stderr = "y".repeat(3 * EXCERPT_BYTES) + "END-ERR";
    let out = RunOutput {
        exit_code: Some(9),
        stdout: stdout.clone(),
        stderr: stderr.clone(),
        ..RunOutput::default()
    };
    match from_output(&out) {
        StopReason::Unrecognised {
            exit_code,
            stdout_tail,
            stderr_tail,
        } => {
            assert_eq!(exit_code, Some(9));
            assert_eq!(stdout_tail.len(), EXCERPT_BYTES);
            assert_eq!(stderr_tail.len(), EXCERPT_BYTES);
            assert!(stdout_tail.ends_with("END-OUT"));
            assert!(stderr_tail.ends_with("END-ERR"));
        }
        other => panic!("{other:?}"),
    }
}

// ---- Finding 6: guest text printed with svcOutputDebugString must not change the category. The
// captures are real ounce-core v0.8.1 output for the spoof programs in make_nros.py.

#[test]
fn guest_cannot_forge_a_budget_stop_before_a_real_unhandled_svc() {
    // The guest prints a budget line, then calls svc #0x7E. The old parser took the first match.
    assert_eq!(
        classify(&capture!("spoof_budget")),
        StopReason::UnhandledSvc { id: 0x7E }
    );
}

#[test]
fn guest_cannot_hide_a_fault_behind_a_printed_clean_line() {
    // The guest prints the clean-exit line, then faults. The real line is on stderr.
    assert_eq!(
        classify(&capture!("spoof_clean")),
        StopReason::MemoryFault {
            message: "Unmapped memory access at virtual address: 0x0000000000000031".to_string(),
            address: Some(0x31),
        }
    );
}

#[test]
fn guest_cannot_make_a_clean_run_fail_by_printing_a_fault_line() {
    // A `[VM Error]` line on stdout is guest text; ounce-core prints its own on stderr.
    assert_eq!(
        classify(&capture!("spoof_fault_then_clean")),
        StopReason::CleanExit
    );
}

#[test]
fn guest_cannot_plant_break_details() {
    // Fake svcBreak lines with PC 0xDEADBEEF come first; the real break follows them.
    assert_eq!(
        classify(&capture!("spoof_break")),
        StopReason::Break {
            pc: Some(0x0000_0000_8000_0098),
            result: None,
        }
    );
}

#[test]
fn stderr_error_lines_are_not_taken_from_stdout() {
    // Only ounce-core writes stderr. The same text on stdout with a failing exit and no real stop
    // line is unrecognised, not a CPU error.
    let out = RunOutput {
        exit_code: Some(1),
        stdout: "[VM Error] BRK #0x0 breakpoint hit\n".to_string(),
        ..RunOutput::default()
    };
    assert!(matches!(from_output(&out), StopReason::Unrecognised { .. }));
    let out = RunOutput {
        exit_code: Some(1),
        stderr: "[VM Error] BRK #0x0 breakpoint hit\n".to_string(),
        ..RunOutput::default()
    };
    assert!(matches!(from_output(&out), StopReason::CpuError { .. }));
}

#[test]
fn exit_zero_without_the_clean_line_is_not_called_clean() {
    let out = RunOutput {
        exit_code: Some(0),
        stdout: "something else entirely\n".to_string(),
        ..RunOutput::default()
    };
    assert!(matches!(
        from_output(&out),
        StopReason::Unrecognised {
            exit_code: Some(0),
            ..
        }
    ));
}

#[test]
fn clean_line_with_a_failing_exit_is_not_called_clean() {
    let c = capture!("clean");
    let out = RunOutput {
        exit_code: Some(1),
        stdout: c.stdout.to_string(),
        ..RunOutput::default()
    };
    assert!(matches!(from_output(&out), StopReason::Unrecognised { .. }));
}

#[test]
fn killed_by_signal_is_unrecognised() {
    let out = RunOutput {
        exit_code: None,
        stdout: "[VM] Execution budget reached (5 instructions).\n".to_string(),
        ..RunOutput::default()
    };
    assert!(matches!(
        from_output(&out),
        StopReason::Unrecognised {
            exit_code: None,
            ..
        }
    ));
}

#[test]
fn run_errors_map_to_their_categories() {
    assert!(matches!(
        from_error(&RunError::CoreNotFound("OUNCE_CORE_BIN is not set".into())),
        StopReason::CoreMissing { .. }
    ));
    assert_eq!(
        from_error(&RunError::Timeout(Duration::from_secs(60))),
        StopReason::Timeout {
            limit: Duration::from_secs(60)
        }
    );
    assert!(matches!(
        from_error(&RunError::Io("cannot start".into())),
        StopReason::HostIo { .. }
    ));
    assert!(matches!(
        from_error(&RunError::UnsupportedArg("-x".into())),
        StopReason::Refused { .. }
    ));
}

#[test]
fn every_fixture_category_has_a_documented_exit_code() {
    let cases = [
        (classify(&capture!("clean")), 0),
        (classify(&capture!("usage")), 1),
        (classify(&capture!("unimpl")), 10),
        (classify(&capture!("memfault")), 11),
        (classify(&capture!("svc_break")), 13),
        (classify(&capture!("unhandled_svc")), 14),
        (classify(&capture!("budget")), 16),
        (classify(&capture!("loader_error")), 17),
        (classify(&capture!("spoof_budget")), 14),
        (classify(&capture!("spoof_clean")), 11),
        (classify(&capture!("spoof_fault_then_clean")), 0),
        (classify(&capture!("spoof_break")), 13),
    ];
    for (reason, code) in cases {
        assert_eq!(reason.exit_code(), code, "{reason:?}");
    }
}

#[test]
fn explanation_names_the_gap_and_the_reproduce_command() {
    let text = classify(&capture!("unimpl")).explain("games/demo.nro", &[]);
    assert!(text.contains("(exit code 10)"), "{text}");
    assert!(
        text.contains("instruction 0x00000000 at address 0x0000000080000080"),
        "{text}"
    );
    assert!(
        text.ends_with("To reproduce: ounce-hb run games/demo.nro\n"),
        "{text}"
    );
}

// ---- End to end: the real `ounce-hb run` binary, with ounce-core replaced by a script that replays
// a capture. Unix only, because the script is a shell script.

#[cfg(unix)]
mod end_to_end {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ounce-core-output")
    }

    fn work_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ounce-hb-diagnose-{tag}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A stand-in ounce-core that prints the capture `case` and exits with its recorded status.
    fn replay_core(dir: &Path, case: &str) -> PathBuf {
        let f = fixtures();
        let exit = fs::read_to_string(f.join(format!("{case}.exit"))).unwrap();
        let script = format!(
            "#!/bin/sh\ncat '{}'\ncat '{}' >&2\nexit {}\n",
            f.join(format!("{case}.stdout")).display(),
            f.join(format!("{case}.stderr")).display(),
            exit.trim()
        );
        let bin = dir.join(format!("ounce-core-{case}"));
        fs::write(&bin, script).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    /// A header-only file that ounce-hb accepts as an NRO. The replay script never reads it.
    fn fake_nro(dir: &Path) -> PathBuf {
        let mut nro = vec![0u8; 0x80];
        nro[0x10..0x14].copy_from_slice(b"NRO0");
        let p = dir.join("demo.nro");
        fs::write(&p, nro).unwrap();
        p
    }

    fn run(dir: &Path, case: &str, extra: &[&str]) -> std::process::Output {
        let core = replay_core(dir, case);
        Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
            .env("OUNCE_CORE_BIN", core)
            .arg("run")
            .arg(fake_nro(dir))
            .args(extra)
            .output()
            .unwrap()
    }

    #[test]
    fn clean_guest_exit_is_exit_zero_and_passes_output_through() {
        let dir = work_dir("clean");
        let out = run(&dir, "clean", &[]);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            include_str!("fixtures/ounce-core-output/clean.stdout")
        );
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn each_abnormal_stop_prints_its_category_and_exits_with_its_code() {
        let dir = work_dir("stops");
        for (case, code, headline) in [
            ("unimpl", 10, "Ounce Core does not implement an instruction"),
            ("memfault", 11, "memory access that Ounce Core rejected"),
            ("svc_break", 13, "svcBreak"),
            ("unhandled_svc", 14, "system service"),
            ("budget", 16, "ran out of instructions"),
            ("loader_error", 17, "could not load the program"),
        ] {
            let out = run(&dir, case, &[]);
            let err = String::from_utf8_lossy(&out.stderr).into_owned();
            assert_eq!(out.status.code(), Some(code), "{case}: {err}");
            assert!(err.contains(headline), "{case}: {err}");
            assert!(err.contains("To reproduce: ounce-hb run "), "{case}: {err}");
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reproduce_command_carries_guest_args_quoted() {
        let dir = work_dir("args");
        let out = run(&dir, "unhandled_svc", &["--", "two words"]);
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(14));
        assert!(
            err.contains(&format!(
                "To reproduce: ounce-hb run {} -- 'two words'",
                dir.join("demo.nro").display()
            )),
            "{err}"
        );
        let _ = fs::remove_dir_all(dir);
    }

    /// Finding 6, through the real binary: the replayed v0.8.1 spoof captures get the real category.
    #[test]
    fn spoofed_stop_lines_get_the_real_category() {
        let dir = work_dir("spoof");
        for (case, code) in [
            ("spoof_budget", 14),
            ("spoof_clean", 11),
            ("spoof_fault_then_clean", 0),
            ("spoof_break", 13),
        ] {
            let out = run(&dir, case, &[]);
            let err = String::from_utf8_lossy(&out.stderr).into_owned();
            assert_eq!(out.status.code(), Some(code), "{case}: {err}");
            if code == 13 {
                assert!(err.contains("Break PC: 0x0000000080000098"), "{err}");
                assert!(!err.contains("DEADBEEF"), "{err}");
            }
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unrecognised_output_exits_one_and_keeps_the_raw_text() {
        let dir = work_dir("unrec");
        let out = run(&dir, "usage", &[]);
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(1));
        assert!(err.contains("does not recognise"), "{err}");
        assert!(
            err.contains(include_str!("fixtures/ounce-core-output/usage.stderr").trim_end()),
            "{err}"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_core_is_exit_three_with_the_hint() {
        let dir = work_dir("missing");
        let out = Command::new(env!("CARGO_BIN_EXE_ounce-hb"))
            .env("OUNCE_CORE_BIN", dir.join("no-such-ounce-core"))
            .arg("run")
            .arg(fake_nro(&dir))
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(3));
        assert!(err.contains("OUNCE_CORE_BIN"), "{err}");
        assert!(err.contains("To reproduce: ounce-hb run "), "{err}");
        let _ = fs::remove_dir_all(dir);
    }
}
