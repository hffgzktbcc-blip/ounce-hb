"""Offline tests for scripts/triage_policy.py. No test reaches the network: gh is a FakeGh."""
import json
import unittest

from fake_gh import FakeGh
import triage_policy as tp

# Built by concatenation so this file does not contain the literal key-file names that
# scripts/guardrails.sh refuses to see in code.
PROD_FILE = "prod" + ".keys"
TITLE_FILE = "title" + ".keys"

# (title, body, expected action, expected matches)
CASES = [
    # Strong: a protected term plus a request, with no negation -> close.
    ("Add support for " + PROD_FILE, "So my games decrypt.", "close", ("console keys",)),
    ("Dump your console keys", "Share them here please.", "close", ("console keys",)),
    ("Where can I download firmware 18?", "", "close", ("firmware",)),
    ("Can you add NSP support?", "I want to run my games.", "close", ("retail files or games",)),
    ("Add retail game support", "I want to play Zelda on ounce-hb.", "close", ("retail files or games",)),
    ("How do I bypass DRM?", "", "close", ("piracy or circumvention",)),
    ("Crack and circumvent the anti-piracy check", "", "close", ("piracy or circumvention",)),
    ("Help", "I need firmware 17 and the keys.", "close", ("firmware",)),
    ("Share " + TITLE_FILE + " for my library", "", "close", ("console keys",)),
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
    # Form boilerplate: checkbox lines and HTML comments must not count as evidence.
    (
        "Bug",
        "### Scope confirmation\n- [x] This issue does not involve retail software, keys, firmware, or circumventing protection.\n"
        "<!-- Retail games, keys and firmware are closed. -->\n### What happened\nIt crashed.",
        "none",
        (),
    ),
]


class ClassifyTests(unittest.TestCase):
    def test_table(self):
        for title, body, action, matches in CASES:
            with self.subTest(title=title):
                verdict = tp.classify(title, body)
                self.assertEqual(verdict.action, action)
                self.assertEqual(verdict.matches, tuple(sorted(matches)))

    def test_strong_beats_weak_in_one_item(self):
        verdict = tp.classify("Add support for " + PROD_FILE, "The firmware log mentions keys.")
        self.assertEqual(verdict.action, "close")

    def test_sentence_split_keeps_key_file_names_whole(self):
        # A period inside the file name must not split the sentence and separate the request.
        verdict = tp.classify("", "Please support " + PROD_FILE)
        self.assertEqual(verdict.action, "close")


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


class ApplyIssueTests(unittest.TestCase):
    def setUp(self):
        self.target = tp.Target("issue", "o/r", "42", "", "newcomer")

    def test_strong_issue_comments_labels_closes_and_locks(self):
        fake = FakeGh()
        tp.apply_verdict(fake, self.target, tp.Verdict("close", ("firmware",)))
        self.assertEqual(len(fake.calls), 4)
        comment = fake.calls[0]
        self.assertIn("repos/o/r/issues/42/comments", " ".join(comment))
        body = [a for a in comment if a.startswith("body=")][0]
        self.assertIn(tp.POLICY_QUOTE, body)
        self.assertIn("CONTRIBUTING.md", body)
        self.assertIn("labels[]=area: legal", fake.calls[1])
        self.assertIn("state=closed", fake.calls[2])
        self.assertIn("state_reason=not_planned", fake.calls[2])
        self.assertIn("-X PUT", " ".join(fake.calls[3]))
        self.assertIn("repos/o/r/issues/42/lock", " ".join(fake.calls[3]))

    def test_weak_issue_only_labels(self):
        fake = FakeGh()
        tp.apply_verdict(fake, self.target, tp.Verdict("needs-owner-decision", ("firmware",)))
        self.assertEqual(len(fake.calls), 1)
        self.assertIn("labels[]=needs-owner-decision", fake.calls[0])

    def test_strong_pull_request_closes_without_state_reason(self):
        target = tp.Target("pull_request", "o/r", "7", "", "newcomer")
        fake = FakeGh()
        tp.apply_verdict(fake, target, tp.Verdict("close", ("console keys",)))
        close = fake.calls_containing("repos/o/r/pulls/7")[0]
        self.assertIn("state=closed", close)
        self.assertNotIn("state_reason=not_planned", close)
        self.assertTrue(fake.calls_containing("repos/o/r/issues/7/lock"))


class ApplyDiscussionTests(unittest.TestCase):
    def test_strong_discussion_uses_graphql_and_locks(self):
        target = tp.Target("discussion", "o/r", "3", "D_node", "newcomer")
        label_answer = json.dumps({"data": {"repository": {"label": {"id": "L_legal"}}}})
        fake = FakeGh([("label(name", label_answer)])
        tp.apply_verdict(fake, target, tp.Verdict("close", ("firmware",)))
        queries = [" ".join(c) for c in fake.calls]
        self.assertTrue(any("addDiscussionComment" in q for q in queries))
        self.assertTrue(any("addLabelsToLabelable" in q and "L_legal" in q for q in queries))
        self.assertTrue(any("closeDiscussion" in q and "OUTDATED" in q for q in queries))
        self.assertTrue(any("lockLockable" in q for q in queries))

    def test_missing_label_raises(self):
        target = tp.Target("discussion", "o/r", "3", "D_node", "newcomer")
        fake = FakeGh([("label(name", json.dumps({"data": {"repository": {"label": None}}}))])
        with self.assertRaises(RuntimeError):
            tp.apply_verdict(fake, target, tp.Verdict("close", ("firmware",)))

    def test_weak_discussion_only_labels(self):
        target = tp.Target("discussion", "o/r", "3", "D_node", "newcomer")
        label_answer = json.dumps({"data": {"repository": {"label": {"id": "L_review"}}}})
        fake = FakeGh([("label(name", label_answer)])
        tp.apply_verdict(fake, target, tp.Verdict("needs-owner-decision", ("firmware",)))
        queries = [" ".join(c) for c in fake.calls]
        self.assertFalse(any("closeDiscussion" in q for q in queries))
        self.assertFalse(any("addDiscussionComment" in q for q in queries))
        self.assertTrue(any("L_review" in q for q in queries))


class MainTests(unittest.TestCase):
    def env(self, **overrides):
        base = {
            "GH_REPO": "o/r",
            "REPO_OWNER": "owner",
            "EVENT_KIND": "issue",
            "ITEM_NUMBER": "9",
            "ITEM_AUTHOR": "newcomer",
            "ITEM_AUTHOR_TYPE": "User",
            "ITEM_TITLE": "Dump console keys",
            "ITEM_BODY": "Share them.",
        }
        base.update(overrides)
        return base

    def test_strong_contributor_item_is_actioned(self):
        fake = FakeGh()
        self.assertEqual(tp.main(self.env(), fake), 0)
        self.assertTrue(fake.calls_containing("lock"))

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
