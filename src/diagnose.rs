//! Explains why a run ended. ounce-core reports an abnormal stop as plain text on stdout and stderr
//! and exits non-zero. This module matches the exact lines listed in docs/DIAGNOSIS.md and nothing
//! else. A run whose output it does not recognise becomes [`StopReason::Unrecognised`], with
//! ounce-core's output kept unchanged, and no cause is inferred from it.

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
    /// Anything else. `stdout` and `stderr` are ounce-core's output, unchanged. `exit_code` is
    /// `None` when the process was killed by a signal.
    Unrecognised {
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
    },
}

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
                        "ounce-core exit code: {code}. Its output is printed above unchanged. Nothing was inferred from it, so keep that output when you report this."
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
        RunError::UnsupportedArg(_) | RunError::UnsupportedKind(_) | RunError::BadTimeout(_) => {
            StopReason::Refused {
                message: e.to_string(),
            }
        }
    }
}

/// Classifies one finished ounce-core run from its exit status and output.
pub fn from_output(out: &RunOutput) -> StopReason {
    let lines: Vec<&str> = out
        .stdout
        .lines()
        .chain(out.stderr.lines())
        .map(str::trim)
        .collect();
    let clean = lines.contains(&CLEAN_EXIT);
    let fault = lines.iter().find_map(|line| recognise(line));
    match out.exit_code {
        Some(0) if clean && fault.is_none() => StopReason::CleanExit,
        Some(code) if code != 0 && !clean => match fault {
            Some(StopReason::Break { .. }) => break_details(&lines),
            Some(reason) => reason,
            None => unrecognised(out),
        },
        _ => unrecognised(out),
    }
}

/// Matches one line against the ounce-core stop lines. `None` means the line is not one of them, or
/// it has the shape of one but its fields do not parse.
fn recognise(line: &str) -> Option<StopReason> {
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

/// Reads the `[VM] Break PC` and `[VM] Break result` lines that follow a svcBreak line. A result
/// line that says `unreadable` leaves `result` empty.
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
        stdout: out.stdout.clone(),
        stderr: out.stderr.clone(),
    }
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
                stdout: String::new(),
                stderr: String::new(),
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
