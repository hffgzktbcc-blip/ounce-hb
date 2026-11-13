# Stop diagnosis in `ounce-hb run`

When a program does not exit cleanly, `ounce-hb run` prints the guest output unchanged, then a short
explanation in plain words. The explanation names the category, gives the reason, and ends with the
command that reproduces the run. The process exits with a code that identifies the category.

A clean guest exit prints nothing extra and exits `0`.

Example, from a real ounce-core v0.8.0 run of a program whose first word is undefined:

```
ounce-hb: Ounce Core does not implement an instruction (exit code 10).
  The program reached instruction 0x00000000 at address 0x0000000080000080. That is a gap in Ounce Core, not necessarily a bug in the program.
  To reproduce: ounce-hb run unimpl.nro
```

## Exit codes

These codes are stable. Scripts may rely on them. A new category takes a new code and does not
reuse one.

| Code | Category | Meaning |
|---|---|---|
| `0` | clean exit | The guest reached svcExitProcess and ounce-core exited `0`. |
| `1` | unrecognised | ounce-core stopped in a way this file does not list. Its output is printed unchanged and nothing is inferred from it. Also used when ounce-core was killed by a signal with no recognised line. |
| `2` | refused | The request was refused before ounce-core ran: a guest argument starting with `-`, guest arguments longer than 16 KiB together (see [Guest-argument limit](#guest-argument-limit)), an unsupported NSO kind, a malformed `OUNCE_HB_TIMEOUT_SECS`, or an input that is not an unencrypted NRO (also `2` for the input checks that run before this step). |
| `3` | core missing | No ounce-core binary: `OUNCE_CORE_BIN` is unset or wrong, and no `ounce-core` is on `PATH`. |
| `4` | timeout | ounce-core ran past the wall-clock limit (60 seconds, or `OUNCE_HB_TIMEOUT_SECS`) and was killed. |
| `5` | host I/O | ounce-hb could not write its private copy of the program, or could not start or wait on ounce-core. |
| `10` | unimplemented instruction | The CPU met a word ounce-core does not implement. The message gives the word and the address. |
| `11` | memory fault | ounce-core rejected a memory access. The message keeps ounce-core's wording and, when present, the address. |
| `12` | CPU error | Any other `[VM Error]` line, such as a breakpoint or a branch-target exception. |
| `13` | svcBreak | The guest called svcBreak. The message gives the break PC and result when ounce-core printed them. |
| `14` | unhandled SVC | The guest called a system service ounce-core does not provide. |
| `15` | CPU halted | ounce-core printed `CPU Halted`. No homebrew NRO reaches this line in v0.8.0; see [Evidence status](#evidence-status). |
| `16` | budget exhausted | The instruction budget ran out before the guest exited. The message suggests `OUNCE_NRO_BUDGET`, which ounce-hb passes through to ounce-core. |
| `17` | loader error | ounce-core could not load the file. |

Codes `2` and `3` are also used for input problems ounce-hb finds before running anything (unreadable
file, out-of-scope container, missing core).

## What is matched

ounce-core v0.8.0 prints each stop as plain text. The parser in `src/diagnose.rs` matches these lines
exactly, on stdout and stderr, and only when the exit status is non-zero (the clean line needs exit `0`):

| Category | Line prefix that is matched | Fields read |
|---|---|---|
| clean exit | `[VM] Process exited gracefully (svcExitProcess).` on stdout, with exit `0` | none |
| unimplemented instruction | `[VM Error] Unimplemented instruction 0x<word> at PC: 0x<pc>` | word, PC |
| memory fault | `[VM Error] Memory access violation: <message>` | message, address after `virtual address: 0x` or `at 0x` |
| CPU error | any other `[VM Error] <message>` | message |
| svcBreak | `[VM] Hit svcBreak:`, then `[VM] Break PC: 0x<pc>` and `[VM] Break result: 0x<word>` if present | PC, result |
| unhandled SVC | `[VM] Stopping on unhandled SVC #0x<id>.` | id |
| CPU halted | `[VM] CPU Halted: <message>` | message |
| budget exhausted | `[VM] Execution budget reached (<n> instructions).` | n |
| loader error | `[Loader Error] <message>` | message |

A line whose shape matches but whose numbers do not parse is not recognised. A run with no recognised
line, or a clean line with a non-zero exit, is reported as unrecognised with the exact stdout, stderr and
exit status kept.

ounce-core's `run-nro` does not report a guest exit status. `svcExitProcess` carries none, and the
`Final X0` value in its summary is a register, not a documented exit code. `ounce-hb run` therefore exits
`0` for every clean exit and does not report the register value.

## Limits

- Matching is by the exact lines above. A guest that prints text beginning with one of those prefixes
  could be mistaken for ounce-core output. ounce-core's output mixes guest and runner text, so this
  cannot be fully ruled out from the output alone.
- Another ounce-core version may word these lines differently. Such runs are reported as unrecognised
  with the raw output, not explained.
- The timeout category (`4`) is decided by ounce-hb's own wall-clock limit, not by ounce-core. A
  synthetic infinite loop run with `OUNCE_NRO_BUDGET` raised reaches the limit and exits `4`, and the
  ounce-core child is killed and reaped before ounce-hb returns. Without the raised budget the default
  50 million instructions run out within about a second, so that loop reports `16`, not `4`.

## Guest-argument limit

The guest arguments together may be at most 16384 bytes. The total counts each argument's bytes plus one
NUL terminator, as argv strings are laid out in memory. A longer set is refused with exit code `2` before
ounce-core starts, and the message names the limit. The constant is `MAX_GUEST_ARGS_BYTES` in `src/lib.rs`.

The limit is a workaround for [ounce-core #418](https://github.com/hffgzktbcc-blip/ounce-core/issues/418):
in v0.8.0, once the argument strings pass about 62,839 bytes, the config block at `0x7FFF0000` runs into the
program at `0x80000000` and overwrites its first instruction. The limit stays well below that. Remove it only
after #418 is fixed in a release ounce-hb pins.

Tests: `argument_just_under_the_limit_runs_for_real` (real ounce-core, exit `0`) and
`argument_over_the_limit_is_refused_before_ounce_core_starts` (exit `2`; a marker file shows that no
ounce-core process starts), in `tests/guest_args.rs`.

## Host audio noise on stderr

ounce-core v0.8.0 opens the default host audio device on every run, and it has no flag or variable to
skip that. On a machine with no sound device it writes errors to stderr during an otherwise clean run:
ALSA's `ALSA lib ...` lines on Linux, or `[HostAudioSink] Output stream error: ...` on macOS. Captured
text is in `tests/fixtures/ounce-core-output/clean_audio_noise_*.stderr`. The exit status is still `0`, so
`ounce-hb run` prints no diagnosis for these lines. ounce-hb passes them through unchanged. The end-to-end
tests tolerate only lines that begin `ALSA lib ` or `[HostAudioSink] `.

## Timeout

`ounce-hb run` kills ounce-core after 60 seconds by default. The limit can be changed with
`OUNCE_HB_TIMEOUT_SECS`, a number of seconds greater than 0 and at most 86400 (one day). Surrounding
spaces are allowed. Any other value, including `nan`, `inf`, `0`, a negative number or text, is refused
with exit code `2` before ounce-core is looked for. The variable is not silently replaced by the default.
The limit applies to ounce-core's whole run, including startup.

The end-to-end test `wall_clock_timeout_exits_4_and_kills_ounce_core` (`tests/ounce_core_e2e.rs`) runs a
spinning NRO with `OUNCE_HB_TIMEOUT_SECS=2` and `OUNCE_NRO_BUDGET` set to 10^11, so only the wall-clock
limit can stop it. It asserts exit `4` and then checks that no process still has the run's private
directory in its command line.

## Evidence status

Each exit code, and whether a real run backs it. "Real run" means `ounce-hb run` was executed against
the ounce-core v0.8.0 binary, or the captured ounce-core output from such a run is in the fixtures.

| Code | Category | Backed by a real run? | Where |
|---|---|---|---|
| `0` | clean exit | Yes | `clean_guest_exit_is_zero_with_no_explanation`; fixture `clean` |
| `1` | unrecognised | Yes, by a real run killed by a signal: ounce-core was sent SIGKILL from outside and printed no stop line (`signal_killed_ounce_core_is_unrecognised_exit_1`; fixture `killed_by_signal.stdout`). No *natural* ounce-core stop reaches this code: every exit path in the `run-nro` flow prints a listed marker. About 1,000 generated runs (random SVC sequences, random code words, random loader inputs) and a few argument-size probes produced no panic and no unlisted stop. |
| `2` | refused | Yes (real `ounce-hb` run, no ounce-core output involved) | `refused_guest_argument_exits_2_without_core`, `invalid_timeout_override_exits_2_without_core`, `argument_over_the_limit_is_refused_before_ounce_core_starts` |
| `3` | core missing | Yes (real `ounce-hb` run) | `tests/cli.rs` `valid_nro_without_core_binary_reports_it` |
| `4` | timeout | Yes | `wall_clock_timeout_exits_4_and_kills_ounce_core` |
| `5` | host I/O | Yes (spawn of a non-executable file fails) | `ounce_core_that_cannot_be_started_exits_5` |
| `10` | unimplemented instruction | Yes | `unimplemented_instruction_exits_10`; fixture `unimpl` |
| `11` | memory fault | Yes | `memory_fault_exits_11`; fixture `memfault` |
| `12` | CPU error | Yes (`brk #0` gives `BRK #0x0 breakpoint hit`) | `cpu_error_from_brk_exits_12` |
| `13` | svcBreak | Yes | `svc_break_exits_13`; fixture `svc_break` |
| `14` | unhandled SVC | Yes | `unhandled_svc_exits_14`; fixture `unhandled_svc` |
| `15` | CPU halted | **No. Not reachable from a homebrew NRO in v0.8.0.** See below. | source only |
| `16` | budget exhausted | Yes | `exhausted_budget_exits_16`; fixture `budget` |
| `17` | loader error | Yes | `loader_error_exits_17`; fixture `loader_error` |

### Why `15` cannot be captured

ounce-core v0.8.0 (checkout at commit `cf199bc`) defines the variant and prints the line, but nothing
constructs it:

- `src/cpu/interpreter.rs:44` declares `StepResult::Halt(String)` ("Process exited or CPU halted").
- The only mentions of `Halt(` in `src/` and `crates/` are consumers: `src/main.rs:831` (which prints
  `[VM] CPU Halted:` at line 832), `src/cpu/interpreter.rs:147`, `src/cpu/nce.rs:335`,
  `src/loader/corpus_runner.rs:359`, `src/loader/hbx_frame.rs:82`, `src/loader/init.rs:56`,
  `src/linux/runner.rs:138`, `src/kernel/svc.rs:1519`, `src/loader/fixture_tests.rs:141` and
  `crates/ounce-learn/src/lib.rs:58`.
- `Cpu::step` (`src/cpu/interpreter.rs:139`) only records the result and passes it on. `step_inner`
  (`src/cpu/interpreter.rs:153`) returns `Continue`, `Svc` or an error for each instruction. No
  instruction path returns `Halt`.
- The halt-like instruction `HLT` (`0xD4400000`) is not decoded. A real run gives
  `[VM Error] Unimplemented instruction 0xD4400000 at PC: ...` (exit `10`).
- `CpuError::Halted` (`src/cpu/interpreter.rs:28`) is also never constructed.

The `[VM] CPU Halted:` line is still recognised, because main.rs prints it. If a later ounce-core
constructs `Halt`, its runs will be explained as category `15`. Until a run shows that, `15` is parser
logic with no captured output behind it.

## Evidence

Fixtures are real ounce-core v0.8.0 output in `tests/fixtures/ounce-core-output/`: stdout, stderr and
the exit status for each case, with `NOTES.md` saying how each was made. `make_nros.py` in the same
directory writes the small NRO inputs. The NRO files are generated and not committed.

`tests/diagnose.rs` checks the classification of each fixture, the exit codes, the wording of each
explanation, and runs the real `ounce-hb` binary against a stand-in ounce-core that replays each
capture.

`tests/ounce_core_e2e.rs` runs the real `ounce-hb` binary against the real ounce-core CLI. It generates
each NRO inside the test and checks the exit code and category for each code above that a program can
reach. It skips with a SKIPPED line when no ounce-core binary is configured. CI
(`.github/workflows/ounce-core-e2e.yml`) downloads the v0.8.0 release asset for Linux x86_64 and macOS
arm64, checks its sha256 against the release's `SHA256SUMS`, and fails if any test was skipped.
