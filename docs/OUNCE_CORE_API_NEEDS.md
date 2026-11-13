# ounce-core API needs

## Status

`ounce-hb run` executes NROs through a **subprocess** backend (`src/backend.rs`). It runs the
ounce-core CLI as:

```
ounce-core run-nro <staged copy of the .nro> --present=headless [guest args]
```

- **Binary lookup:** the `OUNCE_CORE_BIN` environment variable first (it must name a file), then the
  first `ounce-core` on `PATH`. If neither works, `ounce-hb` exits with code 3 and names the variable.
- **Input:** the `.nro` is copied to a private temporary file after it passes `classify`, so the
  program that runs is the validated one. The copy is removed after the run.
- **Output:** the exit status, stdout and stderr are captured and returned as a typed result.
  `ounce-hb` prints the captured stdout and stderr, then exits with its own documented exit code for how the run stopped ([DIAGNOSIS.md](DIAGNOSIS.md)), not ounce-core's.
- **Time limit:** 60 seconds by default (`OUNCE_HB_TIMEOUT_SECS` changes it). The process is killed when the limit
  passes, and the run fails with a timeout error (exit 4).
- **Guest arguments:** an argument starting with `-` is refused. ounce-core's `run-nro` silently
  drops those, so passing them through would hide the change.

### What the exit code means

The ounce-hb exit code says how the run stopped; it is not ounce-core's process status and not the guest's return
value. The full list is in [DIAGNOSIS.md](DIAGNOSIS.md). In short:

- `0`: the program exited through the loader's exit path (`svcExitProcess`). In a libnx program this is so
  whatever `main` returns: the `hello` fixture returned `0xBAD0008` and still exited 0.
- `10` to `17`: ounce-core stopped early in a named way (unimplemented instruction, memory fault, svcBreak, unhandled
  SVC, CPU error, budget exhausted, loader error).
- `1`: ounce-core stopped in a way ounce-hb does not recognise; the raw output is printed unchanged.

A guest that wants to report a result must print it. For example, the `hello` fixture in ounce-core
writes its result with `svcOutputDebugString`.

Checked against ounce-core **v0.8.0** (tag `v0.8.0`, commit `cf199bc`). The synthetic-NRO test in
`tests/ounce_core_run.rs` runs whenever an ounce-core binary is available and skips with a message
otherwise.

### Known gaps

- The CPU engine (`--cpu`), `--sdmc` and `--gui` are not exposed. The subprocess runs ounce-core's
  defaults, with the display set to headless.
- `Cargo.toml` has no `ounce-core` dependency: the subprocess backend runs the released binary and
  links nothing. A library backend would add one, pinned to a release tag.

## Parked: library backend

A library backend would call ounce-core in-process instead of spawning it. It is not planned now.
The `Backend` trait in `src/backend.rs` is the seam it would plug into, so nothing else in
`ounce-hb` has to change. Its prerequisite is still missing: as of v0.8.0, ounce-core has **no
`[lib]` target**, so `cargo` ignores the dependency and no library item can be imported.

For reference, the items a library backend would need are:

| Need | Where it lives in ounce-core |
|---|---|
| Parse and map an NRO, apply relocations | `loader::NroLoader` (`parse_header`, `load_into_memory`) |
| Guest memory | `mmu::Memory`, `mmu::Permission` |
| Homebrew loader ABI (`ConfigEntry` list in X0, exit stub) | `loader::homebrew::HomebrewConfig` |
| CPU engine | `cpu::Cpu`, `cpu::StepResult` |
| SVC layer and HLE services | `kernel::HorizonKernel`, `kernel::SvcResult` |

Beyond visibility, the run loop lives in ounce-core's `main.rs` (`run_file`, `run_native_nro`,
`run_vm_internal`). A library entry point would return the exit code and captured output without
exiting the process or opening a window, and would have a headless mode that skips `minifb` and
`cpal` setup.

## Out of scope on purpose

ounce-core's key parser (`src/crypto/`) and NCA/XCI/PFS0 containers (`src/loader/nca.rs`) are not
used by ounce-hb. The owner has decided to remove them from ounce-core.
