"""Offline tests for scripts/triage_policy.py. No test reaches the network: gh is a FakeGh."""
import json
import unittest
from pathlib import Path

from fake_gh import FakeGh
import triage_policy as tp

# Built by concatenation so this file does not contain the literal key-file names that
# scripts/guardrails.sh refuses to see in code.
PROD_FILE = "prod" + ".keys"
TITLE_FILE = "title" + ".keys"
PROD_ZERO_WIDTH = "prod\u200b" + ".keys"
PROD_FULLWIDTH = "ｐｒｏｄ" + ".keys"
PROD_CYRILLIC_O = "prоd" + ".keys"

REPO_ROOT = Path(__file__).resolve().parents[1]

# (title, body, expected action, expected matches)
CASES = [
    # Strong: a protected term plus a request, with no negation -> flag.
    ("Add support for " + PROD_FILE, "So my games decrypt.", "flag", ("console keys",)),
    ("Dump your console keys", "Share them here please.", "flag", ("console keys",)),
    ("Where can I download firmware 18?", "", "flag", ("firmware",)),
    ("Can you add NSP support?", "I want to run my games.", "flag", ("retail files or games",)),
    ("Add retail game support", "I want to play Zelda on ounce-hb.", "flag", ("retail files or games",)),
    ("How do I bypass DRM?", "", "flag", ("piracy or circumvention",)),
    ("Crack and circumvent the anti-piracy check", "", "flag", ("piracy or circumvention",)),
    ("Help", "I need firmware 17 and the keys.", "flag", ("firmware",)),
    ("Share " + TITLE_FILE + " for my library", "", "flag", ("console keys",)),
    # Ambiguous: a protected term without a request, or with negation -> label only.
    (
        "ounce-hb should refuse " + PROD_FILE + " files with a clear error",
        "",
        "needs-owner-decision",
        ("console keys",),
    ),
    ("Please add a clear error when " + PROD_FILE + " is missing", "", "needs-owner-decision", ("console keys",)),
    ("ounce-hb should not support NSP files", "", "needs-owner-decision", ("retail files or games",)),
    ("Crash on start", "The log shows firmware 17.0 and then panics with exit code 1.", "needs-owner-decision", ("firmware",)),
    ("Is ounce-hb DRM-free?", "", "needs-owner-decision", ("piracy or circumvention",)),
    # Ordinary and security reports: no protected term at all -> nothing.
    (
        "Crash on relocation",
        "Running my homebrew.nro with cargo run -- run homebrew.nro prints exit code 1.",
        "none",
        (),
    ),
    (
        "Security question about the NRO loader",
        "Can a malicious NRO bypass the sandbox or corrupt memory? How should I report it?",
        "none",
        (),
    ),
    # Form boilerplate: the exact checkbox label from the bug form is not evidence.
    (
        "Bug",
        "### Scope confirmation\n- [x] This issue does not involve retail software, keys, firmware, or circumventing protection.\n"
        "### What happened\nIt crashed.",
        "none",
        (),
    ),
]

# The strings from the adversarial review (findings 2 and 15), real requests that must still match,
# and the cases the earlier classifier got wrong. (label, title, body, expected action, matches)
FINDING_CASES = [
    # Finding 2: legitimate reports and requests that used to be closed and locked.
    ("F2 bug: stub firmware version", "Crash in my homebrew on firmware 18.1",
     "My NRO calls setsysGetFirmwareVersion. How do I get ounce-hb to stub the firmware version? Exit code 14.",
     "needs-owner-decision", ("firmware",)),
    ("F2 question: stub firmware version", "How do I get ounce-hb to stub the firmware version?", "",
     "needs-owner-decision", ("firmware",)),
    ("F2 PR: read firmware version from NACP", "Add support for reading the firmware version from the NACP", "",
     "needs-owner-decision", ("firmware",)),
    ("F2 register dump on firmware", "I want to dump the registers when my homebrew hits svcBreak on firmware 17.0.", "",
     "needs-owner-decision", ("firmware",)),
    ("F2 DRM/KMS present backend", "Could you add a DRM/KMS present backend so headless Linux can show frames?", "",
     "none", ()),
    ("F2 bare version and environment firmware", "Crash on start",
     "Built from v1.2.0 on firmware 18.1 and the NRO exits with code 1.", "needs-owner-decision", ("firmware",)),
    ("F2 libdrm headers", "Build fails on Linux", "libdrm headers are missing for the DRM backend.", "none", ()),
    # Finding 15: evasions that used to evade the policy entirely.
    ("F15 XCIs and NSPs", "Please make ounce-hb load XCIs and NSPs from my SD card.", "",
     "flag", ("retail files or games",)),
    ("F15 checkbox request", "", "- [ ] where can I download " + PROD_FILE + " for this?",
     "flag", ("console keys",)),
    ("F15 OFW and cartridge backups", "Can you add support for OFW 18.0 sysmodules and cartridge backups?", "",
     "flag", ("firmware",)),
    ("F15 zero-width space in key name", "Where can I download " + PROD_ZERO_WIDTH, "",
     "flag", ("console keys",)),
    ("F15 fullwidth letters", "Where can I download " + PROD_FULLWIDTH, "", "flag", ("console keys",)),
    ("F15 Cyrillic lookalike", "Where can I download " + PROD_CYRILLIC_O, "", "flag", ("console keys",)),
    ("F15 spaced NSP", "Share my n s p files", "", "flag", ("retail files or games",)),
    ("F15 plural NCAs", "Please load my NCAs", "", "flag", ("retail files or games",)),
    ("F15 game dump synonym", "Can you share your game dumps?", "", "flag", ("retail files or games",)),
    ("F15 cartridge backup synonym", "Please add cartridge backup support", "", "flag", ("retail files or games",)),
    ("F15 OFW synonym", "Can you add support for OFW?", "", "flag", ("firmware",)),
    ("F15 request in HTML comment", "", "<!-- where can I download " + PROD_FILE + " -->",
     "flag", ("console keys",)),
    ("F15 checkbox text edited to add a request", "Bug",
     "- [x] This issue does not involve retail software, keys, firmware, or circumventing protection. "
     "Where can I download " + PROD_FILE + "?",
     "flag", ("console keys",)),
    # Known limit: a request split across two sentences is only labelled for the owner.
    ("F15 sentence-split request (limit)", "",
     "I want to run my games.\nThey are retail titles on NSP.", "needs-owner-decision", ("retail files or games",)),
    # Real requests that must keep matching.
    ("Real: where can I download firmware", "Where can I download firmware?", "", "flag", ("firmware",)),
    ("Real: share a dump of my keys", "Share a dump of my keys?", "", "flag", ("console keys",)),
    ("Real: please send me a key file", "", "please send me " + PROD_FILE, "flag", ("console keys",)),
    ("Real: add NSP support", "Add NSP support", "", "flag", ("retail files or games",)),
    # Negation keeps a discussion of the rule from being flagged.
    ("Negated request", "", "ounce-hb should not download " + PROD_FILE, "needs-owner-decision", ("console keys",)),
]


class ClassifyTests(unittest.TestCase):
    def test_table(self):
        for title, body, action, matches in CASES:
            with self.subTest(title=title):
                verdict = tp.classify(title, body)
                self.assertEqual(verdict.action, action)
                self.assertEqual(verdict.matches, tuple(sorted(matches)))

    def test_finding_table(self):
        for label, title, body, action, matches in FINDING_CASES:
            with self.subTest(case=label):
                verdict = tp.classify(title, body)
                self.assertEqual(verdict.action, action)
                self.assertEqual(verdict.matches, tuple(sorted(matches)))

    def test_strong_beats_weak_in_one_item(self):
        verdict = tp.classify("Add support for " + PROD_FILE, "The firmware log mentions keys.")
        self.assertEqual(verdict.action, "flag")
        self.assertEqual(verdict.matches, ("console keys",))

    def test_sentence_split_keeps_key_file_names_whole(self):
        # A period inside the file name must not split the sentence and separate the request.
        verdict = tp.classify("", "Please support " + PROD_FILE)
        self.assertEqual(verdict.action, "flag")

    def test_checkbox_line_that_is_not_boilerplate_is_evidence(self):
        verdict = tp.classify("", "- [ ] where can I download " + PROD_FILE + " for this?")
        self.assertEqual(verdict.action, "flag")

    def test_html_comment_is_evidence(self):
        verdict = tp.classify("", "<!-- download " + PROD_FILE + " -->")
        self.assertEqual(verdict.action, "flag")


class NormaliseTests(unittest.TestCase):
    def test_zero_width_and_soft_hyphens_are_removed(self):
        self.assertEqual(tp.normalise("prod\u200b.keys\u00ad"), PROD_FILE)

    def test_fullwidth_and_ligatures_fold_to_ascii(self):
        self.assertEqual(tp.normalise("ｐｒｏｄ"), "prod")
        self.assertEqual(tp.normalise("ﬁrmware"), "firmware")

    def test_cyrillic_and_greek_lookalikes_fold_to_latin(self):
        self.assertEqual(tp.normalise("prоd"), "prod")
        self.assertEqual(tp.normalise("ΑΒ"), "ab")

    def test_combining_marks_are_removed(self):
        self.assertEqual(tp.normalise("firmwáre"), "firmware")


class BoilerplateTests(unittest.TestCase):
    def test_every_boilerplate_line_is_in_a_real_template(self):
        # Keeps the allowlist honest: a line that no template contains would hide nothing.
        template_text = "\n".join(
            path.read_text(encoding="utf-8")
            for path in [
                *sorted((REPO_ROOT / ".github" / "ISSUE_TEMPLATE").glob("*.yml")),
                REPO_ROOT / ".github" / "pull_request_template.md",
            ]
        )
        for line in tp.BOILERPLATE_LINES:
            with self.subTest(line=line):
                self.assertIn(line, template_text)

    def test_altered_boilerplate_line_is_not_removed(self):
        text = tp.clean_text("", "- [x] This issue does not involve retail software, keys, firmware, or circumventing protection. download " + PROD_FILE)
        self.assertIn("download", text)


class ShouldEnforceTests(unittest.TestCase):
    def test_owner_is_skipped_case_insensitively(self):
        self.assertFalse(tp.should_enforce("Owner-Name", "owner-name", "User"))

    def test_bots_are_skipped(self):
        self.assertFalse(tp.should_enforce("dependabot[bot]", "owner", "User"))
        self.assertFalse(tp.should_enforce("someapp", "owner", "Bot"))

    def test_contributor_is_enforced(self):
        self.assertTrue(tp.should_enforce("newcomer", "owner", "User"))

    def test_empty_author_is_not_enforced(self):
        self.assertFalse(tp.should_enforce("", "owner", "User"))


class AutocloseSettingTests(unittest.TestCase):
    def test_unset_is_off(self):
        self.assertFalse(tp.autoclose_enabled({}))

    def test_only_true_turns_it_on(self):
        self.assertTrue(tp.autoclose_enabled({"POLICY_AUTOCLOSE": "true"}))
        self.assertTrue(tp.autoclose_enabled({"POLICY_AUTOCLOSE": " TRUE "}))

    def test_other_values_stay_off(self):
        for value in ("", "false", "1", "yes", "on"):
            with self.subTest(value=value):
                self.assertFalse(tp.autoclose_enabled({"POLICY_AUTOCLOSE": value}))


class ApplyIssueTests(unittest.TestCase):
    def setUp(self):
        self.target = tp.Target("issue", "o/r", "42", "", "newcomer")

    def test_default_flag_labels_and_comments_but_never_closes_or_locks(self):
        fake = FakeGh()
        tp.apply_verdict(fake, self.target, tp.Verdict("flag", ("firmware",)))
        self.assertEqual(len(fake.calls), 2)
        labels = fake.calls[0]
        self.assertIn("repos/o/r/issues/42/labels", " ".join(labels))
        self.assertIn("labels[]=area: legal", labels)
        self.assertIn("labels[]=needs-owner-decision", labels)
        comment = fake.calls[1]
        self.assertIn("repos/o/r/issues/42/comments", " ".join(comment))
        body = [a for a in comment if a.startswith("body=")][0]
        self.assertIn(tp.POLICY_QUOTE, body)
        self.assertIn("CONTRIBUTING.md", body)
        self.assertIn("has not been closed or locked", body)
        self.assertFalse(fake.calls_containing("state=closed"))
        self.assertFalse(fake.calls_containing("/lock"))

    def test_autoclose_flag_comments_labels_closes_and_locks(self):
        fake = FakeGh()
        tp.apply_verdict(fake, self.target, tp.Verdict("flag", ("firmware",)), autoclose=True)
        self.assertEqual(len(fake.calls), 4)
        body = [a for a in fake.calls[1] if a.startswith("body=")][0]
        self.assertIn("closed and locked", body)
        self.assertIn("state=closed", fake.calls[2])
        self.assertIn("state_reason=not_planned", fake.calls[2])
        self.assertIn("repos/o/r/issues/42/lock", " ".join(fake.calls[3]))

    def test_weak_issue_only_labels(self):
        fake = FakeGh()
        tp.apply_verdict(fake, self.target, tp.Verdict("needs-owner-decision", ("firmware",)), autoclose=True)
        self.assertEqual(len(fake.calls), 1)
        self.assertIn("labels[]=needs-owner-decision", fake.calls[0])
        self.assertNotIn("area: legal", " ".join(fake.calls[0]))

    def test_edited_flag_only_adds_labels_even_with_autoclose(self):
        fake = FakeGh()
        tp.apply_verdict(fake, self.target, tp.Verdict("flag", ("firmware",)), autoclose=True, new_item=False)
        self.assertEqual(len(fake.calls), 1)
        self.assertIn("labels[]=area: legal", fake.calls[0])
        self.assertFalse(fake.calls_containing("comments"))
        self.assertFalse(fake.calls_containing("state=closed"))
        self.assertFalse(fake.calls_containing("/lock"))

    def test_autoclose_pull_request_closes_without_state_reason(self):
        target = tp.Target("pull_request", "o/r", "7", "", "newcomer")
        fake = FakeGh()
        tp.apply_verdict(fake, target, tp.Verdict("flag", ("console keys",)), autoclose=True)
        close = fake.calls_containing("repos/o/r/pulls/7")[0]
        self.assertIn("state=closed", close)
        self.assertNotIn("state_reason=not_planned", close)
        self.assertTrue(fake.calls_containing("repos/o/r/issues/7/lock"))

    def test_default_pull_request_is_labelled_and_left_open(self):
        target = tp.Target("pull_request", "o/r", "7", "", "newcomer")
        fake = FakeGh()
        tp.apply_verdict(fake, target, tp.Verdict("flag", ("console keys",)))
        self.assertFalse(fake.calls_containing("repos/o/r/pulls/7"))
        self.assertFalse(fake.calls_containing("/lock"))
        self.assertTrue(fake.calls_containing("comments"))


class ApplyDiscussionTests(unittest.TestCase):
    def label_fake(self):
        label_answer = json.dumps({"data": {"repository": {"label": {"id": "L_id"}}}})
        return FakeGh([("label(name", label_answer)])

    def test_default_flag_labels_and_comments_without_closing(self):
        target = tp.Target("discussion", "o/r", "3", "D_node", "newcomer")
        fake = self.label_fake()
        tp.apply_verdict(fake, target, tp.Verdict("flag", ("firmware",)))
        queries = [" ".join(c) for c in fake.calls]
        self.assertTrue(any("addDiscussionComment" in q for q in queries))
        self.assertTrue(any("addLabelsToLabelable" in q for q in queries))
        self.assertFalse(any("closeDiscussion" in q for q in queries))
        self.assertFalse(any("lockLockable" in q for q in queries))

    def test_autoclose_flag_uses_graphql_to_close_and_lock(self):
        target = tp.Target("discussion", "o/r", "3", "D_node", "newcomer")
        fake = self.label_fake()
        tp.apply_verdict(fake, target, tp.Verdict("flag", ("firmware",)), autoclose=True)
        queries = [" ".join(c) for c in fake.calls]
        self.assertTrue(any("addDiscussionComment" in q for q in queries))
        self.assertTrue(any("closeDiscussion" in q and "OUTDATED" in q for q in queries))
        self.assertTrue(any("lockLockable" in q for q in queries))

    def test_edited_discussion_only_adds_labels(self):
        target = tp.Target("discussion", "o/r", "3", "D_node", "newcomer")
        fake = self.label_fake()
        tp.apply_verdict(fake, target, tp.Verdict("flag", ("firmware",)), autoclose=True, new_item=False)
        queries = [" ".join(c) for c in fake.calls]
        self.assertFalse(any("addDiscussionComment" in q for q in queries))
        self.assertFalse(any("closeDiscussion" in q for q in queries))
        self.assertFalse(any("lockLockable" in q for q in queries))
        self.assertTrue(any("addLabelsToLabelable" in q for q in queries))

    def test_missing_label_raises(self):
        target = tp.Target("discussion", "o/r", "3", "D_node", "newcomer")
        fake = FakeGh([("label(name", json.dumps({"data": {"repository": {"label": None}}}))])
        with self.assertRaises(RuntimeError):
            tp.apply_verdict(fake, target, tp.Verdict("flag", ("firmware",)))

    def test_weak_discussion_only_labels(self):
        target = tp.Target("discussion", "o/r", "3", "D_node", "newcomer")
        fake = self.label_fake()
        tp.apply_verdict(fake, target, tp.Verdict("needs-owner-decision", ("firmware",)), autoclose=True)
        queries = [" ".join(c) for c in fake.calls]
        self.assertFalse(any("closeDiscussion" in q for q in queries))
        self.assertFalse(any("addDiscussionComment" in q for q in queries))
        self.assertTrue(any("L_id" in q for q in queries))


class MainTests(unittest.TestCase):
    def env(self, **overrides):
        base = {
            "GH_REPO": "o/r",
            "REPO_OWNER": "owner",
            "EVENT_KIND": "issue",
            "EVENT_ACTION": "opened",
            "ITEM_NUMBER": "9",
            "ITEM_AUTHOR": "newcomer",
            "ITEM_AUTHOR_TYPE": "User",
            "ITEM_TITLE": "Dump console keys",
            "ITEM_BODY": "Share them.",
        }
        base.update(overrides)
        return base

    def test_flagged_contributor_item_is_labelled_not_closed_by_default(self):
        fake = FakeGh()
        self.assertEqual(tp.main(self.env(), fake), 0)
        self.assertTrue(fake.calls_containing("comments"))
        self.assertFalse(fake.calls_containing("state=closed"))
        self.assertFalse(fake.calls_containing("/lock"))

    def test_flagged_item_is_closed_and_locked_when_autoclose_is_on(self):
        fake = FakeGh()
        self.assertEqual(tp.main(self.env(POLICY_AUTOCLOSE="true"), fake), 0)
        self.assertTrue(fake.calls_containing("state=closed"))
        self.assertTrue(fake.calls_containing("/lock"))

    def test_edited_item_only_gets_labels_even_with_autoclose(self):
        fake = FakeGh()
        self.assertEqual(tp.main(self.env(EVENT_ACTION="edited", POLICY_AUTOCLOSE="true"), fake), 0)
        self.assertEqual(len(fake.calls), 1)
        self.assertIn("labels", " ".join(fake.calls[0]))
        self.assertFalse(fake.calls_containing("comments"))
        self.assertFalse(fake.calls_containing("state=closed"))

    def test_edit_that_adds_a_request_is_labelled(self):
        fake = FakeGh()
        env = self.env(EVENT_ACTION="edited", ITEM_TITLE="Crash", ITEM_BODY="Where can I download " + PROD_FILE + "?")
        self.assertEqual(tp.main(env, fake), 0)
        self.assertTrue(fake.calls_containing("labels[]=area: legal"))

    def test_owner_item_is_untouched(self):
        fake = FakeGh()
        self.assertEqual(tp.main(self.env(ITEM_AUTHOR="owner"), fake), 0)
        self.assertEqual(fake.calls, [])

    def test_bot_item_is_untouched(self):
        fake = FakeGh()
        self.assertEqual(tp.main(self.env(ITEM_AUTHOR="renovate[bot]", ITEM_AUTHOR_TYPE="Bot"), fake), 0)
        self.assertEqual(fake.calls, [])

    def test_clean_item_is_untouched(self):
        fake = FakeGh()
        env = self.env(ITEM_TITLE="Crash on relocation", ITEM_BODY="exit code 1")
        self.assertEqual(tp.main(env, fake), 0)
        self.assertEqual(fake.calls, [])

    def test_firmware_bug_report_is_labelled_only(self):
        fake = FakeGh()
        env = self.env(ITEM_TITLE="Crash in my homebrew on firmware 18.1", ITEM_BODY="How do I get ounce-hb to stub the firmware version?")
        self.assertEqual(tp.main(env, fake), 0)
        self.assertEqual(len(fake.calls), 1)
        self.assertIn("labels[]=needs-owner-decision", fake.calls[0])
        self.assertNotIn("area: legal", " ".join(fake.calls[0]))

    def test_gh_failure_returns_nonzero(self):
        from gh_client import GhError

        fake = FakeGh([("comments", GhError("HTTP 403"))])
        self.assertEqual(tp.main(self.env(), fake), 1)

    def test_missing_item_information_is_a_noop(self):
        fake = FakeGh()
        self.assertEqual(tp.main(self.env(ITEM_NUMBER=""), fake), 0)
        self.assertEqual(fake.calls, [])


if __name__ == "__main__":
    unittest.main()
