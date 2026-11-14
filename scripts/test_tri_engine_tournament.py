#!/usr/bin/env python3
"""Unit tests for scripts/tri_engine_tournament.py."""

import unittest
from pathlib import Path

from tri_engine_tournament import (
    make_synthetic_nro,
    parse_stage20,
    parse_summary,
    render_markdown,
    sanitize_text,
)


class TestTriEngineTournament(unittest.TestCase):
    def test_synthetic_nro_builder(self):
        nro_bytes = make_synthetic_nro([0x52800000, 0xD40000E1])
        self.assertEqual(len(nro_bytes), 0x80 + 8)
        self.assertEqual(nro_bytes[0x10:0x14], b"NRO0")
        self.assertEqual(int.from_bytes(nro_bytes[0x18:0x1C], "little"), 0x80 + 8)

    def test_sanitize_text(self):
        text = "error at /Users/john_doe/orca/project and /tmp/foo/bar.nro"
        cleaned = sanitize_text(text, "/Users/john_doe")
        self.assertNotIn("john_doe", cleaned)

    def test_parse_summary(self):
        stdout = (
            "[VM Summary] Executed 16,460,486 instructions in 305.35ms (53.91 MIPS) | Frames: 15\n"
            "  -> Basic-Block JIT Cache: 2010 Blocks (10607 Insns) | Hits: 8950443 | Misses: 2010\n"
        )
        res = parse_summary(stdout)
        self.assertEqual(res["instructions"], 16460486)
        self.assertEqual(res["elapsed_ms"], 305.35)
        self.assertEqual(res["mips"], 53.91)
        self.assertEqual(res["blocks_compiled"], 2010)
        self.assertEqual(res["jit_hits"], 8950443)

    def test_parse_stage20(self):
        stdout = (
            "[Stage 20/25] Tier-3 JIT Super-Block Execution & Multi-Asset Package Pipeline\n"
            "  [Tier-4 JIT Engine] Executed 15304 insns (4 native blocks compiled, 5037 native runs, 15109 native / 195 Tier-3 insns) | Exit: Svc { svc_id: 7 }\n"
            "  [Tier-4 Throughput] Measured execution performance: 8.55 MIPS\n"
            "  [Gate] Stage 20 PASS (Tier-3 guest X0 = 0xAAA)\n"
        )
        res = parse_stage20(stdout)
        self.assertEqual(res["engine"], "Tier-4")
        self.assertEqual(res["instructions"], 15304)
        self.assertEqual(res["native_blocks"], 4)
        self.assertEqual(res["native_runs"], 5037)
        self.assertEqual(res["mips"], 8.55)
        self.assertTrue(res["pass"])

    def test_render_markdown(self):
        sample_results = {
            "metadata": {
                "timestamp": "2026-11-14T12:00:00Z",
                "host_os": "Darwin",
                "host_machine": "arm64",
                "python_version": "3.11.0",
                "core_bin": "ounce-core",
                "runtime_bin": "ounce-runtime",
                "hb_bin": "ounce-hb",
            },
            "event_1_homebrew": [
                {
                    "name": "clean_exit",
                    "expected": "Process exited gracefully",
                    "runs": {
                        "direct_core": {"exit_code": 0},
                        "direct_runtime": {"exit_code": 0},
                        "hb_core": {"exit_code": 0},
                        "hb_runtime": {"exit_code": 0},
                    },
                }
            ],
            "event_2_stage20": [
                {
                    "impl": "ounce-core",
                    "cpu_tier": "tier4",
                    "exit_code": 0,
                    "stage20_insns": 15304,
                    "stage20_mips": 8.55,
                    "native_blocks": 4,
                    "stage20_pass": True,
                }
            ],
            "event_3_conformance": [
                {
                    "impl": "ounce-runtime",
                    "tier": "nce",
                    "score": "12/12",
                    "passed_cases": 12,
                    "total_cases": 12,
                }
            ],
        }
        md = render_markdown(sample_results)
        self.assertIn("# The Great Tri-Engine Tournament", md)
        self.assertIn("15,304", md)
        self.assertIn("12/12", md)


if __name__ == "__main__":
    unittest.main()
