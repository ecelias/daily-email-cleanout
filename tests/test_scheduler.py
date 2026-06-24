import unittest
from datetime import datetime
from zoneinfo import ZoneInfo

from scheduler import next_run, parse_run_time


class SchedulerTests(unittest.TestCase):
    def test_parse_run_time(self) -> None:
        self.assertEqual(parse_run_time("08:05"), (8, 5))

    def test_invalid_run_time(self) -> None:
        for value in ("8", "25:00", "08:61", "nope"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                parse_run_time(value)

    def test_next_run_today_or_tomorrow(self) -> None:
        tz = ZoneInfo("America/Chicago")
        morning = datetime(2026, 6, 23, 7, 0, tzinfo=tz)
        evening = datetime(2026, 6, 23, 9, 0, tzinfo=tz)

        self.assertEqual(next_run(morning, 8, 0).day, 23)
        self.assertEqual(next_run(evening, 8, 0).day, 24)


if __name__ == "__main__":
    unittest.main()
