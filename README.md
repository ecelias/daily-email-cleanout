# Daily Email Cleanout

A local macOS application that removes one recurring class of spam from a
personal Outlook Junk Email folder.

The stable sender pattern is:

```text
info@-----
```

The app scans the complete Junk Email backlog, follows all Microsoft Graph
pages, snapshots every match before deletion, and moves matches to Deleted
Items. It never permanently purges messages.

`MINIMAL_SPEC.md` is the immutable behavior specification. `AGENTS.md` contains
the immutable development rules for future coding agents.

## Native macOS app

The desktop implementation lives in [`desktop/`](desktop/). It uses:

- Tauri 2
- React and TypeScript
- Rust
- Microsoft device-code authorization
- macOS Keychain
- A local `launchd` background helper

No Docker service, cloud hosting, remote scheduler, App Store publication, or
deployed backend is required.

### Build locally

```bash
./scripts/build-macos.sh
```

The script checks prerequisites, installs JavaScript dependencies, runs checks
and tests, and produces:

```text
desktop/src-tauri/target/release/bundle/macos/Daily Email Cleanout.app
desktop/src-tauri/target/release/bundle/dmg/Daily Email Cleanout_0.1.0_aarch64.dmg
```

Move the `.app` to `/Applications`, open it, enter the Microsoft Application
Client ID, and complete the device-code sign-in. The client ID is not a secret.

The Microsoft Entra application registration must:

- Support personal Microsoft accounts.
- Use the `consumers` authority.
- Be configured as a Mobile and desktop application.
- Enable public client flows.
- Have delegated `Mail.ReadWrite`, `User.Read`, and `offline_access`
  permissions.

New installations begin in dry-run mode. Review the first scan before enabling
deletion.

### Local files

The application stores settings and bounded run history under:

```text
~/Library/Application Support/com.elizabeth.daily-email-cleanout/
```

Refresh credentials are stored in macOS Keychain. Access tokens remain in
memory. The optional daily schedule is installed at:

```text
~/Library/LaunchAgents/com.elizabeth.daily-email-cleanout.plist
```

### Uninstall

Disable scheduling in the app, disconnect Microsoft, and then remove:

```text
/Applications/Daily Email Cleanout.app
~/Library/Application Support/com.elizabeth.daily-email-cleanout/
```

If necessary, remove the LaunchAgent manually:

```bash
launchctl bootout "gui/$(id -u)" \
  "$HOME/Library/LaunchAgents/com.elizabeth.daily-email-cleanout.plist" 2>/dev/null || true
rm -f "$HOME/Library/LaunchAgents/com.elizabeth.daily-email-cleanout.plist"
```

## Legacy Docker implementation

The original Python/Docker implementation remains in the repository as a
behavioral reference while desktop parity is verified. It is not required to
run the macOS application.
