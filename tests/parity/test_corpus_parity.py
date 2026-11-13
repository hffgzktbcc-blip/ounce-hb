"""Unit tests for scripts/corpus_parity.py. Run with: python3 -m unittest discover -s tests/parity

The stop lines used here are the ones docs/DIAGNOSIS.md lists. The end-to-end test uses small shell
scripts as stand-ins for ounce-core and ounce-hb, so no NRO or devkitA64 toolchain is needed.
"""

import contextlib
import importlib.util
import io
import json
import os
import stat
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location("corpus_parity", REPO / "scripts" / "corpus_parity.py")
cp = importlib.util.module_from_spec(_spec)
sys.modules["corpus_parity"] = cp
_spec.loader.exec_module(cp)

CLEAN = "[VM] Process exited gracefully (svcExitProcess)."
CODES = {"clean": 0, "unrecognised": 1, "unimplemented": 10, "memory_fault": 11, "cpu_error": 12,
         "break": 13, "unhandled_svc": 14, "cpu_halted": 15, "budget": 16, "loader": 17}
DOC = """# Exit codes

## Exit codes

| Code | Category | Meaning |
|---|---|---|
| `0` | clean exit | x |
| `1` | unrecognised | x |
| `10` | unimplemented instruction | x |
| `11` | memory fault | x |
| `12` | CPU error | x |
| `13` | svcBreak | x |
| `14` | unhandled SVC | x |
| `15` | CPU halted | x |
| `16` | budget exhausted | x |
| `17` | loader error | x |

## What is matched

| x | y |
"""


def run(exit_code, stdout="", stderr="", status="ok"):
    return cp.Run(status, exit_code, stdout.encode(), stderr.encode(), 0.01)


class ClassifyTests(unittest.TestCase):
    def cat(self, exit_code, stdout="", stderr=""):
        return cp.classify_core_output(run(exit_code, stdout, stderr))

    def test_clean_needs_exit_zero_and_the_clean_line(self):
        self.assertEqual(self.cat(0, "hello\n" + CLEAN + "\n"), "clean")
        self.assertEqual(self.cat(0, "hello\n"), "unrecognised")
        self.assertEqual(self.cat(1, CLEAN), "unrecognised")

    def test_clean_line_on_stderr_counts(self):
        self.assertEqual(self.cat(0, "", CLEAN), "clean")

    def test_clean_with_a_fault_line_is_unrecognised(self):
        self.assertEqual(self.cat(0, CLEAN + "\n[VM] Execution budget reached (5 instructions)."), "unrecognised")

    def test_each_stop_line(self):
        cases = {
            "[VM Error] Unimplemented instruction 0x12345678 at PC: 0x0000000080000080": "unimplemented",
            "[VM Error] Memory access violation: bad read at virtual address: 0x10": "memory_fault",
            "[VM Error] Breakpoint hit": "cpu_error",
            "[VM] Hit svcBreak: 1": "break",
            "[VM] Stopping on unhandled SVC #0x2A.": "unhandled_svc",
            "[VM] CPU Halted: wfi": "cpu_halted",
            "[VM] Execution budget reached (50000000 instructions).": "budget",
            "[Loader Error] bad magic": "loader",
        }
        for line, key in cases.items():
            with self.subTest(line=line):
                self.assertEqual(self.cat(1, line), key)

    def test_unparsable_stop_shapes_are_unrecognised(self):
        self.assertEqual(self.cat(1, "[VM Error] Unimplemented instruction 0xZZ"), "unrecognised")
        self.assertEqual(self.cat(1, "[VM] Stopping on unhandled SVC #0x1FFFF."), "unrecognised")
        self.assertEqual(self.cat(1, "[VM] Execution budget reached (many)."), "unrecognised")

    def test_lines_are_trimmed_before_matching(self):
        self.assertEqual(self.cat(1, "   [VM] Hit svcBreak:   "), "break")

    def test_signal_and_no_line_are_unrecognised(self):
        self.assertEqual(self.cat(None, "[VM] Execution budget reached (5 instructions)."), "unrecognised")
        self.assertEqual(self.cat(1, "just text"), "unrecognised")

    def test_exit_table_parses_and_requires_every_category(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp, "DIAGNOSIS.md")
            path.write_text(DOC)
            self.assertEqual(cp.load_exit_codes(path)["budget"], 16)
            self.assertEqual(cp.load_exit_codes(path)["clean"], 0)
            path.write_text(DOC.replace("| `17` | loader error | x |\n", ""))
            with self.assertRaises(cp.ConfigError):
                cp.load_exit_codes(path)


class CompareTests(unittest.TestCase):
    NRO = "/work/prog.nro"

    def check(self, direct, via, nro=NRO):
        return cp.compare(direct, via, CODES, nro)

    def test_clean_pair_matches(self):
        out = "hi\n" + CLEAN + "\n"
        self.assertEqual(self.check(run(0, out), run(0, out))["verdict"], "match")

    def test_exit_must_be_the_category_code(self):
        out = "hi\n" + CLEAN + "\n"
        result = self.check(run(0, out), run(1, out))
        self.assertEqual(result["verdict"], "differ")
        self.assertIn("expected 0", result["reasons"][0])

    def test_stdout_difference_is_a_difference(self):
        result = self.check(run(0, CLEAN + "\nA"), run(0, CLEAN + "\nB"))
        self.assertEqual(result["verdict"], "differ")
        self.assertIn("stdout differs", result["reasons"])

    def test_clean_run_must_have_identical_stderr(self):
        result = self.check(run(0, CLEAN), run(0, CLEAN, "extra"))
        self.assertIn("stderr differs", result["reasons"])

    def test_stopped_run_may_carry_the_explanation_after_guest_stderr(self):
        budget = "[VM] Execution budget reached (50000000 instructions)."
        direct = run(1, budget, "guest warning\n")
        via = run(16, budget, "guest warning\nounce-hb: the program ran out of instructions.\n")
        result = self.check(direct, via)
        self.assertEqual((result["verdict"], result["category"], result["expected_exit"]),
                         ("match", "budget", 16))

    def test_stopped_run_must_keep_the_guest_stderr_first(self):
        budget = "[VM] Execution budget reached (50000000 instructions)."
        result = self.check(run(1, budget, "guest warning\n"), run(16, budget, "ounce-hb: x\n"))
        self.assertIn("stderr differs before the ounce-hb explanation", result["reasons"])

    def test_ounce_hb_refusal_is_a_category_difference(self):
        budget = "[VM] Execution budget reached (50000000 instructions)."
        result = self.check(run(1, budget), run(2, "", "ounce-hb: rejected"))
        self.assertEqual(result["verdict"], "differ")
        self.assertIn("ounce-hb exit 2, expected 16", result["reasons"][0])

    def test_timeouts_are_errors(self):
        self.assertEqual(self.check(run(None, status="timeout"), run(0))["verdict"], "error")
        self.assertEqual(self.check(run(0), run(None, status="timeout"))["verdict"], "error")

    def test_nro_path_is_masked_in_both_outputs(self):
        direct = run(0, "[Loader] Reading package/executable: /work/prog.nro\n" + CLEAN)
        via = run(0, "[Loader] Reading package/executable: /var/T/ounce-hb-run-26049-17945-0/program.nro\n" + CLEAN)
        self.assertEqual(self.check(direct, via)["verdict"], "match")

    def test_timing_and_jit_counters_are_masked(self):
        direct = run(0, "[VM Summary] Executed 50000000 instructions in 1.7437s (28.67 MIPS) | Frames: 0\n"
                        "  -> Basic-Block JIT Cache: 1632 Blocks (9165 Insns) | Hits: 1 | Misses: 2\n" + CLEAN)
        via = run(0, "[VM Summary] Executed 50000000 instructions in 912.3ms (54.8 MIPS) | Frames: 0\n"
                     "  -> Basic-Block JIT Cache: 1633 Blocks (9168 Insns) | Hits: 3 | Misses: 4\n" + CLEAN)
        self.assertEqual(self.check(direct, via)["verdict"], "match")

    def test_path_dependent_final_register_is_masked(self):
        budget = "[VM] Execution budget reached (50000000 instructions).\n"
        direct = run(1, budget + "  -> Final X0: 32 (0x0000000000000020) | D0: 0.00\n")
        via = run(16, budget + "  -> Final X0: 80 (0x0000000000000050) | D0: 0.00\n", "ounce-hb: x\n")
        self.assertEqual(self.check(direct, via)["verdict"], "match")

    def test_instruction_count_on_the_summary_line_is_masked(self):
        direct = run(0, "[VM Summary] Executed 16556877 instructions in 1s (1 MIPS) | Frames: 0\n" + CLEAN)
        via = run(0, "[VM Summary] Executed 16557482 instructions in 1s (1 MIPS) | Frames: 0\n" + CLEAN)
        self.assertEqual(self.check(direct, via)["verdict"], "match")

    def test_host_audio_noise_on_stderr_is_dropped(self):
        direct = run(0, CLEAN, "ALSA lib conf.c:1 (x) cannot find card '0'\n")
        via = run(0, CLEAN, "[HostAudioSink] Output stream error: underrun\n")
        self.assertEqual(self.check(direct, via)["verdict"], "match")

    def test_audio_noise_on_stdout_is_still_compared(self):
        direct = run(0, CLEAN + "\nALSA lib x")
        via = run(0, CLEAN)
        self.assertEqual(self.check(direct, via)["verdict"], "differ")

    def test_memory_bucket_sizes_are_masked(self):
        direct = run(1, "  -> Memory Buckets (267248 KB): Code=140 KB | Data=262480 KB\n"
                        "[VM] Execution budget reached (50000000 instructions).")
        via = run(16, "  -> Memory Buckets (267332 KB): Code=196 KB | Data=262508 KB\n"
                      "[VM] Execution budget reached (50000000 instructions).", "ounce-hb: x\n")
        self.assertEqual(self.check(direct, via)["verdict"], "match")

    def test_budget_line_count_is_still_compared(self):
        direct = run(1, "[VM] Execution budget reached (50000000 instructions).")
        via = run(16, "[VM] Execution budget reached (50000001 instructions).")
        self.assertEqual(self.check(direct, via)["verdict"], "differ")


class RegressionTests(unittest.TestCase):
    def test_match_that_stops_matching_is_a_regression(self):
        self.assertEqual(cp.regressions({"a": "match"}, {"a": "differ"}, ["a"], False),
                         ["a: baseline match, now differ"])

    def test_missing_counts_only_with_require_present(self):
        self.assertEqual(cp.regressions({"a": "match"}, {"a": "missing"}, ["a"], False), [])
        self.assertEqual(cp.regressions({"a": "match"}, {"a": "missing"}, ["a"], True),
                         ["a: baseline match, now missing"])

    def test_program_removed_from_corpus_is_a_regression(self):
        self.assertEqual(cp.regressions({"a": "match"}, {}, ["a"], False),
                         ["a: baseline match, now absent from the corpus"])

    def test_only_baseline_matches_are_enforced(self):
        self.assertEqual(cp.regressions({"a": "differ", "b": "error"}, {"a": "error", "b": "differ"},
                                        ["a", "b"], True), [])

    def test_recovery_and_new_programs_are_not_regressions(self):
        self.assertEqual(cp.regressions({"a": "differ"}, {"a": "match", "new": "differ"}, ["a", "new"], True), [])

    def test_unselected_programs_are_not_checked(self):
        self.assertEqual(cp.regressions({"a": "match"}, {}, [], False), [])


class CorpusLoadTests(unittest.TestCase):
    def write(self, tmp, oracles):
        Path(tmp, "tests", "corpus").mkdir(parents=True)
        Path(tmp, "tests", "corpus", "oracles.json").write_text(json.dumps({"oracles": oracles}))
        return Path(tmp)

    def test_valid_registry(self):
        with tempfile.TemporaryDirectory() as tmp:
            core = self.write(tmp, [{"id": "a", "build": "switch-examples", "nro": "x/a.nro"}])
            self.assertEqual(cp.load_corpus(core), [{"id": "a", "build": "switch-examples", "nro": "x/a.nro"}])

    def test_unsafe_or_duplicate_entries_are_rejected(self):
        bad = [
            [{"id": "a", "build": "b", "nro": "/abs.nro"}],
            [{"id": "a", "build": "b", "nro": "../escape.nro"}],
            [{"id": "a", "build": "../b", "nro": "x.nro"}],
            [{"id": "a", "build": "b", "nro": "x.nro"}, {"id": "a", "build": "b", "nro": "y.nro"}],
            [{"id": "a", "build": "b"}],
        ]
        for oracles in bad:
            with self.subTest(oracles=oracles), tempfile.TemporaryDirectory() as tmp:
                core = self.write(tmp, oracles)
                with self.assertRaises(cp.ConfigError):
                    cp.load_corpus(core)


class RunCommandTests(unittest.TestCase):
    def test_captures_exit_and_output(self):
        r = cp.run_command(["sh", "-c", "echo hi; echo err >&2; exit 3"], dict(os.environ), 30)
        self.assertEqual((r.status, r.exit_code, r.stdout, r.stderr), ("ok", 3, b"hi\n", b"err\n"))

    def test_signal_has_no_exit_code(self):
        r = cp.run_command(["sh", "-c", "kill -9 $$"], dict(os.environ), 30)
        self.assertEqual((r.status, r.exit_code), ("ok", None))
        self.assertIn("signal 9", r.detail)

    def test_timeout_kills_the_whole_process_group(self):
        # The shell's child stands in for ounce-core, which ounce-hb starts. It must die with the run.
        started = time.monotonic()
        r = cp.run_command(["sh", "-c", "sleep 30 & wait"], dict(os.environ), 0.5)
        self.assertEqual(r.status, "timeout")
        self.assertLess(time.monotonic() - started, 10)

    def test_timeout_and_spawn_error(self):
        self.assertEqual(cp.run_command(["sleep", "5"], dict(os.environ), 0.2).status, "timeout")
        self.assertEqual(cp.run_command(["/nonexistent/ounce-core"], dict(os.environ), 5).status, "spawn-error")


class EndToEndTests(unittest.TestCase):
    """Fake ounce-core and ounce-hb scripts drive the whole run, including the baseline gate."""

    def fake(self, path, body):
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(path.stat().st_mode | stat.S_IEXEC)
        return path

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        core = self.tmp / "core"
        (core / "tests" / "corpus").mkdir(parents=True)
        (core / "tests" / "corpus" / "oracles.json").write_text(json.dumps({"oracles": [
            {"id": "prog", "build": "b", "nro": "p.nro"},
            {"id": "absent", "build": "b", "nro": "missing.nro"},
        ]}))
        nro = self.tmp / "builds" / "b" / "build" / "source" / "p.nro"
        nro.parent.mkdir(parents=True)
        nro.write_bytes(b"NRO0 fake")
        self.codes = self.tmp / "DIAGNOSIS.md"
        self.codes.write_text(DOC)
        clean = f"echo '{CLEAN}'; exit 0"
        self.core_bin = self.fake(self.tmp / "ounce-core", clean)
        self.hb_ok = self.fake(self.tmp / "ounce-hb-ok", f"echo '{CLEAN}'; exit 0")
        self.hb_bad = self.fake(self.tmp / "ounce-hb-bad", f"echo '{CLEAN}'; exit 5")
        self.core = core

    def run_main(self, hb, baseline, extra=()):
        argv = ["--ounce-core-dir", str(self.core), "--builds-dir", str(self.tmp / "builds"),
                "--ounce-hb", str(hb), "--exit-codes", str(self.codes), "--report",
                str(self.tmp / "out" / "report.json"), "--baseline", str(baseline), *extra]
        quiet = io.StringIO()
        with mock.patch.dict(os.environ, {"OUNCE_CORE_BIN": str(self.core_bin)}), \
                contextlib.redirect_stdout(quiet), contextlib.redirect_stderr(quiet):
            return cp.main(argv)

    def test_baseline_gate_end_to_end(self):
        baseline = self.tmp / "baseline.json"
        self.assertEqual(self.run_main(self.hb_ok, self.tmp / "none.json"), 2)  # no baseline yet
        self.assertEqual(self.run_main(self.hb_ok, baseline, ["--write-baseline", str(baseline)]), 0)
        recorded = json.loads(baseline.read_text())["programs"]
        self.assertEqual(recorded, {"prog": "match", "absent": "missing"})
        self.assertEqual(self.run_main(self.hb_ok, baseline), 0)
        self.assertEqual(self.run_main(self.hb_bad, baseline), 1)  # match stopped matching
        report = json.loads((self.tmp / "out" / "report.json").read_text())
        self.assertEqual(report["summary"]["differ"], 1)
        self.assertEqual(report["programs"][1]["verdict"], "missing")

    def test_missing_counts_with_require_present(self):
        baseline = self.tmp / "baseline.json"
        baseline.write_text(json.dumps({"programs": {"prog": "match", "absent": "match"}}))
        self.assertEqual(self.run_main(self.hb_ok, baseline), 0)
        self.assertEqual(self.run_main(self.hb_ok, baseline, ["--require-present"]), 1)

    def test_missing_ounce_core_bin_is_a_configuration_error(self):
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertEqual(cp.main(["--ounce-core-dir", str(self.core), "--ounce-hb", str(self.hb_ok)]), 2)


if __name__ == "__main__":
    unittest.main()
