import unittest

from scripts.retention_continuous_releases import deletion_candidates


class RetentionContinuousReleasesTests(unittest.TestCase):
    def test_keeps_recent_monthly_and_protected_releases(self):
        releases = [
            {
                "tagName": f"continuous-2026{month:02d}01-0000-old",
                "isPrerelease": True,
                "createdAt": f"2026-{month:02d}-01T00:00:00Z",
            }
            for month in range(1, 7)
        ]
        releases.append(
            {
                "tagName": "continuous-20251201-0000-protected",
                "isPrerelease": True,
                "createdAt": "2025-12-01T00:00:00Z",
            }
        )

        actual = deletion_candidates(
            releases,
            keep_recent=2,
            keep_monthly=2,
            protected_tags={"continuous-20251201-0000-protected"},
        )

        self.assertIn("continuous-20260201-0000-old", actual)
        self.assertNotIn("continuous-20260601-0000-old", actual)
        self.assertNotIn("continuous-20251201-0000-protected", actual)


if __name__ == "__main__":
    unittest.main()
