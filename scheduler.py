#!/usr/bin/env python3
"""Run the Outlook junk cleaner once per day inside a long-running container."""

from __future__ import annotations

import logging
import os
import signal
import subprocess
import sys
import time
from datetime import datetime, timedelta
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

LOGGER = logging.getLogger("outlook-junk-scheduler")
STOP_REQUESTED = False


def stop(_signum: int, _frame: object) -> None:
    global STOP_REQUESTED
    STOP_REQUESTED = True


def parse_run_time(value: str) -> tuple[int, int]:
    try:
        hour_text, minute_text = value.split(":", 1)
        hour, minute = int(hour_text), int(minute_text)
    except (ValueError, AttributeError) as exc:
        raise ValueError("RUN_AT must use 24-hour HH:MM format") from exc
    if not 0 <= hour <= 23 or not 0 <= minute <= 59:
        raise ValueError("RUN_AT must be a valid 24-hour time")
    return hour, minute


def next_run(now: datetime, hour: int, minute: int) -> datetime:
    candidate = now.replace(hour=hour, minute=minute, second=0, microsecond=0)
    if candidate <= now:
        candidate += timedelta(days=1)
    return candidate


def run_cleaner() -> int:
    LOGGER.info("Starting scheduled junk-folder cleanup.")
    result = subprocess.run(
        [sys.executable, "/app/clean_junk.py", "clean"],
        check=False,
    )
    if result.returncode:
        LOGGER.error("Cleanup exited with status %s.", result.returncode)
    else:
        LOGGER.info("Cleanup finished successfully.")
    return result.returncode


def main() -> int:
    logging.basicConfig(
        level=os.getenv("LOG_LEVEL", "INFO").upper(),
        format="%(asctime)s %(levelname)s %(message)s",
    )
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)

    hour, minute = parse_run_time(os.getenv("RUN_AT", "08:00"))
    timezone_name = os.getenv("TZ", "America/Chicago")
    try:
        timezone = ZoneInfo(timezone_name)
    except ZoneInfoNotFoundError:
        LOGGER.error("Unknown TZ value: %s", timezone_name)
        return 2

    if os.getenv("RUN_ON_STARTUP", "true").lower() in {"1", "true", "yes", "on"}:
        run_cleaner()

    while not STOP_REQUESTED:
        now = datetime.now(timezone)
        scheduled = next_run(now, hour, minute)
        LOGGER.info("Next cleanup: %s", scheduled.isoformat())
        remaining = (scheduled - now).total_seconds()
        while remaining > 0 and not STOP_REQUESTED:
            time.sleep(min(remaining, 60))
            remaining = (scheduled - datetime.now(timezone)).total_seconds()
        if not STOP_REQUESTED:
            run_cleaner()

    LOGGER.info("Scheduler stopped.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
