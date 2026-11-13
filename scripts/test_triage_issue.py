"""Offline tests for scripts/triage_issue.py: the missing-details heuristics and the comment."""
import unittest

from fake_gh import FakeGh
import triage_issue as ti

PROD_FILE = "prod" + ".keys"

FORM_COMPLETE = """### Homebrew program
my-app.nro from https://github.com/someone/my-app

### Version
v1.2.0 at commit 9f8e7d6c5b4a

### Exact command and observed output
```sh
cargo run -- run my-app.nro
```
exit code 1, stderr: relocation failed

### What happened, and what did you expect?
It crashed; I expected it to run.
"""

FORM_EMPTY = """### Homebrew program
_No response_

### Version
_No response_

### Exact command and observed output
_No response_

### What happened, and what did you expect?
_No response_
"""


class MissingDetailsTests(unittest.TestCase):
    def test_complete_form_has_nothing_missing(self):
        self.assertEqual(ti.missing_details("Relocation crash", FORM_COMPLETE), [])

    def test_capitalised_no_response_counts_as_missing(self):
        self.assertEqual(
            ti.missing_details("Crash", FORM_EMPTY),
            ["program", "version", "command", "output"],
        )

    def test_word_app_alone_is_not_a_program(self):
        # Previously the substring 'app' in the combined text counted as a program.
        missing = ti.missing_details("The app crashes", "It crashed on start.")
        self.assertIn("program", missing)

    def test_nro_filename_is_a_program(self):
        missing = ti.missing_details("Crash", "running homebrew.nro fails")
        self.assertNotIn("program", missing)

    def test_word_version_alone_is_not_a_version(self):
        # Previously the word 'version' alone counted as a version.
        missing = ti.missing_details("Which version should I use?", "")
        self.assertIn("version", missing)

    def test_version_number_is_a_version(self):
        self.assertNotIn("version", ti.missing_details("Crash", "built from v1.2.0"))
        self.assertNotIn("version", ti.missing_details("Crash", "version: 1.2"))

    def test_hex_word_without_digits_is_not_a_commit(self):
        self.assertIn("version", ti.missing_details("Crash", "the feedface facade is wrong"))

    def test_hex_commit_hash_is_a_version(self):
        self.assertNotIn("version", ti.missing_details("Crash", "built at 9f8e7d6c5b"))

    def test_bare_ounce_hb_is_not_a_command(self):
        self.assertIn("command", ti.missing_details("ounce-hb crashes", "It is broken"))

    def test_cargo_run_is_a_command(self):
        self.assertNotIn("command", ti.missing_details("Crash", "I ran cargo run -- run a.nro"))

    def test_word_failed_alone_is_not_output(self):
        self.assertIn("output", ti.missing_details("Crash", "The build failed for me."))

    def test_exit_code_is_output(self):
        self.assertNotIn("output", ti.missing_details("Crash", "exit code 1"))


class CommentTests(unittest.TestCase):
    def test_missing_lists_each_missing_key(self):
        text = ti.build_comment("alice", ["version"], [])
        self.assertIn(ti.TRIAGE_MARKER, text)
        self.assertIn("**Version**", text)
        self.assertNotIn("**Homebrew Program**", text)

    def test_duplicates_are_listed_by_number(self):
        text = ti.build_comment("alice", [], [(4, "Relocation crash on start")])
        self.assertIn("- #4", text)
        self.assertNotIn("Relocation crash on start", text)

    def test_other_titles_are_not_echoed(self):
        hostile = "@victim1 @victim2 ![](http://x.example/track.png)"
        text = ti.build_comment("alice", [], [(4, hostile)])
        self.assertNotIn("@victim1", text)
        self.assertNotIn("![]", text)
        self.assertNotIn("http://x.example", text)


class FindDuplicatesTests(unittest.TestCase):
    def test_pull_requests_and_self_are_ignored(self):
        items = [
            {"number": 1, "title": "Relocation crash on start"},
            {"number": 2, "title": "Relocation crash", "pull_request": {}},
            {"number": 3, "title": "Unrelated thing entirely"},
        ]
        found = ti.find_duplicates(items, "Relocation crash on start", "1")
        self.assertEqual(found, [])
        found = ti.find_duplicates(items, "Relocation crash on start", "9")
        self.assertEqual(found, [(1, "Relocation crash on start")])


class MainTests(unittest.TestCase):
    def env(self, **overrides):
        base = {
            "GH_REPO": "o/r",
            "ISSUE_NUMBER": "5",
            "ISSUE_TITLE": "Relocation crash",
            "ISSUE_BODY": "exit code 1",
            "ISSUE_AUTHOR": "alice",
        }
        base.update(overrides)
        return base

    def test_posts_triage_comment(self):
        fake = FakeGh([("issues?state=open", "[]")])
        self.assertEqual(ti.main(self.env(), fake), 0)
        posted = fake.calls_containing("-X POST repos/o/r/issues/5/comments")
        self.assertEqual(len(posted), 1)

    def test_policy_close_skips_triage_comment(self):
        fake = FakeGh()
        env = self.env(ISSUE_TITLE="Dump console keys", ISSUE_BODY="Share them.")
        self.assertEqual(ti.main(env, fake), 0)
        self.assertEqual(fake.calls, [])

    def test_ambiguous_issue_still_gets_triage_comment(self):
        fake = FakeGh([("issues?state=open", "[]")])
        env = self.env(ISSUE_TITLE="ounce-hb should refuse " + PROD_FILE + " files", ISSUE_BODY="")
        self.assertEqual(ti.main(env, fake), 0)
        self.assertTrue(fake.calls_containing("comments"))

    def test_duplicate_search_failure_still_posts(self):
        from gh_client import GhError

        fake = FakeGh([("issues?state=open", GhError("rate limited"))])
        self.assertEqual(ti.main(self.env(), fake), 0)
        self.assertTrue(fake.calls_containing("comments"))

    def test_post_failure_returns_nonzero(self):
        from gh_client import GhError

        fake = FakeGh([("issues?state=open", "[]"), ("-X POST", GhError("HTTP 500"))])
        self.assertEqual(ti.main(self.env(), fake), 1)


if __name__ == "__main__":
    unittest.main()
