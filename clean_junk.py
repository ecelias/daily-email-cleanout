#!/usr/bin/env python3
"""Delete matching messages from the Microsoft Outlook junk folder."""

from __future__ import annotations

import argparse
import json
import logging
import os
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

import msal
import requests

GRAPH_BASE = "https://graph.microsoft.com/v1.0"
SCOPES = ["Mail.ReadWrite", "User.Read"]
LOGGER = logging.getLogger("outlook-junk-cleaner")


def env_bool(name: str, default: bool) -> bool:
    value = os.getenv(name)
    if value is None:
        return default
    return value.strip().lower() in {"1", "true", "yes", "on"}


@dataclass(frozen=True)
class Settings:
    client_id: str
    tenant_id: str
    target_domains: tuple[str, ...]
    target_email_prefixes: tuple[str, ...]
    dry_run: bool
    token_cache_file: Path
    request_timeout: int
    max_retries: int

    @classmethod
    def from_env(cls, require_domains: bool = True) -> "Settings":
        client_id = os.getenv("MICROSOFT_CLIENT_ID", "").strip()
        if not client_id:
            raise ValueError("MICROSOFT_CLIENT_ID is required")

        raw_domains = os.getenv("TARGET_DOMAINS", "")
        domains = tuple(
            normalized
            for value in raw_domains.split(",")
            if (normalized := normalize_domain(value))
        )
        prefixes = tuple(
            normalized
            for value in os.getenv("TARGET_EMAIL_PREFIXES", "").split(",")
            if (normalized := normalize_email_prefix(value))
        )
        if require_domains and not domains and not prefixes:
            raise ValueError(
                "Set TARGET_DOMAINS or TARGET_EMAIL_PREFIXES "
                "(use comma-separated lists)"
            )

        return cls(
            client_id=client_id,
            tenant_id=os.getenv("MICROSOFT_TENANT_ID", "common").strip() or "common",
            target_domains=domains,
            target_email_prefixes=prefixes,
            dry_run=env_bool("DRY_RUN", True),
            token_cache_file=Path(
                os.getenv("TOKEN_CACHE_FILE", "/data/token_cache.json")
            ),
            request_timeout=int(os.getenv("REQUEST_TIMEOUT_SECONDS", "30")),
            max_retries=int(os.getenv("MAX_RETRIES", "4")),
        )


def normalize_domain(value: str) -> str:
    return value.strip().lower().lstrip("@").strip(".")


def normalize_email_prefix(value: str) -> str:
    return value.strip().lower()


def sender_domain(email_address: str | None) -> str | None:
    if not email_address or "@" not in email_address:
        return None
    return normalize_domain(email_address.rsplit("@", 1)[1])


def domain_matches(domain: str | None, targets: Iterable[str]) -> bool:
    if not domain:
        return False
    return any(domain == target or domain.endswith(f".{target}") for target in targets)


def email_prefix_matches(
    email_address: str | None, prefixes: Iterable[str]
) -> bool:
    if not email_address:
        return False
    normalized = email_address.strip().lower()
    return any(normalized.startswith(prefix) for prefix in prefixes)


class AuthenticationRequired(RuntimeError):
    pass


class GraphClient:
    def __init__(self, settings: Settings) -> None:
        self.settings = settings
        self.cache = msal.SerializableTokenCache()
        self._load_cache()
        self.app = msal.PublicClientApplication(
            client_id=settings.client_id,
            authority=(
                "https://login.microsoftonline.com/"
                f"{settings.tenant_id}"
            ),
            token_cache=self.cache,
        )

    def _load_cache(self) -> None:
        if not self.settings.token_cache_file.exists():
            return
        self.cache.deserialize(self.settings.token_cache_file.read_text())

    def _save_cache(self) -> None:
        if not self.cache.has_state_changed:
            return
        path = self.settings.token_cache_file
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(self.cache.serialize())
        path.chmod(0o600)

    def authenticate_device_code(self) -> None:
        flow = self.app.initiate_device_flow(scopes=SCOPES)
        if "user_code" not in flow:
            raise RuntimeError(f"Could not start device login: {json.dumps(flow)}")

        print(flow["message"], flush=True)
        result = self.app.acquire_token_by_device_flow(flow)
        self._save_cache()
        self._access_token_or_raise(result)
        LOGGER.info("Microsoft authorization completed and token cache saved.")

    def access_token(self) -> str:
        accounts = self.app.get_accounts()
        if not accounts:
            raise AuthenticationRequired(
                "No cached Microsoft login was found. Run the authenticate command."
            )

        result = self.app.acquire_token_silent(SCOPES, account=accounts[0])
        self._save_cache()
        if not result:
            raise AuthenticationRequired(
                "The cached Microsoft login could not be refreshed. "
                "Run the authenticate command again."
            )
        return self._access_token_or_raise(result)

    @staticmethod
    def _access_token_or_raise(result: dict[str, Any]) -> str:
        token = result.get("access_token")
        if token:
            return str(token)
        description = result.get("error_description", result)
        raise RuntimeError(f"Microsoft authentication failed: {description}")

    def request(
        self, method: str, url: str, *, expected_status: int = 200
    ) -> requests.Response:
        headers = {
            "Authorization": f"Bearer {self.access_token()}",
            "Accept": "application/json",
        }
        for attempt in range(self.settings.max_retries + 1):
            response = requests.request(
                method,
                url,
                headers=headers,
                timeout=self.settings.request_timeout,
            )
            if response.status_code == expected_status:
                return response

            retryable = response.status_code == 429 or response.status_code >= 500
            if retryable and attempt < self.settings.max_retries:
                delay = int(response.headers.get("Retry-After", 2**attempt))
                LOGGER.warning(
                    "Graph returned HTTP %s; retrying in %s second(s).",
                    response.status_code,
                    delay,
                )
                time.sleep(delay)
                continue

            response.raise_for_status()

        raise RuntimeError("Microsoft Graph request exhausted its retries")

    def junk_messages(self) -> Iterable[dict[str, Any]]:
        url: str | None = (
            f"{GRAPH_BASE}/me/mailFolders/junkemail/messages"
            "?$select=id,subject,from,receivedDateTime&$top=100"
        )
        while url:
            payload = self.request("GET", url).json()
            yield from payload.get("value", [])
            url = payload.get("@odata.nextLink")

    def delete_message(self, message_id: str) -> None:
        self.request(
            "DELETE",
            f"{GRAPH_BASE}/me/messages/{message_id}",
            expected_status=204,
        )


def clean(settings: Settings) -> int:
    client = GraphClient(settings)
    scanned = matched = deleted = failures = 0

    for message in client.junk_messages():
        scanned += 1
        address = (
            message.get("from", {}).get("emailAddress", {}).get("address")
        )
        matches_domain = domain_matches(
            sender_domain(address), settings.target_domains
        )
        matches_prefix = email_prefix_matches(
            address, settings.target_email_prefixes
        )
        if not matches_domain and not matches_prefix:
            continue

        matched += 1
        subject = message.get("subject") or "(no subject)"
        received = message.get("receivedDateTime") or "unknown date"
        LOGGER.info("MATCH sender=%s received=%s subject=%r", address, received, subject)

        if settings.dry_run:
            continue

        try:
            client.delete_message(message["id"])
            deleted += 1
            LOGGER.info("Moved message to Deleted Items.")
        except requests.RequestException:
            failures += 1
            LOGGER.exception("Could not delete message id=%s", message.get("id"))

    LOGGER.info(
        "Run complete: scanned=%s matched=%s deleted=%s failures=%s mode=%s",
        scanned,
        matched,
        deleted,
        failures,
        "DRY_RUN" if settings.dry_run else "DELETE",
    )
    return 1 if failures else 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command",
        choices=("clean", "authenticate"),
        nargs="?",
        default="clean",
    )
    return parser.parse_args()


def main() -> int:
    logging.basicConfig(
        level=os.getenv("LOG_LEVEL", "INFO").upper(),
        format="%(asctime)s %(levelname)s %(message)s",
    )
    args = parse_args()
    try:
        settings = Settings.from_env(require_domains=args.command == "clean")
        client = GraphClient(settings)
        if args.command == "authenticate":
            client.authenticate_device_code()
            return 0
        return clean(settings)
    except (ValueError, AuthenticationRequired, RuntimeError) as exc:
        LOGGER.error("%s", exc)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
