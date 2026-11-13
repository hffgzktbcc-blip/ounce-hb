//! Explains why a run ended. ounce-core reports an abnormal stop as plain text on stdout and stderr
//! and exits non-zero. This module matches the exact lines listed in docs/DIAGNOSIS.md and nothing
//! else. A run whose output it does not recognise becomes [`StopReason::Unrecognised`], and no cause
//! is inferred from it.
//!
//! Guest text reaches the output too: `svcOutputDebugString` is printed on stdout as it is, so a
//! program can print a line that looks like a stop line. Two facts about ounce-core v0.8.1 make the
//! classification hold anyway (docs/DIAGNOSIS.md, "Guest text and the stop line"):
//!
//! - Guest text never reaches stderr. A stop line on stderr is ounce-core's own.
//! - ounce-core prints its stop line on stdout after the VM has stopped, so it is the last stop line
//!   there. Nothing the guest printed can come after it.
//!
//! So the stop line is the last recognised line on stderr if there is one, else the last recognised
//! line on stdout. `[VM Error]` and `[Loader Error]` lines are only taken from stderr, where
//! ounce-core prints them.

use std::fmt::Write as _;
use std::time::Duration;

use crate::{RunError, RunOutput};

/// ounce-core's line for a guest that reached svcExitProcess. It is printed on stdout.
const CLEAN_EXIT: &str = "[VM] Process exited gracefully (svcExitProcess).";

/// Why a run ended, with the facts ounce-core printed for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// The guest reached svcExitProcess and ounce-core exited 0.
    CleanExit,
    /// The request was refused before ounce-core ran: a guest argument starting with '-', or an
    /// input kind this runner does not execute.
    Refused { message: String },
    /// ounce-core could not be found. `message` says where it looked.
    CoreMissing { message: String },
    /// ounce-core could not be started or waited on, or a private copy of the program could not be
    /// written.
    HostIo { message: String },
    /// ounce-core ran past the wall-clock limit and was killed.
    Timeout { limit: Duration },
    /// `[VM Error] Unimplemented instruction 0x... at PC: 0x...`
    UnimplementedInstruction { word: u32, pc: u64 },
    /// `[VM Error] Memory access violation: ...`. `address` is set when the message carries one.
    MemoryFault {
        message: String,
        address: Option<u64>,
    },
    /// Any other `[VM Error] ...` line, such as a breakpoint or a branch-target exception.
    CpuError { message: String },
    /// `[VM] Hit svcBreak: ...`. `pc` and `result` come from the `[VM] Break PC` and
    /// `[VM] Break result` lines when they are present.
    Break {
        pc: Option<u64>,
        result: Option<u32>,
    },
    /// `[VM] Stopping on unhandled SVC #0x...`
    UnhandledSvc { id: u16 },
    /// `[VM] CPU Halted: ...`
    CpuHalted { message: String },
    /// `[VM] Execution budget reached (N instructions).`
    BudgetExhausted { instructions: u64 },
    /// `[Loader Error] ...`: ounce-core could not load the file.
    LoaderError { message: String },
    /// Anything else. `exit_code` is `None` when the process was killed by a signal. The tails
    /// hold the last [`EXCERPT_BYTES`] of each stream, for the record; the full output is printed
    /// by `ounce-hb run` before the explanation.
    Unrecognised {
        exit_code: Option<i32>,
        stdout_tail: String,
        stderr_tail: String,
    },
}

/// How much of the end of each stream [`StopReason::Unrecognised`] keeps.
pub const EXCERPT_BYTES: usize = 1024;

impl StopReason {
    /// The process exit code `ounce-hb run` uses for this outcome. Each value is documented in
    /// docs/DIAGNOSIS.md and must not change.
    pub fn exit_code(&self) -> u8 {
        match self {
            StopReason::CleanExit => 0,
            StopReason::Unrecognised { .. } => 1,
            StopReason::Refused { .. } => 2,
            StopReason::CoreMissing { .. } => 3,
            StopReason::Timeout { .. } => 4,
            StopReason::HostIo { .. } => 5,
            StopReason::UnimplementedInstruction { .. } => 10,
            StopReason::MemoryFault { .. } => 11,
            StopReason::CpuError { .. } => 12,
            StopReason::Break { .. } => 13,
            StopReason::UnhandledSvc { .. } => 14,
            StopReason::CpuHalted { .. } => 15,
            StopReason::BudgetExhausted { .. } => 16,
            StopReason::LoaderError { .. } => 17,
        }
    }

    /// Plain-words message for `ounce-hb run` to print after the guest's own output. Empty for a clean
    /// exit. Otherwise it names the category and the exit code, says why in one line, and ends with the
    /// `ounce-hb run` command that reproduces the run. `program` and `args` are copied as given.
    pub fn explain(&self, program: &str, args: &[String]) -> String {
        let (headline, detail) = match self {
            StopReason::CleanExit => return String::new(),
            StopReason::Refused { message } => ("the request was refused", message.clone()),
            StopReason::CoreMissing { message } => ("ounce-core was not found", message.clone()),
            StopReason::HostIo { message } => {
                ("ounce-hb could not run ounce-core", message.clone())
            }
            StopReason::Timeout { limit } => (
                "the run did not finish in time",
                format!(
                    "ounce-core was killed after {} s. The program may be waiting for input, looping, or slower than this limit.",
                    limit.as_secs_f64()
                ),
            ),
            StopReason::UnimplementedInstruction { word, pc } => (
                "Ounce Core does not implement an instruction",
                format!(
                    "The program reached instruction 0x{word:08X} at address 0x{pc:016X}. That is a gap in Ounce Core, not necessarily a bug in the program."
                ),
            ),
            StopReason::MemoryFault { message, .. } => (
                "the program made a memory access that Ounce Core rejected",
                format!("Ounce Core said: {message}."),
            ),
            StopReason::CpuError { message } => (
                "Ounce Core stopped the CPU with an error",
                format!("Ounce Core said: {message}."),
            ),
            StopReason::Break { pc, result } => {
                let mut text = String::from("svcBreak is how a program reports a fatal error.");
                if let Some(pc) = pc {
                    let _ = write!(text, " Break PC: 0x{pc:016X}.");
                }
                if let Some(result) = result {
                    let _ = write!(text, " Break result: 0x{result:08X}.");
                }
                ("the program stopped itself with svcBreak", text)
            }
            StopReason::UnhandledSvc { id } => (
                "the program asked for a system service that Ounce Core does not provide",
                format!(
                    "System call #0x{id:02X} is not implemented. See docs/svc-coverage.md in the ounce-core checkout."
                ),
            ),
            StopReason::CpuHalted { message } => {
                ("the CPU halted", format!("Ounce Core said: {message}."))
            }
            StopReason::BudgetExhausted { instructions } => (
                "the program ran out of instructions",
                format!(
                    "It did not exit within {instructions} instructions. For a long program, set OUNCE_NRO_BUDGET to a larger number and run again."
                ),
            ),
            StopReason::LoaderError { message } => (
                "Ounce Core could not load the program",
                format!("Ounce Core said: {message}."),
            ),
            StopReason::Unrecognised { exit_code, .. } => {
                let code = match exit_code {
                    Some(code) => code.to_string(),
                    None => "none (killed by a signal)".to_string(),
                };
                (
                    "ounce-core stopped in a way ounce-hb does not recognise",
                    format!(
                        "ounce-core exit code: {code}. Its output is printed above. Nothing was inferred from it, so keep that output when you report this."
                    ),
                )
            }
        };
        let mut reproduce = format!("ounce-hb run {}", shell_quote(program));
        if !args.is_empty() {
            reproduce.push_str(" --");
            for arg in args {
                reproduce.push(' ');
                reproduce.push_str(&shell_quote(arg));
            }
        }
        let code = self.exit_code();
        if matches!(self, StopReason::Refused { .. }) {
            format!("ounce-hb: {headline} (exit code {code}).\n  {detail}\n")
        } else {
            format!(
                "ounce-hb: {headline} (exit code {code}).\n  {detail}\n  To reproduce: {reproduce}\n"
            )
        }
    }
}

/// Maps an error from [`crate::run`] to a stop reason. Every [`RunError`] becomes one, so the caller
/// has a single path for exit codes.
pub fn from_error(e: &RunError) -> StopReason {
    match e {
        RunError::CoreNotFound(_) => StopReason::CoreMissing {
            message: e.to_string(),
        },
        RunError::Timeout(limit) => StopReason::Timeout { limit: *limit },
        RunError::Io(_) => StopReason::HostIo {
            message: e.to_string(),
        },
        RunError::UnsupportedArg(_)
        | RunError::UnsupportedKind(_)
        | RunError::BadTimeout(_)
        | RunError::GuestArgsTooLong { .. } => StopReason::Refused {
            message: e.to_string(),
        },
    }
}

/// Classifies one finished ounce-core run from its exit status and output. See the module notes
/// for why the last stop line is used and why stderr is read first.
pub fn from_output(out: &RunOutput) -> StopReason {
    let stderr: Vec<&str> = out.stderr.lines().map(str::trim).collect();
    let stdout: Vec<&str> = out.stdout.lines().map(str::trim).collect();
    let chosen = last_recognised(&stderr, Stream::Stderr)
        .or_else(|| last_recognised(&stdout, Stream::Stdout));
    match (out.exit_code, chosen) {
        (Some(0), Some((StopReason::CleanExit, _))) => StopReason::CleanExit,
        (Some(code), Some((reason, after))) if code != 0 => match reason {
            StopReason::CleanExit => unrecognised(out),
            StopReason::Break { .. } => break_details(after),
            reason => reason,
        },
        _ => unrecognised(out),
    }
}

/// Which stream a line came from. Guest text only ever reaches stdout.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stream {
    Stdout,
    Stderr,
}

/// The last line of `lines` that is a stop line ounce-core prints on `stream`, with the lines that
/// follow it (the svcBreak details are there).
fn last_recognised<'a>(
    lines: &'a [&'a str],
    stream: Stream,
) -> Option<(StopReason, &'a [&'a str])> {
    lines.iter().enumerate().rev().find_map(|(i, line)| {
        let reason = recognise(line)?;
        let printed_on_stderr = matches!(
            reason,
            StopReason::UnimplementedInstruction { .. }
                | StopReason::MemoryFault { .. }
                | StopReason::CpuError { .. }
                | StopReason::LoaderError { .. }
        );
        if printed_on_stderr && stream == Stream::Stdout {
            return None;
        }
        Some((reason, &lines[i + 1..]))
    })
}

/// Matches one line against the ounce-core stop lines, including the clean-exit line. `None` means
/// the line is not one of them, or it has the shape of one but its fields do not parse.
fn recognise(line: &str) -> Option<StopReason> {
    if line == CLEAN_EXIT {
        return Some(StopReason::CleanExit);
    }
    if let Some(rest) = line.strip_prefix("[VM Error] Unimplemented instruction 0x") {
        let (word, pc) = rest.split_once(" at PC: 0x")?;
        return Some(StopReason::UnimplementedInstruction {
            word: u32::from_str_radix(word, 16).ok()?,
            pc: u64::from_str_radix(pc, 16).ok()?,
        });
    }
    if let Some(message) = line.strip_prefix("[VM Error] Memory access violation: ") {
        let address =
            hex_after(message, "virtual address: 0x").or_else(|| hex_after(message, "at 0x"));
        return Some(StopReason::MemoryFault {
            message: message.to_string(),
            address,
        });
    }
    if let Some(message) = line.strip_prefix("[VM Error] ") {
        return Some(StopReason::CpuError {
            message: message.to_string(),
        });
    }
    if line.starts_with("[VM] Hit svcBreak:") {
        return Some(StopReason::Break {
            pc: None,
            result: None,
        });
    }
    if let Some(rest) = line.strip_prefix("[VM] Stopping on unhandled SVC #0x") {
        return Some(StopReason::UnhandledSvc {
            id: u16::try_from(leading_hex(rest)?).ok()?,
        });
    }
    if let Some(message) = line.strip_prefix("[VM] CPU Halted: ") {
        return Some(StopReason::CpuHalted {
            message: message.to_string(),
        });
    }
    if let Some(rest) = line.strip_prefix("[VM] Execution budget reached (") {
        let instructions = rest.split(' ').next()?.parse().ok()?;
        return Some(StopReason::BudgetExhausted { instructions });
    }
    if let Some(message) = line.strip_prefix("[Loader Error] ") {
        return Some(StopReason::LoaderError {
            message: message.to_string(),
        });
    }
    None
}

/// Reads the `[VM] Break PC` and `[VM] Break result` lines from `lines`, the lines after the
/// svcBreak line. Earlier lines are not read, so a guest cannot plant them. A result line that says
/// `unreadable` leaves `result` empty.
fn break_details(lines: &[&str]) -> StopReason {
    let pc = lines
        .iter()
        .find_map(|line| line.strip_prefix("[VM] Break PC: 0x").and_then(leading_hex));
    let result = lines
        .iter()
        .find_map(|line| {
            line.strip_prefix("[VM] Break result: 0x")
                .and_then(leading_hex)
        })
        .and_then(|v| u32::try_from(v).ok());
    StopReason::Break { pc, result }
}

fn unrecognised(out: &RunOutput) -> StopReason {
    StopReason::Unrecognised {
        exit_code: out.exit_code,
        stdout_tail: tail(&out.stdout, EXCERPT_BYTES),
        stderr_tail: tail(&out.stderr, EXCERPT_BYTES),
    }
}

/// The last `max` bytes of `text`, cut at a character boundary.
fn tail(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

/// A line for `ounce-hb run` to print after the explanation when ounce-core's output was cut to fit
/// [`crate::MAX_CAPTURED_BYTES`] per stream. `None` when nothing was dropped.
pub fn truncation_note(out: &RunOutput) -> Option<String> {
    if !out.truncated() {
        return None;
    }
    Some(format!(
        "ounce-hb: ounce-core wrote more than ounce-hb keeps ({} bytes per stream): {} bytes of stdout and {} bytes of stderr were dropped where the output above says so. The diagnosis used the kept parts only.\n",
        crate::MAX_CAPTURED_BYTES,
        out.stdout_dropped,
        out.stderr_dropped
    ))
}

/// The value of the hex digits at the start of `text`. `None` when there are none or they overflow.
fn leading_hex(text: &str) -> Option<u64> {
    let digits: &str = &text[..text
        .find(|c: char| !c.is_ascii_hexdigit())
        .unwrap_or(text.len())];
    if digits.is_empty() {
        return None;
    }
    u64::from_str_radix(digits, 16).ok()
}

/// The hex value that follows `marker` in `text`.
fn hex_after(text: &str, marker: &str) -> Option<u64> {
    let (_, rest) = text.split_once(marker)?;
    leading_hex(rest)
}

/// Quotes one word for a POSIX shell. Plain words are left as they are.
fn shell_quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=+,@%".contains(c));
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_helpers_read_only_leading_digits() {
        assert_eq!(leading_hex("7E."), Some(0x7E));
        assert_eq!(leading_hex("0000F601 (module=1)"), Some(0xF601));
        assert_eq!(leading_hex("x1"), None);
        assert_eq!(leading_hex(""), None);
        assert_eq!(
            hex_after("at virtual address: 0xFF", "virtual address: 0x"),
            Some(0xFF)
        );
        assert_eq!(hex_after("nothing here", "at 0x"), None);
    }

    #[test]
    fn tail_keeps_the_end_at_a_character_boundary() {
        assert_eq!(tail("abc", 10), "abc");
        assert_eq!(tail("abcdef", 3), "def");
        assert_eq!(tail("aé", 1), "");
        assert_eq!(tail("aé", 2), "é");
    }

    #[test]
    fn truncation_note_is_only_given_when_bytes_were_dropped() {
        let out = RunOutput {
            exit_code: Some(0),
            ..RunOutput::default()
        };
        assert_eq!(truncation_note(&out), None);
        let out = RunOutput {
            stdout_dropped: 5,
            ..RunOutput::default()
        };
        let note = truncation_note(&out).unwrap();
        assert!(note.contains("5 bytes of stdout"), "{note}");
        assert!(
            note.contains(&crate::MAX_CAPTURED_BYTES.to_string()),
            "{note}"
        );
    }

    #[test]
    fn shell_quote_leaves_plain_words_and_quotes_the_rest() {
        assert_eq!(shell_quote("a/b.nro"), "a/b.nro");
        assert_eq!(shell_quote("my game.nro"), "'my game.nro'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn malformed_shape_is_not_guessed_at() {
        assert_eq!(
            recognise("[VM Error] Unimplemented instruction 0xZZ at PC: 0x10"),
            None
        );
        assert_eq!(
            recognise("[VM] Execution budget reached (many instructions)."),
            None
        );
        assert_eq!(recognise("[Horizon SVC] Unhandled SVC #0x7E"), None);
    }

    #[test]
    fn exit_codes_are_distinct_per_category() {
        let samples = [
            StopReason::CleanExit,
            StopReason::Unrecognised {
                exit_code: None,
                stdout_tail: String::new(),
                stderr_tail: String::new(),
            },
            StopReason::Refused {
                message: String::new(),
            },
            StopReason::CoreMissing {
                message: String::new(),
            },
            StopReason::Timeout {
                limit: Duration::from_secs(1),
            },
            StopReason::HostIo {
                message: String::new(),
            },
            StopReason::UnimplementedInstruction { word: 0, pc: 0 },
            StopReason::MemoryFault {
                message: String::new(),
                address: None,
            },
            StopReason::CpuError {
                message: String::new(),
            },
            StopReason::Break {
                pc: None,
                result: None,
            },
            StopReason::UnhandledSvc { id: 0 },
            StopReason::CpuHalted {
                message: String::new(),
            },
            StopReason::BudgetExhausted { instructions: 0 },
            StopReason::LoaderError {
                message: String::new(),
            },
        ];
        let mut codes: Vec<u8> = samples.iter().map(StopReason::exit_code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), samples.len());
        assert_eq!(StopReason::CleanExit.exit_code(), 0);
    }

    #[test]
    fn explain_is_empty_for_a_clean_exit_and_ends_with_the_reproduce_command() {
        assert_eq!(StopReason::CleanExit.explain("p.nro", &[]), "");
        let text =
            StopReason::UnhandledSvc { id: 0x7E }.explain("my game.nro", &["a b".to_string()]);
        assert!(text.contains("(exit code 14)"), "{text}");
        assert!(
            text.ends_with("To reproduce: ounce-hb run 'my game.nro' -- 'a b'\n"),
            "{text}"
        );
    }

    #[test]
    fn refusals_are_not_given_a_reproduce_command() {
        let text = StopReason::Refused {
            message: "bad".to_string(),
        }
        .explain("p.nro", &[]);
        assert!(!text.contains("To reproduce"), "{text}");
    }
}
