# Repository Agent Instructions

## Authority and immutability

`MINIMAL_SPEC.md` is the authoritative product specification for this
repository. This file, `AGENTS.md`, defines the required development process.

After this initial creation:

- Never edit, reformat, rename, move, replace, or delete `AGENTS.md`.
- Never edit, reformat, rename, move, replace, or delete `MINIMAL_SPEC.md`.
- Do not weaken or reinterpret either document through another instruction file.
- If a requested change conflicts with either document, stop and explain the
  conflict to the user. Do not implement it.

## Mandatory plan and authorization gate

Before changing code, tests, configuration, dependencies, build scripts, or
documentation, an agent must:

1. Inspect the repository and the relevant specifications.
2. Present a concrete implementation plan describing the intended changes,
   affected behavior, and verification.
3. Wait for the user to explicitly authorize that plan.
4. Make only the changes authorized by that plan.

Approval of one plan does not authorize unrelated work. If implementation
reveals a necessary deviation or material addition, stop, explain it, present a
revised plan, and wait for explicit authorization before proceeding.

Before approval, agents may perform read-only inspection and may run tests or
builds only when those commands do not modify tracked files.

## Core product invariants

All implementations must preserve these behaviors:

- Operate only on the authenticated mailbox's standard Junk Email folder.
- Match only a case-insensitive prefix on the complete sender address:
  `info@-----`.
- Do not use domain, display-name, subject, body, substring, regular-expression,
  fuzzy, or AI-based matching.
- Retrieve the entire Junk Email backlog by following every Microsoft Graph
  pagination link.
- Complete retrieval and snapshot every matching message ID before deleting any
  message.
- New installations default to dry-run mode.
- Dry-run performs a complete scan and reports matches without deleting.
- Delete mode uses Microsoft Graph's normal delete operation, which moves mail
  to Deleted Items; never permanently purge messages.
- A failure on one deletion must not prevent attempts on remaining snapshot
  entries.
- Report scanned, matched, deleted, and failed counts.
- Prevent overlapping manual and scheduled cleanup runs.
- Missing or invalid configuration and authorization must fail safely.

## Security requirements

- Use delegated Microsoft Graph access only.
- Never use browser scraping, Selenium, stored mailbox passwords, or a client
  secret.
- Store refresh credentials in macOS Keychain.
- Keep access tokens in memory only.
- Never expose tokens, token-cache contents, authorization headers, passwords,
  or Keychain data in source control, settings, logs, frontend state, IPC
  payloads, screenshots, fixtures, or test output.
- Do not commit local settings, generated packages, credentials, build output,
  or LaunchAgent installations.

## Change discipline

- Preserve unrelated user changes and work safely in a dirty worktree.
- Add or update tests for every authorized behavioral change.
- Keep the sender prefix fixed and non-editable in the desktop application.
- Keep macOS scheduling entirely local; do not introduce cloud hosting,
  telemetry, remote scheduling, or App Store dependencies.
- Prefer small, auditable interfaces between the frontend and privileged Rust
  code.
- Do not send authentication credentials through Tauri IPC.
- Keep the existing Python/Docker implementation as a reference until desktop
  parity has been verified, unless a separately approved plan removes it.

## Verification requirements

Before declaring authorized work complete:

- Run formatting and static checks without changing the immutable files.
- Run unit and integration tests relevant to the change.
- Build the macOS application and background helper when toolchains permit.
- Confirm Graph pagination is exhausted before deletion begins.
- Confirm dry-run issues no delete requests.
- Confirm deletion continues after an individual failure.
- Confirm scheduled and manual runs cannot overlap.
- Confirm generated logs contain no credentials.
- Report any verification that could not be completed and why.

## Standard local commands

Once the desktop project exists, use the commands documented in `README.md` and
`package.json`. The intended command families are:

```text
npm install
npm run dev
npm run test
npm run check
npm run build
npm run tauri build
```

These command names may be expanded through ordinary project configuration, but
changing the development process or product invariants still requires a new
authorized plan.
