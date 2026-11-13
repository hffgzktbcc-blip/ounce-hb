# Stop diagnosis in `ounce-hb run`

When a program does not exit cleanly, `ounce-hb run` prints the guest output, then a short
explanation in plain words. The explanation names the category, gives the reason, and ends with the
command that reproduces the run. The process exits with a code that identifies the category.

A clean guest exit prints nothing extra and exits `0`.

The guest output is printed unchanged unless it was longer than ounce-hb keeps (8 MiB per stream, see
[Output limits](#output-limits)); then a marker line says where bytes were dropped and a note follows the
explanation. If stdout or stderr cannot be written, because whoever was reading it has gone away, the
write is dropped and the exit code is still the one below. `ounce-hb run` never exits `101` (a Rust
panic) for an output error.

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
| `2` | refused | The request was refused before ounce-core ran: a guest argument starting with `-`, guest arguments longer than 16 KiB together (see [Guest-argument limit](#guest-argument-limit)), an unsupported NSO kind, a malformed `OUNCE_HB_TIMEOUT_SECS`, a word between the program path and `--` (see [Command line](#command-line)), or an input that is not an unencrypted NRO (also `2` for the input checks that run before this step). |
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

## Command line

`ounce-hb run <file.nro>` and `ounce-hb run <file.nro> -- <guest args...>` are the only accepted forms.
A word between the program path and `--` (for example `ounce-hb run game.nro --timeout 5 -- a`) is
refused with exit code `2` and a message that names the word. It used to be dropped without a word,
which hid typos and options meant for ounce-hb. Guest arguments are everything after `--`, and each is
checked as before (no leading `-`, the length limit).

## What is matched

ounce-core v0.8.0 and v0.8.1 print each stop as plain text, with the same wording. The parser in
`src/diagnose.rs` matches these lines exactly and only when the exit status is non-zero (the clean line
needs exit `0`). The "stream" column says where ounce-core prints the line and where the parser reads it:

| Category | Line prefix that is matched | Stream | Fields read |
|---|---|---|---|
| clean exit | `[VM] Process exited gracefully (svcExitProcess).`, with exit `0` | stdout | none |
| unimplemented instruction | `[VM Error] Unimplemented instruction 0x<word> at PC: 0x<pc>` | stderr | word, PC |
| memory fault | `[VM Error] Memory access violation: <message>` | stderr | message, address after `virtual address: 0x` or `at 0x` |
| CPU error | any other `[VM Error] <message>` | stderr | message |
| svcBreak | `[VM] Hit svcBreak:`, then `[VM] Break PC: 0x<pc>` and `[VM] Break result: 0x<word>` if they follow it | stdout | PC, result |
| unhandled SVC | `[VM] Stopping on unhandled SVC #0x<id>.` | stdout | id |
| CPU halted | `[VM] CPU Halted: <message>` | either (no capture exists; see [Evidence status](#evidence-status)) | message |
| budget exhausted | `[VM] Execution budget reached (<n> instructions).` | stdout | n |
| loader error | `[Loader Error] <message>` | stderr | message |

The stop line is **the last recognised line on stderr**, if there is one, **else the last recognised line
on stdout**. `[VM Error]` and `[Loader Error]` lines are taken from stderr only; the same text on stdout
is treated as guest text. The next section says why.

A line whose shape matches but whose numbers do not parse is not recognised. A run with no recognised
line, or a clean line with a non-zero exit, is reported as unrecognised. The unrecognised reason keeps
the exit status and the last 1 KiB of each stream for the record (the output itself is printed in full
before the explanation, subject to the [output limits](#output-limits)).

## Guest text and the stop line

A program can print anything with `svcOutputDebugString`. ounce-core v0.8.1 writes that text to stdout
as it is, with no prefix, so a program can print a line that looks exactly like a stop line. The
adversarial review showed three outcomes on the earlier parser, which took the first recognised line
across both streams: a program that printed a budget line and then called an unhandled SVC was reported
as `16` instead of `14`; one that printed the clean line and then faulted was reported as `1` instead of
`11`; one that printed a `[VM Error]` line and then exited cleanly was reported as `1` instead of `0`.

Two facts about ounce-core's output, checked against the real v0.8.1 binary (fixtures `spoof_*` in
`tests/fixtures/ounce-core-output/`, programs in `make_nros.py`), make the classification hold:

1. **Guest text never reaches stderr.** `svcOutputDebugString` is the only way a homebrew program
   writes text, and it goes to stdout. So a stop line on stderr is ounce-core's own. ounce-core prints
   `[VM Error]` and `[Loader Error]` lines there, and only there.
2. **ounce-core prints its stdout stop line after the VM has stopped.** Nothing the guest printed can
   follow it, so the last stop line on stdout is ounce-core's. The `[VM] Break PC` and `[VM] Break
   result` lines are read only after that line, so planted ones are not used.

The four spoof programs now give `14`, `11`, `0` and `13` with the real break PC, both from the
captured fixtures (`tests/diagnose.rs`) and from runs against the real binary (`tests/ounce_core_e2e.rs`,
the `guest_text_cannot_*` tests).

What remains, stated plainly:

- The two facts are properties of ounce-core v0.8.1's `run-nro`, observed, not promised by an API. If a
  later ounce-core gives guests a way to write stderr, or prints guest text after its stop line, the
  classification is open to forgery again. The fixtures pin the current behaviour.
- A guest cannot change ounce-core's exit status except by stopping in a real way, so the clean-exit
  category (exit `0`) cannot be forged or hidden.
- Guest text can still make the *printed output* confusing: the forged lines are printed with the rest.
  The explanation and the exit code are what to trust.
- Under the [output limits](#output-limits), a guest that prints more than 4 MiB after ounce-core's
  real stop line cannot exist (the VM has stopped), so the kept tail always holds the real stop line.
  A guest that prints more than 8 MiB before stopping loses the middle of its output, not the stop line.

ounce-core's `run-nro` does not report a guest exit status. `svcExitProcess` carries none, and the
`Final X0` value in its summary is a register, not a documented exit code. `ounce-hb run` therefore exits
`0` for every clean exit and does not report the register value.

## Limits

- Matching is by the exact lines above, under the two facts in [Guest text and the stop line](#guest-text-and-the-stop-line).
- Another ounce-core version may word these lines differently. Such runs are reported as unrecognised
  with the raw output, not explained.
- The timeout category (`4`) is decided by ounce-hb's own wall-clock limit, not by ounce-core. A
  synthetic infinite loop run with `OUNCE_NRO_BUDGET` raised reaches the limit and exits `4`, and the
  ounce-core process group is killed and reaped before ounce-hb returns. Without the raised budget the
  default 50 million instructions run out within about a second, so that loop reports `16`, not `4`.

## Output limits

ounce-hb keeps at most 8 MiB of ounce-core's stdout and 8 MiB of its stderr (`MAX_CAPTURED_BYTES` in
`src/backend.rs`). Of a longer stream it keeps the first 4 MiB and the last 4 MiB and drops the bytes
between them. Both pipes are read to the end regardless, so ounce-core never blocks on a full pipe;
the dropped bytes are counted, not stored. Before this limit a program that printed without pause for
its whole run could take host memory without bound (the review measured 604 MB resident for 300 MB
of output; the same run now stays near 31 MB).

Where bytes were dropped, the printed output carries one marker line in their place:

```
[ounce-hb: 1048576 bytes of ounce-core's stdout were dropped here to stay within 8388608 bytes; the first 4194304 and the last 4194304 bytes are kept]
```

and after the explanation ounce-hb adds a note on stderr saying how many bytes of each stream were
dropped and that the diagnosis used the kept parts only. The exit code is not changed by truncation. The
tail is kept because ounce-core's stop line comes last, so the category still comes from the real line.

## Process control

- **Process group.** On unix ounce-core runs in its own process group. The timeout, and every other
  kill ounce-hb performs, is `kill -s KILL -- -<pgid>` of that group, done with the `kill` command
  (std has no group kill and this crate forbids `unsafe`), followed by a kill of the direct child. On
  Windows the tree is ended with `taskkill /PID <pid> /T /F`, then the direct child. Before this, only
  the direct child was killed, so a process it had started kept running and kept ounce-hb waiting.
- **Bounded waits.** After the child is reaped, ounce-hb waits at most 2 seconds (`PIPE_CLOSE_GRACE`)
  for its pipes to close. If something still holds them, on unix the group is killed once more and
  the wait repeats once; then ounce-hb uses the output it has. A leaked grandchild cannot hang
  ounce-hb.
- **Death watch (unix).** No signal handler is installed: that needs `unsafe`, which this crate
  forbids, and no crate dependency is used. Instead ounce-hb starts a small `sh` process next to each
  run and holds a pipe to it. The watch ignores `INT`, `TERM` and `HUP` and waits for one line. When
  ounce-hb has reaped the child it writes `ok`, and the watch exits. If ounce-hb dies first, for any
  reason (SIGTERM from a job runner, Ctrl-C, SIGKILL, a crash), the pipe closes with no line and the
  watch removes the staged directory and kills ounce-core's process group. The watch joins
  ounce-core's process group, so a kill aimed at ounce-hb's own group does not take it along. Checked
  by `sigterm_and_sigint_kill_the_child_group_and_remove_the_staged_copy` in `tests/runner_harden.rs`.

  Limits of the watch: it needs `sh`, `kill` and `rm` (POSIX; present on Linux and macOS). If `sh`
  cannot be started the run goes ahead without it, as before. If the watch itself is killed with
  SIGKILL, nothing cleans up. The group is named by the child's pid; between the moment ounce-hb reaps
  the child and the moment it writes `ok` (microseconds) a death of ounce-hb would kill a group with
  that id if one had been created in the meantime, which POSIX forbids while any member of the old
  group lives. On Windows there is no watch: Ctrl-C in a console reaches ounce-core directly, and a
  `TerminateProcess` of ounce-hb leaves ounce-core running to its budget and the staged copy on disk.
- **Ctrl-C.** Because ounce-core is in its own process group, a terminal's SIGINT reaches ounce-hb and
  the watch only. ounce-hb dies, and the watch kills ounce-core. The shell reports ounce-hb as
  interrupted, as before.
- **Closed output.** If stdout or stderr is a pipe whose reader has gone (`| head`, `| true`, a crashed
  log collector), the write fails. The failure is ignored (a `BrokenPipe`) or reported on stderr once
  (any other error), and the documented exit code is returned. Before this, `ounce-hb run` exited `101`
  with a Rust panic.
- **Finding ounce-core on PATH.** An empty `PATH` entry is skipped instead of being read as the current
  directory, and a file without execute permission is passed over for the next entry. `OUNCE_CORE_BIN`
  is unchanged: it must be a file, and a file that cannot be started gives exit `5`.

## Guest-argument limit

The guest arguments together may be at most 16384 bytes. The total counts each argument's bytes plus one
NUL terminator, as argv strings are laid out in memory. A longer set is refused with exit code `2` before
ounce-core starts, and the message names the limit. The constant is `MAX_GUEST_ARGS_BYTES` in `src/lib.rs`.

The limit is a workaround for [ounce-core #418](https://github.com/hffgzktbcc-blip/ounce-core/issues/418):
in v0.8.0, once the argument strings pass about 62,839 bytes, the config block at `0x7FFF0000` runs into the
program at `0x80000000` and overwrites its first instruction. ounce-core v0.8.1 refuses such arguments
itself: an argument string (the program path and the guest arguments, joined by spaces) over 62,855 bytes
gives `[Loader Error] homebrew environment: argument string is <n> bytes, over the 62855-byte limit ...`
and exit `1`, which ounce-hb would report as `17`. Checked against the real v0.8.1 binary on
2026-11-13 with arguments of 62,845 to 65,000 bytes. ounce-hb's own limit stays, well below both
numbers, so the refusal is ounce-hb's clear exit-`2` message rather than a loader error, and so that
v0.8.0 binaries stay safe to use.

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

`ounce-hb run` kills ounce-core's process group after 60 seconds by default. The limit can be changed with
`OUNCE_HB_TIMEOUT_SECS`, a number of seconds greater than 0 and at most 86400 (one day). Surrounding
spaces are allowed. Any other value, including `nan`, `inf`, `0`, a negative number or text, is refused
with exit code `2` before ounce-core is looked for. The variable is not silently replaced by the default.
The limit applies to ounce-core's whole run, including startup.

The end-to-end test `wall_clock_timeout_exits_4_and_kills_ounce_core` (`tests/ounce_core_e2e.rs`) runs a
spinning NRO with `OUNCE_HB_TIMEOUT_SECS=2` and `OUNCE_NRO_BUDGET` set to 10^11, so only the wall-clock
limit can stop it. It asserts exit `4` and then checks that no process still has the run's private
directory in its command line (that covers the death watch too, whose command line names the
directory). `timeout_kills_the_whole_process_group_and_returns_promptly` in `tests/runner_harden.rs`
uses a stand-in ounce-core that starts a grandchild and a great-grandchild holding the pipes, and
checks that ounce-hb returns in about a second (12 seconds before the fix, with the grandchild left
running) and that none of them remains.

## Evidence status

Each exit code, and whether a real run backs it. "Real run" means `ounce-hb run` was executed against
the ounce-core v0.8.0 or v0.8.1 binary, or the captured ounce-core output from such a run is in the
fixtures. The end-to-end job pins v0.8.1; the plain fixtures are v0.8.0 captures and the spoof fixtures
are v0.8.1 captures, and the two versions print the same stop lines (checked by regenerating every
fixture program against v0.8.1 on 2026-11-13).

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

### Exit code by verified OS

The e2e job (`.github/workflows/ounce-core-e2e.yml`) runs the same tests against the pinned v0.8.1
release binary on each OS. "Verified" means the test ran on that OS in CI and passed. Nothing here is
from a local run on that OS.

| Code | Category | Linux x86_64 | macOS arm64 | Windows x86_64 |
|---|---|---|---|---|
| `0` | clean exit | Verified | Verified | Verified |
| `1` | unrecognised | Verified (killed by SIGKILL) | Verified (killed by SIGKILL) | Verified for a kill by `Stop-Process`, which reports exit code `-1`. Not a signal. A natural Windows stop with exit code 1 is not verified. |
| `2` | refused | Verified | Verified | Verified |
| `3` | core missing | Verified by `tests/cli.rs` in the `check` job, not by the e2e job | Verified by `tests/cli.rs` in the `check` job, not by the e2e job | Verified by `tests/cli.rs` in the `check` job, not by the e2e job |
| `4` | timeout | Verified | Verified | Verified |
| `5` | host I/O | Verified | Verified | Verified (a non-PE `.exe` cannot be spawned) |
| `10` | unimplemented instruction | Verified | Verified | Verified |
| `11` | memory fault | Verified | Verified | Verified |
| `12` | CPU error | Verified | Verified | Verified |
| `13` | svcBreak | Verified | Verified | Verified |
| `14` | unhandled SVC | Verified | Verified | Verified |
| `15` | CPU halted | Not reachable | Not reachable | Not reachable |
| `16` | budget exhausted | Verified | Verified | Verified |
| `17` | loader error | Verified | Verified | Verified |

What the Windows column does not cover:

- **Code `1` on Windows is the kill path only.** The test kills the real ounce-core with
  `Stop-Process -Force`. Windows has no signals, so the process exits with code `-1` (`0xFFFFFFFF`),
  and ounce-hb prints `ounce-core exit code: -1.` and exits `1`. The "killed by a signal" wording
  appears only on unix. The generated-run campaign behind the code `1` evidence above was not run on
  Windows, so no natural Windows stop reaches code `1` in the evidence.
- **Code `3` is not in the e2e job.** Its Windows evidence is `tests/cli.rs` in the `check` job,
  which runs `cargo test --workspace` on each OS, and that job passes on Windows.
- **No Windows fixture exists.** Everything in `tests/fixtures/ounce-core-output/` is a capture from
  Linux or macOS. The clean-exit test passed on Windows, so any stderr that ounce-core printed there
  matched the tolerated audio-noise prefixes or was empty. No Windows capture was saved, so the
  Windows stderr text is not recorded here.

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

Fixtures are real ounce-core output in `tests/fixtures/ounce-core-output/` (v0.8.0 for the plain
cases, v0.8.1 for the `spoof_*` cases): stdout, stderr and the exit status for each case, with
`NOTES.md` saying how each was made. `make_nros.py` in the same
directory writes the small NRO inputs. The NRO files are generated and not committed.

`tests/diagnose.rs` checks the classification of each fixture, the exit codes, the wording of each
explanation, and runs the real `ounce-hb` binary against a stand-in ounce-core that replays each
capture.

`tests/ounce_core_e2e.rs` runs the real `ounce-hb` binary against the real ounce-core CLI. It generates
each NRO inside the test and checks the exit code and category for each code above that a program can
reach, plus the four spoof programs. It skips with a SKIPPED line when no ounce-core binary is
configured. CI (`.github/workflows/ounce-core-e2e.yml`) downloads the v0.8.1 release asset for Linux
x86_64, macOS arm64 and Windows x86_64, checks its sha256 against both the hash pinned in the workflow
and the release's `SHA256SUMS`, runs the tests on each OS, and fails if any test was skipped.

`tests/runner_harden.rs` runs the real `ounce-hb` binary against stand-in ounce-core scripts for the
process-control cases: a closed stdout or stderr pipe, a timeout with a grandchild holding the pipes,
output past the 8 MiB limit, SIGTERM and SIGINT to ounce-hb, and the two PATH lookup cases. The job summary gives the passed, failed and skipped counts per
OS. The per-OS results are in the table above.
