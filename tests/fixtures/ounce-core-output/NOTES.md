# Captured ounce-core output

These files are real stdout, stderr and exit codes from the ounce-core CLI, used as fixtures for
`src/diagnose.rs` (tests in `tests/diagnose.rs`). Each case is `<case>.stdout`, `<case>.stderr`
and `<case>.exit`. Timing lines (`in 41.2 ms`, `MIPS`) vary between runs and the parser ignores them.

## How they were produced

- Binary: `ounce-core` v0.8.0 (`--version` banner says v0.8.0), built from the ounce-core checkout at
  commit `cf199bc` (merge of release 0.8.0). (ounce-hb no longer depends on the ounce-core crate.) The binary was already built in that checkout and was not rebuilt
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

## Host-audio noise on machines with no sound device

ounce-core v0.8.0 opens the default host audio device for every run (`HostAudioSink::new` in
`src/audio/host.rs`, called from `src/main.rs`), and has no flag or variable to skip it. On CI runners
with no sound device, the lines below appear on stderr of an otherwise clean run. They do not change the
exit status and are not ounce-hb diagnoses.

- `clean_audio_noise_linux.stderr`: the ALSA lines from the `clean` test on `ubuntu-latest`, copied from
  CI run 43376441897, job `e2e (linux-x86_64)`, with the log timestamps removed. ALSA prints them from the
  C library, not from ounce-core's Rust code.
- `clean_audio_noise_macos.stderr`: the `[HostAudioSink]` line from the same run's `e2e (macos-arm64)`
  job, with the log timestamp removed. It is a runtime stream error from the audio sink, not a
  startup failure.

`tests/ounce_core_e2e.rs` tolerates only lines beginning `ALSA lib ` or `[HostAudioSink] `.

## Killed by a signal (exit code 1 in ounce-hb)

- `killed_by_signal.stdout`: the stdout of a real ounce-core v0.8.0 run (one line, the staged program path after "Reading package/executable:", was replaced by `/tmp/ounce-hb-run-REDACTED/program.nro` because the original was a private local path; the parser ignores that line) of a spinning NRO (`b .`, with
  `OUNCE_NRO_BUDGET=100000000000`) that was sent SIGKILL from outside while running. ounce-core printed no
  stop line and no stderr, so the run has no recognised stop and ounce-hb reports it as unrecognised with
  exit 1. The `.stderr` file is empty for that reason. There is no `.exit` file: the process had no exit
  status, only a signal.
- Reproduced by `signal_killed_ounce_core_is_unrecognised_exit_1` in `tests/ounce_core_e2e.rs`.
