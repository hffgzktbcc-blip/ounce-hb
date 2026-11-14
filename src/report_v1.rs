//! Zero-dependency parser for the `ounce.run.v1` report contract and `repro.json` writer.
//! Keeps `ounce-hb` dependency-free and `#![forbid(unsafe_code)]` while isolating `MIT` and
//! `GPL-3.0-or-later` engines behind a pure data contract.

use crate::diagnose::StopReason;

/// Parsed `ounce.run.v1` payload emitted by `--report-json=<path>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportV1 {
    pub schema: String,
    pub engine: String,
    pub engine_version: String,
    pub tier: String,
    pub silent_fallback: bool,
    pub stop_kind: String,
    pub exit_code: i32,
    pub pc: Option<u64>,
    pub opcode: Option<u32>,
    pub fault_addr: Option<u64>,
    pub svc_id: Option<u32>,
    pub break_result: Option<u32>,
    pub insns_executed: u64,
    pub message: Option<String>,
}

impl ReportV1 {
    /// Parse a flat `ounce.run.v1` JSON object. Returns `None` if `schema != "ounce.run.v1"`.
    pub fn parse(json: &str) -> Option<Self> {
        let schema = extract_str(json, "schema")?;
        if schema != "ounce.run.v1" {
            return None;
        }
        Some(Self {
            schema,
            engine: extract_str(json, "engine")?,
            engine_version: extract_str(json, "engine_version").unwrap_or_default(),
            tier: extract_str(json, "tier").unwrap_or_else(|| "unknown".to_string()),
            silent_fallback: extract_bool(json, "silent_fallback").unwrap_or(false),
            stop_kind: extract_str(json, "stop_kind")?,
            exit_code: extract_u64(json, "exit_code").unwrap_or(1) as i32,
            pc: extract_u64(json, "pc"),
            opcode: extract_u64(json, "opcode").map(|v| v as u32),
            fault_addr: extract_u64(json, "fault_addr"),
            svc_id: extract_u64(json, "svc_id").map(|v| v as u32),
            break_result: extract_u64(json, "break_result").map(|v| v as u32),
            insns_executed: extract_u64(json, "insns_executed").unwrap_or(0),
            message: extract_str(json, "message"),
        })
    }

    /// Map structured `ounce.run.v1` fields directly to `StopReason` without text scraping.
    pub fn to_stop_reason(&self, fallback_stdout: &str, fallback_stderr: &str) -> StopReason {
        match self.stop_kind.as_str() {
            "clean_exit" | "window_closed" | "frame_limit" => StopReason::CleanExit,
            "unimplemented_insn" => StopReason::UnimplementedInstruction {
                word: self.opcode.unwrap_or(0),
                pc: self.pc.unwrap_or(0),
            },
            "memory_fault" => StopReason::MemoryFault {
                message: self
                    .message
                    .clone()
                    .unwrap_or_else(|| "Memory access violation".to_string()),
                address: self.fault_addr,
            },
            "cpu_error" => StopReason::CpuError {
                message: self
                    .message
                    .clone()
                    .unwrap_or_else(|| "CPU fault".to_string()),
            },
            "svc_break" => StopReason::Break {
                pc: self.pc,
                result: self.break_result,
            },
            "unhandled_svc" => StopReason::UnhandledSvc {
                id: self.svc_id.unwrap_or(0) as u16,
            },
            "cpu_halted" => StopReason::CpuHalted {
                message: self
                    .message
                    .clone()
                    .unwrap_or_else(|| "CPU halted".to_string()),
            },
            "budget_exhausted" => StopReason::BudgetExhausted {
                instructions: self.insns_executed,
            },
            "loader_error" => StopReason::LoaderError {
                message: self
                    .message
                    .clone()
                    .unwrap_or_else(|| "Loader error".to_string()),
            },
            _ => StopReason::Unrecognised {
                exit_code: Some(self.exit_code),
                stdout_tail: fallback_stdout.to_string(),
                stderr_tail: fallback_stderr.to_string(),
            },
        }
    }
}

/// Format a self-contained `repro.json` bundle when `runtime` diverges from `core`.
pub fn format_repro_bundle(
    program_path: &str,
    program_len: usize,
    guest_args: &[String],
    runtime_report: Option<&ReportV1>,
    core_report: Option<&ReportV1>,
    runtime_exit: Option<i32>,
    core_exit: Option<i32>,
) -> String {
    let args_json: Vec<String> = guest_args
        .iter()
        .map(|a| format!("\"{}\"", escape_json(a)))
        .collect();
    let rt_kind = runtime_report
        .map(|r| format!("\"{}\"", escape_json(&r.stop_kind)))
        .unwrap_or_else(|| "null".to_string());
    let core_kind = core_report
        .map(|r| format!("\"{}\"", escape_json(&r.stop_kind)))
        .unwrap_or_else(|| "null".to_string());
    let rt_tier = runtime_report
        .map(|r| format!("\"{}\"", escape_json(&r.tier)))
        .unwrap_or_else(|| "null".to_string());
    let rt_pc = runtime_report
        .and_then(|r| r.pc)
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());
    let core_pc = core_report
        .and_then(|r| r.pc)
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());
    let rt_insns = runtime_report
        .map(|r| r.insns_executed.to_string())
        .unwrap_or_else(|| "null".to_string());
    let core_insns = core_report
        .map(|r| r.insns_executed.to_string())
        .unwrap_or_else(|| "null".to_string());
    let rt_exit = runtime_exit
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());
    let core_exit_str = core_exit
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());

    format!(
        "{{\n  \"schema\": \"ounce.repro.v1\",\n  \"program_path\": \"{}\",\n  \"program_bytes\": {},\n  \"guest_args\": [{}],\n  \"runtime\": {{\n    \"tier\": {},\n    \"stop_kind\": {},\n    \"exit_code\": {},\n    \"pc\": {},\n    \"insns_executed\": {}\n  }},\n  \"core\": {{\n    \"stop_kind\": {},\n    \"exit_code\": {},\n    \"pc\": {},\n    \"insns_executed\": {}\n  }}\n}}\n",
        escape_json(program_path),
        program_len,
        args_json.join(", "),
        rt_tier,
        rt_kind,
        rt_exit,
        rt_pc,
        rt_insns,
        core_kind,
        core_exit_str,
        core_pc,
        core_insns,
    )
}

/// Parsed `ounce.repro.v1` regression manifest for `ounce-hb repro`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReproManifest {
    pub program_path: String,
    pub guest_args: Vec<String>,
    pub expected_stop_kind: Option<String>,
    pub expected_exit_code: Option<i32>,
}

impl ReproManifest {
    /// Parse an `ounce.repro.v1` JSON manifest. Returns `None` if schema does not match.
    pub fn parse(json: &str) -> Option<Self> {
        let schema = extract_str(json, "schema")?;
        if schema != "ounce.repro.v1" {
            return None;
        }
        let program_path = extract_str(json, "program_path")?;
        let guest_args = extract_str_array(json, "guest_args").unwrap_or_default();
        let core_section = extract_object_section(json, "core");
        let expected_stop_kind = core_section.and_then(|s| extract_str(s, "stop_kind"));
        let expected_exit_code =
            core_section.and_then(|s| extract_u64(s, "exit_code").map(|v| v as i32));
        Some(Self {
            program_path,
            guest_args,
            expected_stop_kind,
            expected_exit_code,
        })
    }
}

fn extract_object_section<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let rest = find_field_value(json, key)?;
    if !rest.starts_with('{') {
        return None;
    }
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for (idx, ch) in rest.char_indices() {
        if in_str {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[..=idx]);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_str_array(json: &str, key: &str) -> Option<Vec<String>> {
    let rest = find_field_value(json, key)?.strip_prefix('[')?;
    let mut items = Vec::new();
    let mut cur = rest.trim_start();
    while !cur.starts_with(']') {
        let after_quote = cur.strip_prefix('"')?;
        let mut val = String::new();
        let mut consumed = 0usize;
        let mut chars = after_quote.char_indices();
        let mut closed = false;
        while let Some((idx, ch)) = chars.next() {
            match ch {
                '"' => {
                    consumed = idx + 1;
                    closed = true;
                    break;
                }
                '\\' => {
                    let (_, esc) = chars.next()?;
                    match esc {
                        '"' => val.push('"'),
                        '\\' => val.push('\\'),
                        'n' => val.push('\n'),
                        'r' => val.push('\r'),
                        't' => val.push('\t'),
                        other => val.push(other),
                    }
                }
                c => val.push(c),
            }
        }
        if !closed {
            return None;
        }
        items.push(val);
        cur = after_quote[consumed..].trim_start();
        if let Some(next) = cur.strip_prefix(',') {
            cur = next.trim_start();
        }
    }
    Some(items)
}

pub fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn find_field_value<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("\"{key}\"");
    let mut offset = 0;
    while let Some(idx) = json[offset..].find(&pattern) {
        let after_key = offset + idx + pattern.len();
        let rest = json[after_key..].trim_start();
        if let Some(after_colon) = rest.strip_prefix(':') {
            return Some(after_colon.trim_start());
        }
        offset = after_key;
    }
    None
}

fn extract_str(json: &str, key: &str) -> Option<String> {
    let rest = find_field_value(json, key)?;
    if rest.starts_with("null") {
        return None;
    }
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

fn extract_bool(json: &str, key: &str) -> Option<bool> {
    let rest = find_field_value(json, key)?;
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn extract_u64(json: &str, key: &str) -> Option<u64> {
    let rest = find_field_value(json, key)?;
    if rest.starts_with("null") {
        return None;
    }
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ounce_run_v1_and_maps_stop_reason() {
        let sample = r#"{
  "schema": "ounce.run.v1",
  "engine": "ounce-core",
  "engine_version": "0.8.2",
  "tier": "interpreter",
  "silent_fallback": false,
  "stop_kind": "unimplemented_insn",
  "exit_code": 10,
  "pc": 2147483904,
  "opcode": 3735928559,
  "fault_addr": null,
  "svc_id": null,
  "break_result": null,
  "insns_executed": 15304,
  "message": "Unimplemented instruction \"foo\""
}"#;
        let parsed = ReportV1::parse(sample).expect("valid report");
        assert_eq!(parsed.engine, "ounce-core");
        assert_eq!(parsed.insns_executed, 15304);
        assert_eq!(parsed.opcode, Some(3735928559));
        assert_eq!(
            parsed.to_stop_reason("", ""),
            StopReason::UnimplementedInstruction {
                word: 3735928559,
                pc: 2147483904,
            }
        );
    }

    #[test]
    fn rejects_wrong_schema() {
        assert!(ReportV1::parse(r#"{"schema": "other.v0"}"#).is_none());
    }
}
