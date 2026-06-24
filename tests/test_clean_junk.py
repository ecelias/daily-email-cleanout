import unittest

from clean_junk import (
    domain_matches,
    email_prefix_matches,
    normalize_domain,
    sender_domain,
)


class DomainMatchingTests(unittest.TestCase):
    def test_normalize_domain(self) -> None:
        self.assertEqual(
            normalize_domain(" @News.Example.COM. "),
            "news.example.com",
        )

    def test_sender_domain(self) -> None:
        self.assertEqual(
            sender_domain("Person@News.Example.com"),
            "news.example.com",
        )
        self.assertIsNone(sender_domain(None))
        self.assertIsNone(sender_domain("not-an-email"))

    def test_domain_matching_is_boundary_safe(self) -> None:
        targets = ("example.com",)
        self.assertTrue(domain_matches("example.com", targets))
        self.assertTrue(domain_matches("news.example.com", targets))
        self.assertFalse(domain_matches("notexample.com", targets))
        self.assertFalse(domain_matches("example.com.evil.test", targets))

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


if __name__ == "__main__":
    unittest.main()
