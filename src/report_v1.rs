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
    /// Frames the engine presented; `0` when absent (older engines) or when none were presented.
    pub frames_presented: u64,
    /// `fnv1a64:<16 hex>` of the last presented frame before any overlay; `None` when absent or `null`.
    pub framebuffer_hash: Option<String>,
    pub message: Option<String>,
}

impl ReportV1 {
    /// Parse an `ounce.run.v1` JSON object. Returns `None` if invalid JSON or `schema != "ounce.run.v1"`.
    pub fn parse(json: &str) -> Option<Self> {
        let root = parse_json(json)?;
        let schema = root.field("schema")?.as_str()?.to_string();
        if schema != "ounce.run.v1" {
            return None;
        }
        Some(Self {
            schema,
            engine: root.field("engine")?.as_str()?.to_string(),
            engine_version: root
                .field("engine_version")
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_string(),
            tier: root
                .field("tier")
                .and_then(JsonValue::as_str)
                .unwrap_or("unknown")
                .to_string(),
            silent_fallback: root
                .field("silent_fallback")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false),
            stop_kind: root.field("stop_kind")?.as_str()?.to_string(),
            exit_code: root
                .field("exit_code")
                .and_then(JsonValue::as_i32)
                .unwrap_or(1),
            pc: root.field("pc").and_then(JsonValue::as_u64),
            opcode: root.field("opcode").and_then(JsonValue::as_u32),
            fault_addr: root.field("fault_addr").and_then(JsonValue::as_u64),
            svc_id: root.field("svc_id").and_then(JsonValue::as_u32),
            break_result: root.field("break_result").and_then(JsonValue::as_u32),
            insns_executed: root
                .field("insns_executed")
                .and_then(JsonValue::as_u64)
                .unwrap_or(0),
            frames_presented: root
                .field("frames_presented")
                .and_then(JsonValue::as_u64)
                .unwrap_or(0),
            framebuffer_hash: root
                .field("framebuffer_hash")
                .and_then(JsonValue::as_str)
                .map(str::to_string),
            message: root
                .field("message")
                .and_then(JsonValue::as_str)
                .map(str::to_string),
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
    format_repro_bundle_with_opts(
        program_path,
        program_len,
        guest_args,
        None,
        None,
        runtime_report,
        core_report,
        runtime_exit,
        core_exit,
    )
}

/// Format a self-contained `repro.json` bundle with optional `--cpu` and `--sdmc` settings.
#[allow(clippy::too_many_arguments)]
pub fn format_repro_bundle_with_opts(
    program_path: &str,
    program_len: usize,
    guest_args: &[String],
    cpu: Option<crate::CpuTier>,
    sdmc: Option<&str>,
    runtime_report: Option<&ReportV1>,
    core_report: Option<&ReportV1>,
    runtime_exit: Option<i32>,
    core_exit: Option<i32>,
) -> String {
    let args_json: Vec<String> = guest_args
        .iter()
        .map(|a| format!("\"{}\"", escape_json(a)))
        .collect();
    let cpu_json = cpu
        .map(|c| format!("\"{}\"", c.as_str()))
        .unwrap_or_else(|| "null".to_string());
    let sdmc_json = sdmc
        .map(|s| format!("\"{}\"", escape_json(s)))
        .unwrap_or_else(|| "null".to_string());
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
    let frames = |r: Option<&ReportV1>| {
        r.map(|r| r.frames_presented.to_string())
            .unwrap_or_else(|| "null".to_string())
    };
    let hash = |r: Option<&ReportV1>| {
        r.and_then(|r| r.framebuffer_hash.as_ref())
            .map(|h| format!("\"{}\"", escape_json(h)))
            .unwrap_or_else(|| "null".to_string())
    };
    let (rt_frames, core_frames) = (frames(runtime_report), frames(core_report));
    let (rt_hash, core_hash) = (hash(runtime_report), hash(core_report));
    let rt_exit = runtime_exit
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());
    let core_exit_str = core_exit
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());

    format!(
        "{{\n  \"schema\": \"ounce.repro.v1\",\n  \"program_path\": \"{}\",\n  \"program_bytes\": {},\n  \"guest_args\": [{}],\n  \"cpu\": {},\n  \"sdmc\": {},\n  \"runtime\": {{\n    \"tier\": {},\n    \"stop_kind\": {},\n    \"exit_code\": {},\n    \"pc\": {},\n    \"insns_executed\": {},\n    \"frames_presented\": {},\n    \"framebuffer_hash\": {}\n  }},\n  \"core\": {{\n    \"stop_kind\": {},\n    \"exit_code\": {},\n    \"pc\": {},\n    \"insns_executed\": {},\n    \"frames_presented\": {},\n    \"framebuffer_hash\": {}\n  }}\n}}\n",
        escape_json(program_path),
        program_len,
        args_json.join(", "),
        cpu_json,
        sdmc_json,
        rt_tier,
        rt_kind,
        rt_exit,
        rt_pc,
        rt_insns,
        rt_frames,
        rt_hash,
        core_kind,
        core_exit_str,
        core_pc,
        core_insns,
        core_frames,
        core_hash,
    )
}

/// Parsed `ounce.repro.v1` regression manifest for `ounce-hb repro`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReproManifest {
    pub program_path: String,
    pub guest_args: Vec<String>,
    pub cpu: Option<crate::CpuTier>,
    pub sdmc: Option<String>,
    pub expected_stop_kind: Option<String>,
    pub expected_exit_code: Option<i32>,
    /// From the manifest's `core` section; checked by `ounce-hb repro` only when present.
    pub expected_frames_presented: Option<u64>,
    pub expected_framebuffer_hash: Option<String>,
}

impl ReproManifest {
    /// Parse an `ounce.repro.v1` JSON manifest. Returns `None` if invalid JSON or schema does not match.
    pub fn parse(json: &str) -> Option<Self> {
        let root = parse_json(json)?;
        if root.field("schema")?.as_str()? != "ounce.repro.v1" {
            return None;
        }
        let program_path = root.field("program_path")?.as_str()?.to_string();
        let guest_args = root
            .field("guest_args")
            .and_then(JsonValue::as_str_array)
            .unwrap_or_default();
        let cpu = root
            .field("cpu")
            .and_then(JsonValue::as_str)
            .and_then(crate::CpuTier::parse);
        let sdmc = root
            .field("sdmc")
            .and_then(JsonValue::as_str)
            .map(str::to_string);
        let core = root.field("core");
        let expected_stop_kind = core
            .and_then(|c| c.field("stop_kind"))
            .and_then(JsonValue::as_str)
            .map(str::to_string);
        let expected_exit_code = core
            .and_then(|c| c.field("exit_code"))
            .and_then(JsonValue::as_i32);
        let expected_frames_presented = core
            .and_then(|c| c.field("frames_presented"))
            .and_then(JsonValue::as_u64);
        let expected_framebuffer_hash = core
            .and_then(|c| c.field("framebuffer_hash"))
            .and_then(JsonValue::as_str)
            .map(str::to_string);
        Some(Self {
            program_path,
            guest_args,
            cpu,
            sdmc,
            expected_stop_kind,
            expected_exit_code,
            expected_frames_presented,
            expected_framebuffer_hash,
        })
    }
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

const MAX_JSON_DEPTH: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
enum JsonNumber {
    Int(i128),
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(JsonNumber),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    fn as_object(&self) -> Option<&[(String, JsonValue)]> {
        match self {
            JsonValue::Object(entries) => Some(entries),
            _ => None,
        }
    }

    fn field(&self, key: &str) -> Option<&JsonValue> {
        self.as_object()?
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn as_i32(&self) -> Option<i32> {
        match self {
            JsonValue::Number(JsonNumber::Int(n)) => i32::try_from(*n).ok(),
            _ => None,
        }
    }

    fn as_u32(&self) -> Option<u32> {
        match self {
            JsonValue::Number(JsonNumber::Int(n)) => u32::try_from(*n).ok(),
            _ => None,
        }
    }

    fn as_u64(&self) -> Option<u64> {
        match self {
            JsonValue::Number(JsonNumber::Int(n)) => u64::try_from(*n).ok(),
            _ => None,
        }
    }

    fn as_str_array(&self) -> Option<Vec<String>> {
        match self {
            JsonValue::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(item.as_str()?.to_string());
                }
                Some(out)
            }
            _ => None,
        }
    }
}

struct Parser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect_byte(&mut self, expected: u8) -> Option<()> {
        if self.peek() == Some(expected) {
            self.pos += 1;
            Some(())
        } else {
            None
        }
    }

    fn consume_bytes(&mut self, expected: &[u8]) -> bool {
        let Some(end) = self.pos.checked_add(expected.len()) else {
            return false;
        };
        if self.bytes.get(self.pos..end) == Some(expected) {
            self.pos = end;
            true
        } else {
            false
        }
    }

    fn parse_value(&mut self, depth: usize) -> Option<JsonValue> {
        if depth > MAX_JSON_DEPTH {
            return None;
        }
        self.skip_ws();
        match self.peek()? {
            b'n' => self.consume_bytes(b"null").then_some(JsonValue::Null),
            b't' => self.consume_bytes(b"true").then_some(JsonValue::Bool(true)),
            b'f' => self
                .consume_bytes(b"false")
                .then_some(JsonValue::Bool(false)),
            b'"' => self.parse_string().map(JsonValue::String),
            b'{' => self.parse_object(depth + 1).map(JsonValue::Object),
            b'[' => self.parse_array(depth + 1).map(JsonValue::Array),
            b'-' | b'0'..=b'9' => self.parse_number().map(JsonValue::Number),
            _ => None,
        }
    }

    fn parse_object(&mut self, depth: usize) -> Option<Vec<(String, JsonValue)>> {
        if depth > MAX_JSON_DEPTH {
            return None;
        }
        self.expect_byte(b'{')?;
        self.skip_ws();
        let mut entries = Vec::new();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Some(entries);
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return None;
            }
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect_byte(b':')?;
            let val = self.parse_value(depth)?;
            entries.push((key, val));
            self.skip_ws();
            match self.peek()? {
                b',' => self.pos += 1,
                b'}' => {
                    self.pos += 1;
                    return Some(entries);
                }
                _ => return None,
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Option<Vec<JsonValue>> {
        if depth > MAX_JSON_DEPTH {
            return None;
        }
        self.expect_byte(b'[')?;
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Some(items);
        }
        loop {
            items.push(self.parse_value(depth)?);
            self.skip_ws();
            match self.peek()? {
                b',' => self.pos += 1,
                b']' => {
                    self.pos += 1;
                    return Some(items);
                }
                _ => return None,
            }
        }
    }

    fn parse_string(&mut self) -> Option<String> {
        self.expect_byte(b'"')?;
        let mut out = String::new();
        loop {
            let start = self.pos;
            while let Some(b) = self.peek() {
                if !(0x20..0x80).contains(&b) || b == b'"' || b == b'\\' {
                    break;
                }
                self.pos += 1;
            }
            if self.pos > start {
                out.push_str(&self.src[start..self.pos]);
            }
            let b = self.peek()?;
            match b {
                b'"' => {
                    self.pos += 1;
                    return Some(out);
                }
                b'\\' => {
                    self.pos += 1;
                    let esc = self.peek()?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\x08'),
                        b'f' => out.push('\x0C'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.parse_unicode_escape()?),
                        _ => return None,
                    }
                }
                0..=0x1F => return None,
                _ => {
                    let ch = self.src[self.pos..].chars().next()?;
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn parse_unicode_escape(&mut self) -> Option<char> {
        let code = self.parse_hex4()?;
        if (0xD800..=0xDBFF).contains(&code) {
            let end = self.pos.checked_add(2)?;
            if self.bytes.get(self.pos..end) != Some(b"\\u") {
                return None;
            }
            self.pos = end;
            let low = self.parse_hex4()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return None;
            }
            let cp = 0x10000 + ((u32::from(code) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
            char::from_u32(cp)
        } else if (0xDC00..=0xDFFF).contains(&code) {
            None
        } else {
            char::from_u32(u32::from(code))
        }
    }

    fn parse_hex4(&mut self) -> Option<u16> {
        let end = self.pos.checked_add(4)?;
        let slice = self.bytes.get(self.pos..end)?;
        let mut val = 0u16;
        for &b in slice {
            let digit = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return None,
            };
            val = (val << 4) | u16::from(digit);
        }
        self.pos = end;
        Some(val)
    }

    fn parse_number(&mut self) -> Option<JsonNumber> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek()? {
            b'0' => {
                self.pos += 1;
                if self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    return None;
                }
            }
            b'1'..=b'9' => {
                self.pos += 1;
                while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
            _ => return None,
        }

        let mut is_int = true;
        if self.peek() == Some(b'.') {
            is_int = false;
            self.pos += 1;
            if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
                return None;
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_int = false;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
                return None;
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.pos += 1;
            }
        }

        let raw = &self.src[start..self.pos];
        if is_int && let Ok(n) = raw.parse::<i128>() {
            Some(JsonNumber::Int(n))
        } else {
            Some(JsonNumber::Other)
        }
    }
}

fn parse_json(input: &str) -> Option<JsonValue> {
    let mut parser = Parser {
        src: input,
        bytes: input.as_bytes(),
        pos: 0,
    };
    let val = parser.parse_value(0)?;
    parser.skip_ws();
    if parser.pos != parser.bytes.len() {
        return None;
    }
    Some(val)
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
    fn parses_frames_presented_and_framebuffer_hash() {
        let with = r#"{"schema": "ounce.run.v1", "engine": "ounce-core", "stop_kind": "frame_limit",
  "exit_code": 0, "insns_executed": 9, "frames_presented": 5, "framebuffer_hash": "fnv1a64:3a00ccf6b6f28ea1",
  "message": null}"#;
        let r = ReportV1::parse(with).unwrap();
        assert_eq!(r.frames_presented, 5);
        assert_eq!(
            r.framebuffer_hash.as_deref(),
            Some("fnv1a64:3a00ccf6b6f28ea1")
        );
        // null hash, and reports from engines that predate the fields
        let null = r#"{"schema": "ounce.run.v1", "engine": "e", "stop_kind": "clean_exit", "frames_presented": 0, "framebuffer_hash": null}"#;
        let r = ReportV1::parse(null).unwrap();
        assert_eq!((r.frames_presented, r.framebuffer_hash), (0, None));
        let old = r#"{"schema": "ounce.run.v1", "engine": "e", "stop_kind": "clean_exit"}"#;
        let r = ReportV1::parse(old).unwrap();
        assert_eq!((r.frames_presented, r.framebuffer_hash), (0, None));
        // a message that merely mentions the key must not be read as the field
        let tricky = r#"{"schema": "ounce.run.v1", "engine": "e", "stop_kind": "clean_exit", "message": "saw \"framebuffer_hash\" in log", "framebuffer_hash": "fnv1a64:0000000000000001"}"#;
        assert_eq!(
            ReportV1::parse(tricky).unwrap().framebuffer_hash.as_deref(),
            Some("fnv1a64:0000000000000001")
        );
    }

    #[test]
    fn repro_bundle_round_trips_hash_and_frames_from_the_core_section() {
        let mk = |frames, hash: Option<&str>| ReportV1 {
            schema: "ounce.run.v1".into(),
            engine: "e".into(),
            engine_version: String::new(),
            tier: "interpreter".into(),
            silent_fallback: false,
            stop_kind: "frame_limit".into(),
            exit_code: 0,
            pc: None,
            opcode: None,
            fault_addr: None,
            svc_id: None,
            break_result: None,
            insns_executed: 1,
            frames_presented: frames,
            framebuffer_hash: hash.map(str::to_string),
            message: None,
        };
        let rt = mk(5, Some("fnv1a64:aaaaaaaaaaaaaaaa"));
        let core = mk(5, Some("fnv1a64:bbbbbbbbbbbbbbbb"));
        let json = format_repro_bundle("p.nro", 1, &[], Some(&rt), Some(&core), Some(0), Some(0));
        let m = ReproManifest::parse(&json).unwrap();
        assert_eq!(m.expected_frames_presented, Some(5));
        assert_eq!(
            m.expected_framebuffer_hash.as_deref(),
            Some("fnv1a64:bbbbbbbbbbbbbbbb")
        );
        // no hash on either side stays null and parses as absent
        let json = format_repro_bundle(
            "p.nro",
            1,
            &[],
            Some(&mk(0, None)),
            Some(&mk(0, None)),
            Some(0),
            Some(0),
        );
        let m = ReproManifest::parse(&json).unwrap();
        assert_eq!(
            (m.expected_frames_presented, m.expected_framebuffer_hash),
            (Some(0), None)
        );
    }

    #[test]
    fn repro_bundle_round_trips_cpu_and_sdmc() {
        let mk = |frames, hash: Option<&str>| ReportV1 {
            schema: "ounce.run.v1".into(),
            engine: "e".into(),
            engine_version: String::new(),
            tier: "tier4".into(),
            silent_fallback: false,
            stop_kind: "clean_exit".into(),
            exit_code: 0,
            pc: None,
            opcode: None,
            fault_addr: None,
            svc_id: None,
            break_result: None,
            insns_executed: 1,
            frames_presented: frames,
            framebuffer_hash: hash.map(str::to_string),
            message: None,
        };
        let json = format_repro_bundle_with_opts(
            "p.nro",
            1,
            &[],
            Some(crate::CpuTier::Tier4),
            Some("sdmc_dir"),
            Some(&mk(0, None)),
            Some(&mk(0, None)),
            Some(0),
            Some(0),
        );
        let m = ReproManifest::parse(&json).unwrap();
        assert_eq!(m.cpu, Some(crate::CpuTier::Tier4));
        assert_eq!(m.sdmc.as_deref(), Some("sdmc_dir"));

        let json_none = format_repro_bundle_with_opts(
            "p.nro",
            1,
            &[],
            None,
            None,
            Some(&mk(0, None)),
            Some(&mk(0, None)),
            Some(0),
            Some(0),
        );
        let m_none = ReproManifest::parse(&json_none).unwrap();
        assert_eq!(m_none.cpu, None);
        assert_eq!(m_none.sdmc, None);
    }

    #[test]
    fn rejects_wrong_schema() {
        assert!(ReportV1::parse(r#"{"schema": "other.v0"}"#).is_none());
    }

    #[test]
    fn parses_negative_exit_code_in_report_and_repro() {
        for code in [-1, -42, -1_073_741_819, i32::MIN, i32::MAX, 0] {
            let report_json = format!(
                r#"{{"schema":"ounce.run.v1","engine":"ounce-core","stop_kind":"unrecognised","exit_code":{code},"insns_executed":12}}"#
            );
            let parsed = ReportV1::parse(&report_json).unwrap();
            assert_eq!(parsed.exit_code, code);
            assert_eq!(
                parsed.to_stop_reason("", ""),
                StopReason::Unrecognised {
                    exit_code: Some(code),
                    stdout_tail: String::new(),
                    stderr_tail: String::new(),
                }
            );

            let repro_json = format_repro_bundle(
                "app.nro",
                128,
                &[],
                Some(&parsed),
                Some(&parsed),
                Some(-7),
                Some(code),
            );
            let manifest = ReproManifest::parse(&repro_json).unwrap();
            assert_eq!(manifest.expected_exit_code, Some(code));
        }

        // Out-of-i32-range exit_code falls back cleanly rather than wrapping.
        let overflow = r#"{"schema":"ounce.run.v1","engine":"e","stop_kind":"clean_exit","exit_code":99999999999999}"#;
        assert_eq!(ReportV1::parse(overflow).unwrap().exit_code, 1);
    }

    #[test]
    fn ignores_colliding_keys_inside_nested_objects_and_arrays() {
        let report_json = r#"{
  "meta": {
    "schema": "other.v0",
    "engine": "wrong-engine",
    "stop_kind": "cpu_error",
    "exit_code": 99,
    "pc": 1234,
    "silent_fallback": true,
    "framebuffer_hash": "fnv1a64:deadbeefdeadbeef",
    "nested_arr": [{"frames_presented": 999, "message": "wrong"}]
  },
  "metrics": [1, 2.5, 1e-4, {"exit_code": -999}],
  "schema": "ounce.run.v1",
  "engine": "ounce-core",
  "engine_version": "0.10.0",
  "tier": "tier4",
  "silent_fallback": false,
  "stop_kind": "clean_exit",
  "exit_code": -3,
  "pc": null,
  "insns_executed": 42,
  "frames_presented": 5,
  "framebuffer_hash": "fnv1a64:0123456789abcdef",
  "message": null
}"#;
        let parsed = ReportV1::parse(report_json).unwrap();
        assert_eq!(parsed.engine, "ounce-core");
        assert_eq!(parsed.stop_kind, "clean_exit");
        assert_eq!(parsed.exit_code, -3);
        assert_eq!(parsed.pc, None);
        assert!(!parsed.silent_fallback);
        assert_eq!(parsed.frames_presented, 5);
        assert_eq!(
            parsed.framebuffer_hash.as_deref(),
            Some("fnv1a64:0123456789abcdef")
        );
        assert_eq!(parsed.message, None);

        let repro_json = r#"{
  "runtime": {
    "stop_kind": "cpu_error",
    "exit_code": 12,
    "frames_presented": 99,
    "framebuffer_hash": "fnv1a64:aaaaaaaaaaaaaaaa"
  },
  "extra": {
    "core": { "stop_kind": "wrong", "exit_code": 77 }
  },
  "schema": "ounce.repro.v1",
  "program_path": "app.nro",
  "guest_args": ["a\"b", "c\\d", "\u0041\n"],
  "cpu": "tier3",
  "sdmc": null,
  "core": {
    "meta": { "exit_code": 88, "framebuffer_hash": "fnv1a64:bad" },
    "stop_kind": "clean_exit",
    "exit_code": -5,
    "frames_presented": 7,
    "framebuffer_hash": "fnv1a64:bbbbbbbbbbbbbbbb"
  }
}"#;
        let manifest = ReproManifest::parse(repro_json).unwrap();
        assert_eq!(manifest.program_path, "app.nro");
        assert_eq!(
            manifest.guest_args,
            vec!["a\"b".to_string(), "c\\d".to_string(), "A\n".to_string()]
        );
        assert_eq!(manifest.cpu, Some(crate::CpuTier::Tier3));
        assert_eq!(manifest.sdmc, None);
        assert_eq!(manifest.expected_stop_kind.as_deref(), Some("clean_exit"));
        assert_eq!(manifest.expected_exit_code, Some(-5));
        assert_eq!(manifest.expected_frames_presented, Some(7));
        assert_eq!(
            manifest.expected_framebuffer_hash.as_deref(),
            Some("fnv1a64:bbbbbbbbbbbbbbbb")
        );
    }

    #[test]
    fn string_values_with_escaped_key_names_and_unicode_do_not_spoof_fields() {
        let msg = r#"{"schema":"evil","stop_kind":"cpu_error","exit_code":99,"framebuffer_hash":"fnv1a64:evil"}"#;
        let escaped_msg = escape_json(msg);
        let json = format!(
            r#"{{"schema":"ounce.run.v1","engine":"ounce-core","message":"{escaped_msg}","stop_kind":"clean_exit","exit_code":0,"framebuffer_hash":null}}"#
        );
        let parsed = ReportV1::parse(&json).unwrap();
        assert_eq!(parsed.stop_kind, "clean_exit");
        assert_eq!(parsed.exit_code, 0);
        assert_eq!(parsed.framebuffer_hash, None);
        assert_eq!(parsed.message.as_deref(), Some(msg));

        // Check standard JSON escapes including `\/`, `\b`, `\f`, `\uXXXX`, and UTF-16 surrogates.
        let escaped_json = r#"{"schema":"ounce.run.v1","engine":"e\/1","stop_kind":"clean_exit","message":"\b\f\n\r\t\"\\\/\u0041\uD83E\uDD80"}"#;
        let parsed_esc = ReportV1::parse(escaped_json).unwrap();
        assert_eq!(parsed_esc.engine, "e/1");
        assert_eq!(
            parsed_esc.message.as_deref(),
            Some("\x08\x0C\n\r\t\"\\/A🦀")
        );
    }

    #[test]
    fn rejects_invalid_json_and_excessive_nesting_without_panicking() {
        let invalid = [
            "",
            "null",
            "[]",
            r#"{"schema":"ounce.run.v1","engine":"e","stop_kind":"clean_exit",}"#,
            r#"{"schema":"ounce.run.v1","engine":"e","stop_kind":"clean_exit"} trailing"#,
            r#"{"schema":"ounce.run.v1","engine":"e","stop_kind":"clean_exit","exit_code":01}"#,
            "{\"schema\":\"ounce.run.v1\",\"engine\":\"e\",\"stop_kind\":\"clean_exit\",\"message\":\"raw\nnewline\"}",
            r#"{"schema":"ounce.run.v1","engine":"e","stop_kind":"clean_exit","message":"\uD800"}"#,
            r#"{"schema":"ounce.run.v1","engine":"e","stop_kind":"clean_exit","message":"\uDC00"}"#,
            r#"{"schema":"ounce.run.v1","engine":"e","stop_kind":"clean_exit","message":"\x"}"#,
        ];
        for case in invalid {
            assert!(
                ReportV1::parse(case).is_none(),
                "should reject invalid JSON: {case:?}"
            );
        }

        let deep_open = "[".repeat(MAX_JSON_DEPTH + 10);
        let deep_close = "]".repeat(MAX_JSON_DEPTH + 10);
        let deep = format!(
            r#"{{"schema":"ounce.run.v1","engine":"e","stop_kind":"clean_exit","deep":{deep_open}1{deep_close}}}"#
        );
        assert!(parse_json(&deep).is_none());
        assert!(ReportV1::parse(&deep).is_none());
    }

    #[test]
    fn property_and_fuzz_tests_round_trip_and_never_panic() {
        struct Rng(u64);
        impl Rng {
            fn next(&mut self) -> u64 {
                let mut x = self.0;
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                self.0 = x;
                x
            }
            fn gen_str(&mut self, max_len: usize) -> String {
                const POOL: &[&str] = &[
                    "a",
                    "\"",
                    "\\",
                    "/",
                    "\n",
                    "\r",
                    "\t",
                    "\x08",
                    "\x0c",
                    "\"exit_code\": 99",
                    "{\"stop_kind\":\"bad\"}",
                    "🦀",
                    "é",
                    " ",
                    "0",
                ];
                let len = (self.next() as usize) % (max_len + 1);
                let mut s = String::new();
                for _ in 0..len {
                    let idx = (self.next() as usize) % POOL.len();
                    s.push_str(POOL[idx]);
                }
                s
            }
        }

        let mut rng = Rng(0x5EED_CAFE_BABE_1234);
        for _ in 0..400 {
            let engine = rng.gen_str(8);
            let engine_version = rng.gen_str(8);
            let stop_kind = "clean_exit";
            let exit_code = (rng.next() as i32).wrapping_sub(1_000_000);
            let insns = rng.next();
            let frames = rng.next() % 1000;
            let silent = rng.next().is_multiple_of(2);
            let has_hash = rng.next().is_multiple_of(2);
            let has_msg = rng.next().is_multiple_of(2);
            let msg = rng.gen_str(16);
            let hash_str = format!("fnv1a64:{:016x}", rng.next());

            let distractor = format!(
                r#"{{"exit_code":999,"stop_kind":"bad","framebuffer_hash":"fnv1a64:0000","msg":"{}"}}"#,
                escape_json(&rng.gen_str(10))
            );
            let json = format!(
                r#"{{"distractor":{distractor},"schema":"ounce.run.v1","engine":"{}","engine_version":"{}","tier":"tier4","silent_fallback":{},"stop_kind":"{}","exit_code":{},"insns_executed":{},"frames_presented":{},"framebuffer_hash":{},"message":{}}}"#,
                escape_json(&engine),
                escape_json(&engine_version),
                silent,
                stop_kind,
                exit_code,
                insns,
                frames,
                if has_hash {
                    format!("\"{}\"", escape_json(&hash_str))
                } else {
                    "null".to_string()
                },
                if has_msg {
                    format!("\"{}\"", escape_json(&msg))
                } else {
                    "null".to_string()
                }
            );
            let parsed = ReportV1::parse(&json).expect("generated report must parse");
            assert_eq!(parsed.engine, engine);
            assert_eq!(parsed.engine_version, engine_version);
            assert_eq!(parsed.silent_fallback, silent);
            assert_eq!(parsed.exit_code, exit_code);
            assert_eq!(parsed.insns_executed, insns);
            assert_eq!(parsed.frames_presented, frames);
            assert_eq!(
                parsed.framebuffer_hash.as_deref(),
                has_hash.then_some(hash_str.as_str())
            );
            assert_eq!(parsed.message.as_deref(), has_msg.then_some(msg.as_str()));

            let arg1 = rng.gen_str(8);
            let arg2 = rng.gen_str(8);
            let prog = format!("dir/{}.nro", rng.gen_str(6));
            let repro = format_repro_bundle_with_opts(
                &prog,
                128,
                &[arg1.clone(), arg2.clone()],
                Some(crate::CpuTier::Tier4),
                Some(&msg),
                Some(&parsed),
                Some(&parsed),
                Some(exit_code),
                Some(exit_code),
            );
            let manifest = ReproManifest::parse(&repro).expect("generated repro must parse");
            assert_eq!(manifest.program_path, prog);
            assert_eq!(manifest.guest_args, vec![arg1, arg2]);
            assert_eq!(manifest.sdmc.as_deref(), Some(msg.as_str()));
            assert_eq!(manifest.expected_exit_code, Some(exit_code));
            assert_eq!(manifest.expected_frames_presented, Some(frames));
            assert_eq!(
                manifest.expected_framebuffer_hash.as_deref(),
                has_hash.then_some(hash_str.as_str())
            );
        }

        // Mutation fuzzing: arbitrary token soup must never panic.
        const FUZZ_TOKENS: &[&str] = &[
            "{",
            "}",
            "[",
            "]",
            ":",
            ",",
            "\"",
            "\\",
            "null",
            "true",
            "false",
            "-0",
            "1e999999",
            "01",
            "\\u",
            "\\uD800",
            "\\uDC00",
            "\n",
            "\t",
            "🦀",
            "schema",
            "core",
            "exit_code",
        ];
        for _ in 0..2000 {
            let parts = (rng.next() as usize) % 24;
            let mut doc = String::new();
            for _ in 0..parts {
                doc.push_str(FUZZ_TOKENS[(rng.next() as usize) % FUZZ_TOKENS.len()]);
            }
            let _ = parse_json(&doc);
            let _ = ReportV1::parse(&doc);
            let _ = ReproManifest::parse(&doc);
        }
    }
}
