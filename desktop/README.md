# Daily Email Cleanout for macOS

This directory contains the local Tauri desktop application. The React
interface is backed by Rust code that authenticates with Microsoft, scans the
complete Outlook Junk Email folder, snapshots all matching messages, and then
moves those matches to Deleted Items.

## Prerequisites

- Apple Silicon Mac
- Xcode Command Line Tools
- Node.js 20 or newer
- Rust stable toolchain

From this directory:

```bash
npm install
npm run check
npm run test
npm run desktop:build
```

Build artifacts are written under:

```text
src-tauri/target/release/bundle/macos/
src-tauri/target/release/bundle/dmg/
```

The app is locally/ad-hoc signed. It is not notarized or intended for public
distribution.

## Development

```bash
npm run tauri dev
```

The installed application stores:

- Preferences and run history under
  `~/Library/Application Support/com.elizabeth.daily-email-cleanout/`
- Refresh credentials in macOS Keychain
- Its LaunchAgent at
  `~/Library/LaunchAgents/com.elizabeth.daily-email-cleanout.plist`

The background schedule should be enabled from the packaged app after moving it
to `/Applications`; development executables are not stable LaunchAgent targets.
