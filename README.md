# bookmark-nav

A small portable desktop bookmark manager built with [Tauri](https://tauri.app) + Vue 3 + TypeScript. It runs as a tray app: a global hotkey (`Alt+Space`) toggles a window where you can organize bookmarks into categories, search them, and add new ones — with title, description, and favicon auto-fetched from the URL.

## Features

- Categorized bookmarks with instant search/filter
- System tray icon + global `Alt+Space` shortcut to show/hide the window
- Launches at login (autostart)
- Add a bookmark by URL only — title, meta description, and favicon are fetched automatically
- Portable: no installer-managed app-data folder — your data travels with the app

## Data storage

Bookmarks are stored in a `bookmarks.json` file next to the app's executable (not in a system app-data directory), so the whole folder can be copied between machines and keeps working. In development (`pnpm tauri dev`), that means `src-tauri/target/debug/bookmarks.json`. If that location isn't writable (e.g. installed to `Program Files`), it automatically falls back to the OS app-data directory instead.

`src-tauri/bookmarks.json` in this repo is just seed/example data (not read directly by the app) showing the expected shape — copy it next to your built executable to start with some sample bookmarks instead of an empty list.

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

## Tech stack

- Frontend: Vue 3, TypeScript, Vite, Tailwind CSS
- Shell/backend: Tauri v2 (Rust) — handles reading/writing `bookmarks.json` and fetching page metadata for new bookmarks

## License

MIT — see [LICENSE](LICENSE).
