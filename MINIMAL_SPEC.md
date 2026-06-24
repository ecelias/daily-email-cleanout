# Minimal System Specification

## Purpose

This system removes a specific recurring class of spam from one Microsoft
Outlook or Microsoft 365 mailbox.

The spam uses continually changing sender domains, so conventional
domain-blocking is ineffective. The stable identifying characteristic is that
the sender's full email address begins with:

```text
info@-----
```

The system must inspect the mailbox's Junk Email folder and move every matching
message to Deleted Items.

This document defines the baseline behavior independently of Docker, Python, or
any future desktop application framework. Packaging and user experience may
change, but the intent and safety rules below should remain intact.

## Core Behavior

On each cleanup run, the system must:

1. Authenticate the configured Microsoft personal account.
2. Access the account's standard Junk Email folder through Microsoft Graph.
3. Retrieve every message currently in that folder, following all pagination.
4. Match messages using the sender's full email address.
5. Create a stable snapshot of every matching message before deleting anything.
6. Move each matched message to Deleted Items using Microsoft Graph's normal
   message-delete operation.
7. Report the number of messages scanned, matched, deleted, and failed.

The system must not permanently purge messages.

## Matching Rule

The only required spam rule is a case-insensitive prefix match against the
sender's complete email address:

```text
info@-----
```

Examples that must match:

```text
info@-----mail.FnopEKGKj0M9cG.com
INFO@-----mail.example.com
info@-----anything.invalid
```

Examples that must not match:

```text
sales@-----mail.example.com
myinfo@-----mail.example.com
info@example.com
```

The baseline system does not match by domain, subject, display name, message
body, substring, regular expression, or approximate similarity.

If configuration remains user-editable, the relevant setting is:

```dotenv
TARGET_EMAIL_PREFIXES=info@-----
```

The cleaner must refuse to run if no email prefix is configured. It must not
silently broaden the rule.

## Full-Backlog Requirement

Every run must inspect the entire current Junk Email folder, not only messages
received today or since the previous run.

This ensures that:

- The first run can clean the existing backlog.
- Messages are not missed when the application is stopped for several days.
- A later run can recover messages missed by a prior interrupted or failed run.
- No local last-run timestamp is required for correctness.

The implementation must follow every Microsoft Graph `@odata.nextLink` until no
additional page remains.

## Stable-Snapshot Requirement

The system must finish retrieving and recording all matching message IDs before
it begins deleting messages.

Deleting while paginating is prohibited because changing the folder during
pagination can shift results and cause matching messages to be skipped.

The required sequence is:

```text
fetch all pages -> collect all matches -> delete collected matches
```

## Safety Behavior

The system must support a dry-run mode.

In dry-run mode, it must:

- Perform the same complete folder scan.
- Apply the same matching rule.
- Log or display every match.
- Report final counts.
- Delete nothing.

Dry-run mode should be enabled by default for a new installation. A user must
deliberately enable deletion after reviewing matches.

A failure to delete one message must not prevent attempts to delete the other
messages in the stable snapshot. Individual failures must be counted and
reported.

## Authentication

The system uses delegated Microsoft Graph access for the mailbox owner. It does
not use browser scraping, Selenium, stored mailbox passwords, or an application
secret.

Required delegated permissions:

```text
Mail.ReadWrite
User.Read
offline_access
```

For the current personal Microsoft account registration:

```text
MICROSOFT_TENANT_ID=consumers
```

The Microsoft Entra application registration must:

- Support personal Microsoft accounts.
- Be configured as a Mobile and desktop application.
- Include the `http://localhost` redirect URI.
- Have public client flows enabled.

Initial authorization may require an interactive device-code login. Subsequent
runs should use a securely persisted token cache and refresh access silently.
The token cache must not be committed to source control.

If silent authentication stops working, the system must fail safely and request
reauthorization rather than deleting without confirmed mailbox access.

## Microsoft Graph Operations

The logical Graph operations are:

```text
GET /me/mailFolders/junkemail/messages
DELETE /me/messages/{message-id}
```

Message retrieval must include at least:

```text
id
from
subject
receivedDateTime
```

The locale-independent well-known folder name `junkemail` must be used rather
than relying on the folder's displayed language-specific name.

Graph throttling and temporary server failures should be retried with bounded
backoff. Authentication errors and permanent request errors must be surfaced.

## Scheduling and Recovery

The current service runs:

- Once when the service starts.
- Once daily at a configured local time and timezone.

A future local executable may use a different scheduling mechanism, but it must
preserve these behavioral goals:

- The user can configure a daily cleanup time.
- The application can run without daily manual interaction after initial
  authorization.
- Starting the application triggers a catch-up cleanup unless the user
  explicitly disables that behavior.
- Missed scheduled runs do not lose messages because the next run scans the
  complete Junk Email backlog.

The scheduler must not start a second cleanup while one is already running.

## Logging and User Feedback

Each cleanup must make the following visible:

- Cleanup start.
- Whether the run is dry-run or delete mode.
- Confirmation that the entire Junk Email backlog is being scanned.
- Each matched sender address, received timestamp, and subject.
- Completion of the scan before deletion begins.
- Each successful move to Deleted Items.
- Each deletion failure.
- Final counts for scanned, matched, deleted, and failed messages.

Logs must not expose access tokens, refresh tokens, the token-cache contents, or
other authentication secrets.

## Required Configuration

The conceptual configuration is:

```dotenv
MICROSOFT_CLIENT_ID=<registered application client ID>
MICROSOFT_TENANT_ID=consumers
TARGET_EMAIL_PREFIXES=info@-----
DRY_RUN=true
RUN_AT=08:00
TZ=America/Chicago
RUN_ON_STARTUP=true
```

A local executable may store these values in application preferences instead of
an environment file. Their meaning must remain equivalent.

## Non-Goals

The baseline system does not:

- Clean Inbox, Deleted Items, Archive, or other folders.
- Delete mail from senders that do not begin with the configured prefix.
- Permanently purge messages.
- Block future delivery at the mail-server level.
- Unsubscribe from mailing lists.
- Analyze message content with AI.
- Maintain a local copy of mailbox messages.
- Depend on a web browser remaining signed in.
- Require Docker as part of its core behavior.

## Acceptance Criteria

A conforming implementation must pass these behavioral checks:

1. `info@-----mail.random.example` is matched regardless of letter case.
2. `sales@-----mail.random.example` is not matched.
3. `myinfo@-----mail.random.example` is not matched.
4. Messages from previous days are scanned and matched.
5. More than one Graph page of messages is processed.
6. No deletion begins until all pages have been scanned.
7. Dry-run mode produces matches and counts but performs no deletes.
8. Delete mode moves every snapshotted match to Deleted Items.
9. One deletion failure does not stop the remaining deletion attempts.
10. A final summary reports scanned, matched, deleted, and failed counts.
11. Missing match configuration causes a safe failure.
12. Missing or expired authorization causes a safe failure and requests
    reauthorization.
13. Authentication credentials and token data are never written to logs.

## Core Invariant

The central invariant is:

> Scan the complete Outlook Junk Email backlog, identify only senders whose
> full address begins with `info@-----`, snapshot all matches, and safely move
> those matches to Deleted Items.

Any future local executable should be considered compatible only if it preserves
that invariant.
