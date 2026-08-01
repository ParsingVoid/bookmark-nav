# bookmark-nav

A small desktop bookmark manager built with [Tauri](https://tauri.app) + Vue 3 + TypeScript. It runs as a tray app: a global hotkey (`Alt+Space`) toggles a window where you can organize bookmarks into categories, search them, and add new ones — with title, description, and favicon auto-fetched from the URL.

## Features

- Categorized bookmarks with instant search/filter
- System tray icon + global `Alt+Space` shortcut to show/hide the window
- Launches at login (autostart)
- Add a bookmark by URL only — title, meta description, and favicon are fetched automatically
- Export/import your data as a JSON file, and automatic rotating backups on every save
- Checks for updates on startup and can self-update via GitHub Releases

## Data storage

Bookmarks are stored in a `bookmarks.json` file in the OS-standard app-data directory (Windows: `%APPDATA%\com.bookmark-nav.desktop\bookmarks.json`), independent of where the app itself is installed or how it's rebuilt. A fresh install starts with an empty list.

On uninstall (via the NSIS installer), you'll be asked whether to keep or delete this data — useful if you're reinstalling vs. uninstalling for good.

Every save keeps the previous version as a timestamped copy in `.../com.bookmark-nav.desktop/backups/` (last 10 kept) — if something goes wrong, use the Import button and pick one of those files to restore it.

`src-tauri/bookmarks.json` in this repo is just seed/example data (not read directly by the app) showing the expected shape.

## Security

The "fetch title/description/favicon from a URL" feature validates that a URL (and every redirect it follows) resolves to a public IP address before the Rust backend requests it, rejecting private/loopback/link-local ranges to prevent it being used to probe internal network services. See `cargo test` in `src-tauri/`.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [pnpm](https://pnpm.io/)
- Rust toolchain + [Tauri's platform prerequisites](https://tauri.app/start/prerequisites/) for your OS

## Getting started

```sh
pnpm install
pnpm tauri dev
```

## Building

```sh
pnpm tauri build
```

Produces a platform-native installer/bundle in `src-tauri/target/release/bundle/`.

## Auto-updates

Update checks call `https://github.com/ParsingVoid/bookmark-nav/releases/latest/download/latest.json`, which `tauri-action` generates and uploads automatically. Update packages are signed; the public key lives in `tauri.conf.json`, and the CI workflow signs new releases using a private key stored as the `TAURI_SIGNING_PRIVATE_KEY` GitHub Actions secret (never committed to the repo).

## Tech stack

- Frontend: Vue 3, TypeScript, Vite, Tailwind CSS
- Shell/backend: Tauri v2 (Rust) — handles reading/writing `bookmarks.json` and fetching page metadata for new bookmarks

## License

MIT — see [LICENSE](LICENSE).
