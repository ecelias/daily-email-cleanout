import unittest
from pathlib import Path
from unittest.mock import patch

from clean_junk import (
    Settings,
    clean,
    email_prefix_matches,
)


class EmailPrefixMatchingTests(unittest.TestCase):
    def test_email_prefix_matching(self) -> None:
        prefixes = ("info@-----",)
        self.assertTrue(
            email_prefix_matches(
                "info@-----mail.FnopEKGKj0M9cG.com",
                prefixes,
            )
        )
        self.assertTrue(
            email_prefix_matches(
                "INFO@-----mail.example.com",
                prefixes,
            )
        )
        self.assertFalse(
            email_prefix_matches("sales@-----mail.example.com", prefixes)
        )
        self.assertFalse(
            email_prefix_matches("myinfo@-----mail.example.com", prefixes)
        )


class CleanupTests(unittest.TestCase):
    def test_collects_all_matches_before_deleting(self) -> None:
        events: list[str] = []

        class FakeGraphClient:
            def __init__(self, _settings: Settings) -> None:
                pass

            def junk_messages(self):
                events.append("scan-1")
                yield {
                    "id": "first",
                    "from": {
                        "emailAddress": {
                            "address": "info@-----mail.first.example"
                        }
                    },
                }
                events.append("scan-2")
                yield {
                    "id": "second",
                    "from": {
                        "emailAddress": {
                            "address": "info@-----mail.second.example"
                        }
                    },
                }
                events.append("scan-complete")

            def delete_message(self, message_id: str) -> None:
                events.append(f"delete-{message_id}")

        settings = Settings(
            client_id="client",
            tenant_id="consumers",
            target_email_prefixes=("info@-----",),
            dry_run=False,
            token_cache_file=Path("/tmp/not-used"),
            request_timeout=30,
            max_retries=0,
        )

        with patch("clean_junk.GraphClient", FakeGraphClient):
            self.assertEqual(clean(settings), 0)

        self.assertEqual(
            events,
            [
                "scan-1",
                "scan-2",
                "scan-complete",
                "delete-first",
                "delete-second",
            ],
        )


if __name__ == "__main__":
    unittest.main()
