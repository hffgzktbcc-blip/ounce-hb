# Captured ounce-core output

These files are real stdout, stderr and exit codes from the ounce-core CLI, used as fixtures for
`src/diagnose.rs` (tests in `tests/diagnose.rs`). Each case is `<case>.stdout`, `<case>.stderr`
and `<case>.exit`. Timing lines (`in 41.2 ms`, `MIPS`) vary between runs and the parser ignores them.

## How they were produced

- Binary: `ounce-core` v0.8.0 (`--version` banner says v0.8.0), built from the ounce-core checkout at
  commit `cf199bc` (merge of release 0.8.0). The pinned dependency in `Cargo.lock` is `f80d2e4`, which
  is an ancestor of the v0.8.0 tag. The binary was already built in that checkout and was not rebuilt
  for these captures. Its path is `target/release/ounce-core` under the checkout.
- Command, the same one `ounce-hb run` issues, run from this directory after `make_nros.py` wrote
  the NROs:

  ```sh
  ounce-core run-nro <case>.nro --present=headless
  ```

  `exit` is the process exit status. `budget` was captured with `OUNCE_NRO_BUDGET=1000` set in the
  environment, because the default budget is 50 million instructions.
- NROs: `make_nros.py` writes them. They are generated test inputs, not committed binaries, so
  `docs/PROVENANCE.md` does not list them.

| Case | Program (from `make_nros.py`) | Exit | What ounce-core printed |
|---|---|---|---|
| `clean` | `mov w0,#0; svc #7` (svcExitProcess) | 0 | `[VM] Process exited gracefully (svcExitProcess).` on stdout; stderr empty |
| `unimpl` | one undefined word `0x00000000` | 1 | `[VM Error] Unimplemented instruction 0x00000000 at PC: 0x0000000080000080` on stderr, then `  Final PC:` |
| `memfault` | `ldr x0,[x1]` with x1 unmapped | 1 | `[VM Error] Memory access violation: Unmapped memory access at virtual address: 0xFFFFFFFFFFFFFFFF` |
| `budget` | `b .` (infinite loop), `OUNCE_NRO_BUDGET=1000` | 1 | `[VM] Execution budget reached (1000 instructions).` on stdout |
| `svc_break` | `add x1,sp,#0; mov x2,#4; svc #0x26` (svcBreak) | 1 | `[VM] Hit svcBreak: ...`, `[VM] Break PC: 0x...`, `[VM] Break result unreadable: ...` (the info pointer is not mapped, so the result is not read) |
| `unhandled_svc` | `svc #0x7E` | 1 | `[VM] Stopping on unhandled SVC #0x7E.` on stdout; `[Horizon SVC] Unhandled SVC #0x7E` on stderr |
| `loader_error` | text size set past the end of the file | 1 | `[Loader Error] Image text segment exceeds file bounds (...)` on stderr |
| `usage` | no NRO: `ounce-core run-nro --present=headless` | 2 | `error: 'run-nro' needs the path of a homebrew .nro file.` on stderr. Not something `ounce-hb run` can produce; kept as text ounce-core prints that the parser must not recognise |

The `hlt #0` probe gave `Unimplemented instruction 0xD4400000`, not a CPU-halted line, so no fixture
for `[VM] CPU Halted:` exists. The parser recognises that line from the ounce-core source
(`src/main.rs`), and `tests/diagnose.rs` says so.
