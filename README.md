# Daily Outlook Junk Cleaner

Problem: An email address of mine got added to a spam email service that would send me multiple emails a day with inconsistent email domains, making it impossible to unsubscribe or block them. However, all email domains start with `@-----` followed by a random string of letters and numbers.

A Docker service that checks the Outlook/Microsoft 365 Junk Email folder every
day and moves messages whose sender begins with `info@-----` to Deleted Items.
It uses Microsoft Graph rather than browser automation.

Each run checks the entire current Junk Email folder, not only messages received
that day. If Docker is stopped for several days—or this is the first run—the
next cleanup processes all matching older messages still in Junk Email. Graph
pagination ensures the cleaner continues beyond the first 100 messages.

The cleaner completes the full scan before deleting anything. This stable
snapshot prevents deletions from shifting Graph's paginated results and causing
later matching messages to be skipped.

## 1. Register a Microsoft application

In Microsoft Entra, create an app registration that supports public client
flows. Add these **delegated** Microsoft Graph permissions:

- `Mail.ReadWrite`
- `User.Read`
- `offline_access`

Under **Authentication**:

1. Select **Add a platform**.
2. Select **Mobile and desktop applications**.
3. Select the `http://localhost` redirect URI and configure it.
4. Under **Advanced settings**, set **Allow public client flows** to **Yes**.
5. Save the registration.

Copy the Application (client) ID. Microsoft can take a few minutes to apply
authentication-setting changes.

For a personal Outlook/Hotmail mailbox, the registration must support personal
Microsoft accounts. A work or school tenant may require administrator approval.

## 2. Configure the service

```bash
cp .env.example .env
```

Edit `.env`:

- Set `MICROSOFT_CLIENT_ID`.
- For a personal-account-only registration, leave
  `MICROSOFT_TENANT_ID=consumers`.
- Leave `TARGET_EMAIL_PREFIXES=info@-----`.
- Leave `DRY_RUN=true` until you have checked the logs.
- Set `RUN_AT` and `TZ` for the daily schedule.

```dotenv
TARGET_EMAIL_PREFIXES=info@-----
```

That matches `info@-----mail.FnopEKGKj0M9cG.com` case-insensitively. It does
not match `sales@-----mail.example.com` or `myinfo@-----mail.example.com`.

## 3. Build and authorize once

```bash
docker compose build
docker compose run --rm outlook-junk-cleaner python /app/clean_junk.py authenticate
```

Open the URL printed in the terminal and enter its device code. The resulting
refresh-token cache is stored in the named Docker volume, not in the repository.

If authorization says the client is not configured for public flows, enable
**Allow public client flows** in the app registration and retry.

## 4. Start daily operation

```bash
docker compose up -d
docker compose logs -f
```

The service runs once on startup by default, then at `RUN_AT` every day. Docker
restarts it automatically unless you deliberately stop it. The Docker engine
(Docker Desktop on macOS) must be running for the schedule to execute.

There is no missed-day bookkeeping to configure: after downtime, the startup
run scans the full existing Junk Email backlog automatically.

Review several dry runs. When the matches are correct, set:

```dotenv
DRY_RUN=false
```

Then apply the change:

```bash
docker compose up -d --force-recreate
```

Microsoft Graph's normal message delete operation moves mail to Deleted Items;
it does not permanently purge the message.

## Useful commands

Run an extra cleanup immediately:

```bash
docker compose exec outlook-junk-cleaner python /app/clean_junk.py clean
```

Stop the service without deleting the login cache:

```bash
docker compose down
```

Remove the service and its saved Microsoft login:

```bash
docker compose down --volumes
```
