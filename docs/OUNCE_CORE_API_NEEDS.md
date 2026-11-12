# ounce-core API needs

`ounce-hb run` validates the input and then needs ounce-core to execute it. ounce-core is
pinned in `Cargo.toml` (commit `f80d2e4`), but as of that commit it has **no `[lib]` target**:
`src/main.rs` declares every module (`mod loader; mod kernel; mod cpu; mod mmu; ...`) and
Cargo reports `ignoring invalid dependency ounce-core which is missing a lib target`.
ounce-core is not modified by this repo; these are the requests for the ounce-core owner.

## Blocking: a library target
- Add `src/lib.rs` exposing the modules below (the binary can then `use ounce_core::...`).
  Until then `ounce_hb::run` returns `RunError::CoreApiMissing` for every NRO.

## Needed public items (all exist today, but are crate-private)
| Need | Where it lives in ounce-core |
|---|---|
| Parse and map an NRO, apply relocations | `loader::NroLoader` (`parse_header`, `load_into_memory`) |
| Guest memory | `mmu::Memory`, `mmu::Permission` |
| Homebrew loader ABI (`ConfigEntry` list in X0, exit stub) | `loader::homebrew::HomebrewConfig` |
| CPU engine | `cpu::Cpu`, `cpu::StepResult` |
| SVC layer and HLE services | `kernel::HorizonKernel`, `kernel::SvcResult` |

## Needed beyond visibility
- The run loop that glues these together lives inside `main.rs` (`run_file`,
  `run_native_nro`, `run_vm_internal`). It should move into a library function such as
  `ounce_core::run_nro(bytes, &RunConfig) -> RunOutcome` returning the exit code and any
  captured console/log output, with no process exit, no GUI window and no host audio required.
- A headless mode (no `minifb`/`cpal` initialisation) so CI and `ounce-hb` can run NROs.
- The loader error type (`loader::LoadError`) public and stable.

## Out of scope on purpose
ounce-core's key parser (`src/crypto/`) and NCA/XCI/PFS0 containers (`src/loader/nca.rs`)
are not used by ounce-hb. The owner has decided to remove them from ounce-core.
