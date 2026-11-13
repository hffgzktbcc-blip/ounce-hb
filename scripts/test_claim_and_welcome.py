"""Offline tests for scripts/claim_issue.py and scripts/welcome_contributor.py."""
import json
import unittest

from fake_gh import FakeGh
import claim_issue as ci
import welcome_contributor as wc


def claim_env(**overrides):
    base = {
        "GH_REPO": "o/r",
        "ISSUE_NUMBER": "12",
        "ISSUE_STATE": "open",
        "ISSUE_ASSIGNEES": "[]",
        "ISSUE_LABELS": json.dumps([{"name": "good first issue"}]),
        "COMMENT_BODY": "/claim",
        "COMMENT_AUTHOR": "newcomer",
    }
    base.update(overrides)
    return base


class ClaimTests(unittest.TestCase):
    def test_phrase_matching_ignores_case_and_punctuation(self):
        self.assertTrue(ci.is_claim("  I'll take this!  "))
        self.assertTrue(ci.is_claim("thanks\n/claim\n"))
        self.assertFalse(ci.is_claim("I think this is a claim about the loader"))

    def test_non_claim_is_skipped(self):
        fake = FakeGh()
        self.assertEqual(ci.main(claim_env(COMMENT_BODY="nice work"), fake), 0)
        self.assertEqual(fake.calls, [])

    def test_closed_issue_is_skipped(self):
        fake = FakeGh()
        self.assertEqual(ci.main(claim_env(ISSUE_STATE="closed"), fake), 0)
        self.assertEqual(fake.calls, [])

    def test_good_first_issue_is_assigned_and_confirmed(self):
        fake = FakeGh()
        self.assertEqual(ci.main(claim_env(), fake), 0)
        self.assertTrue(fake.calls_containing("repos/o/r/issues/12/assignees"))
        self.assertTrue(fake.calls_containing("assignees[]=newcomer"))
        self.assertTrue(fake.calls_containing("Assigned to @newcomer"))

    def test_other_issue_gets_maintainer_reply_not_assignment(self):
        fake = FakeGh()
        env = claim_env(ISSUE_LABELS=json.dumps([{"name": "bug"}]))
        self.assertEqual(ci.main(env, fake), 0)
        self.assertFalse(fake.calls_containing("assignees"))
        self.assertTrue(fake.calls_containing("a maintainer will coordinate"))

    def test_already_assigned_to_someone_else_is_reported(self):
        fake = FakeGh()
        env = claim_env(ISSUE_ASSIGNEES=json.dumps([{"login": "other"}]))
        self.assertEqual(ci.main(env, fake), 0)
        self.assertTrue(fake.calls_containing("already assigned to @other"))

    def test_assignment_failure_returns_nonzero(self):
        from gh_client import GhError

        fake = FakeGh([("assignees", GhError("HTTP 422"))])
        self.assertEqual(ci.main(claim_env(), fake), 1)


class WelcomeTests(unittest.TestCase):
    def env(self, **overrides):
        base = {
            "GH_REPO": "o/r",
            "ISSUE_NUMBER": "30",
            "AUTHOR_LOGIN": "newcomer",
            "REPO_OWNER": "owner",
            "IS_PR": "false",
        }
        base.update(overrides)
        return base

    def test_owner_and_bots_are_skipped(self):
        for author in ("owner", "OWNER", "renovate[bot]"):
            fake = FakeGh()
            self.assertEqual(wc.main(self.env(AUTHOR_LOGIN=author), fake), 0)
            self.assertEqual(fake.calls, [])

    def test_prior_contributor_is_skipped(self):
        fake = FakeGh([("issues?creator", json.dumps([{"number": 2}, {"number": 30}]))])
        self.assertEqual(wc.main(self.env(), fake), 0)
        self.assertFalse(fake.calls_containing("-X POST"))

    def test_first_timer_gets_welcome_issue_text(self):
        fake = FakeGh([("issues?creator", json.dumps([{"number": 30}]))])
        self.assertEqual(wc.main(self.env(), fake), 0)
        post = fake.calls_containing("-X POST")[0]
        body = [a for a in post if a.startswith("body=")][0]
        self.assertIn("first issue", body)
        self.assertNotIn("if generated with an AI assistant", body)

    def test_first_timer_pull_request_gets_policy_line(self):
        fake = FakeGh([("issues?creator", "[]")])
        self.assertEqual(wc.main(self.env(IS_PR="true"), fake), 0)
        body = [a for a in fake.calls_containing("-X POST")[0] if a.startswith("body=")][0]
        self.assertIn("first pull request", body)
        self.assertIn("if generated with an AI assistant", body)

    def test_existing_marker_is_not_repeated(self):
        fake = FakeGh(
            [
                ("issues?creator", "[]"),
                ("comments", json.dumps([{"body": wc.WELCOME_MARKER}])),
            ]
        )
        self.assertEqual(wc.main(self.env(), fake), 0)
        self.assertFalse(fake.calls_containing("-X POST"))

    def test_lookup_failure_skips_quietly(self):
        from gh_client import GhError

        fake = FakeGh([("issues?creator", GhError("HTTP 503"))])
        self.assertEqual(wc.main(self.env(), fake), 0)
        self.assertFalse(fake.calls_containing("-X POST"))


if __name__ == "__main__":
    unittest.main()
