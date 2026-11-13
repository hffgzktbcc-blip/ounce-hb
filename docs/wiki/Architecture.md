# Architecture

This document describes the architectural design of `ounce-hb`, its separation of concerns, and
how it orchestrates execution with `ounce-core`.

---

## High-Level Architecture

```
+-------------------------------------------------------------+
|                          ounce-hb                           |
|                                                             |
|  +--------------------+     +----------------------------+  |
|  |   CLI Interface    | --> |    Format Classifier       |  |
|  |  (run, args, env)  |     |   (src/lib.rs: classify)   |  |
|  +--------------------+     +----------------------------+  |
|                                          |                  |
|                                          v                  |
|                             +----------------------------+  |
|                             |      Backend Trait         |  |
|                             |     (src/backend.rs)       |  |
|                             +----------------------------+  |
+------------------------------------------|------------------+
                                           |
                                           v
                       +---------------------------------------+
                       |       Subprocess Runner Backend       |
                       |  - Discovers ounce-core binary        |
                       |  - Stages file if needed              |
                       |  - Invokes: ounce-core run-nro        |
                       |    --present=headless [args...]       |
                       |  - 60s timeout & thread pipe readers  |
                       +---------------------------------------+
                                           |
                                           v
+-------------------------------------------------------------+
|                         ounce-core                          |
|  +-------------------+  +----------------+  +------------+  |
|  |   AArch64 CPU     |  | Guest Memory   |  | Horizon OS |  |
|  | (Tier3/4/Native)  |  |     (MMU)      |  |  Services  |  |
|  +-------------------+  +----------------+  +------------+  |
+-------------------------------------------------------------+
```

---

## Key Subsystems

### 1. Format Classifier (`src/lib.rs`)
The classifier examines magic bytes without executing or decrypting arbitrary content:
- **NRO (`Kind::Nro`)**: Matches `NRO0` magic at byte offset `0x10`.
- **NSO (`Kind::Nso`)**: Matches `NSO0` magic at byte offset `0x00`.
- **Retail & Out-of-Scope Formats**: Explicitly identifies and rejects `PFS0` (NSP packages), `HEAD` (XCI game cards), `NCA0`/`2`/`3` (content archives), and `META` (NPDM metadata).

### 2. Backend Abstraction (`src/backend.rs`)
Defines the `Backend` trait:
```rust
pub trait Backend {
    fn run_nro(&self, nro: &Path, args: &[String]) -> Result<RunOutput, RunError>;
}
```
This decouples the CLI frontend from the execution mechanism:
- **Subprocess Runner**: Used today in M0. Locates `ounce-core` via `OUNCE_CORE_BIN` or `PATH`, runs it with `--present=headless`, drains stdout/stderr concurrently via dedicated threads, and enforces a 60-second deadline.
- **Future Library Backend**: Will link directly to `ounce-core` once upstream exposes a library crate (see [OUNCE_CORE_API_NEEDS.md](../OUNCE_CORE_API_NEEDS.md)).

### 3. Safety & Sandboxing
- Main crate enforces `#![forbid(unsafe_code)]`.
- Subprocess execution isolates host memory from the guest environment.
- Headless mode (`--present=headless`) asks ounce-core not to open a window. Whether it also skips the host audio
  device depends on the ounce-core version: v0.8.1 and newer open none, v0.8.0 still tries and may print device
  errors on a machine with no audio output ([LIMITS.md](../LIMITS.md)).

---

## Related Documentation
- [Getting Started](Getting-started.md)
- [Roadmap](Roadmap.md)
- [Limits & Capabilities](../LIMITS.md)
