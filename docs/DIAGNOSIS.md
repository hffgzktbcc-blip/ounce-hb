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
| `2` | refused | The request was refused before ounce-core ran: a guest argument starting with `-`, an unsupported NSO kind, or an input that is not an unencrypted NRO (also `2` for the input checks that run before this step). |
| `3` | core missing | No ounce-core binary: `OUNCE_CORE_BIN` is unset or wrong, and no `ounce-core` is on `PATH`. |
| `4` | timeout | ounce-core ran past the 60-second limit and was killed. |
| `5` | host I/O | ounce-hb could not write its private copy of the program, or could not start or wait on ounce-core. |
| `10` | unimplemented instruction | The CPU met a word ounce-core does not implement. The message gives the word and the address. |
| `11` | memory fault | ounce-core rejected a memory access. The message keeps ounce-core's wording and, when present, the address. |
| `12` | CPU error | Any other `[VM Error]` line, such as a breakpoint or a branch-target exception. |
| `13` | svcBreak | The guest called svcBreak. The message gives the break PC and result when ounce-core printed them. |
| `14` | unhandled SVC | The guest called a system service ounce-core does not provide. |
| `15` | CPU halted | ounce-core printed `CPU Halted`. |
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
- The timeout category (`4`) is decided by ounce-hb's own wall-clock limit, not by ounce-core. The
  unit and integration tests cover that mapping. A real timeout run was not captured for this document:
  the synthetic infinite loop reached the default instruction budget (50 million) within about one
  second, so it reported category `16`, not `4`.

## Evidence

Fixtures are real ounce-core v0.8.0 output in `tests/fixtures/ounce-core-output/`: stdout, stderr and
the exit status for each case, with `NOTES.md` saying how each was made. `make_nros.py` in the same
directory writes the small NRO inputs. The NRO files are generated and not committed.

`tests/diagnose.rs` checks the classification of each fixture, the exit codes, the wording of each
explanation, and runs the real `ounce-hb` binary against a stand-in ounce-core that replays each
capture.
