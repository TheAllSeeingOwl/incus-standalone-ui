# Incus Manager

A native desktop app for managing [Incus](https://linuxcontainers.org/incus/) containers and virtual machines. Wraps the official [incus-ui-canonical](https://github.com/zabbly/incus-ui-canonical) web UI in a Tauri shell with a local reverse proxy that handles TLS, mTLS, and Unix socket connections to Incus servers.

## Features

**Connection options**
- HTTPS with optional custom CA certificate
- Mutual TLS (client certificate + key) authentication
- Unix socket (`/var/lib/incus/unix.socket`) for local access
- Skip-TLS-verification mode for development
- Hot-reload — change connection settings without restarting

**Incus management** (via the embedded upstream UI)
- Container and VM lifecycle (create, start, stop, delete, migrate)
- Interactive console and exec sessions over WebSocket
- Configuration editing with Monaco editor (cloud-init, etc.)
- Storage pools, networks, profiles, images, projects
- Metrics and log viewing

**Desktop integration**
- System tray with quick-access menu (open, settings, quit)
- Close-to-tray behavior
- Built-in Incus documentation viewer (Sphinx HTML, served locally)
- External links open in the system browser
- Zoom controls in the docs window (Ctrl+/Ctrl-/Ctrl+0, Ctrl+scroll)

**Security**
- Proxy binds to `127.0.0.1` only (unreachable from the network)
- DNS rebinding protection via Host header validation
- UI assets compiled into the binary (no runtime file serving)
- Iframe isolation — the embedded UI cannot access Tauri IPC

## Architecture

```
┌─────────────────────────────────────────────┐
│  Tauri window                               │
│  ┌───────────┬─────────────────────────────┐│
│  │ Settings  │  <iframe>                   ││
│  │ sidebar   │  incus-ui-canonical SPA     ││
│  │ (React)   │  served from local proxy    ││
│  └───────────┴──────────┬──────────────────┘│
└─────────────────────────┼───────────────────┘
                          │ http://127.0.0.1:<port>
                          ▼
              ┌───────────────────────┐
              │  Axum reverse proxy   │
              │                       │
              │  /1.0/* /oidc/*       │──► Incus API (HTTPS or Unix socket)
              │  /ui/*                │──► Embedded SPA (rust-embed)
              │  /docs/*              │──► Embedded docs (rust-embed)
              │  WebSocket upgrade    │──► Bidirectional WS relay
              └───────────────────────┘
```

The proxy runs on a random localhost port. API requests are streamed (not buffered) so large file transfers, log streams, and exec sessions work without extra memory usage. Connection state is swapped atomically via `ArcSwap` so settings changes take effect on the next request with zero downtime.

## Building

### Prerequisites

Install [devenv](https://devenv.sh/) (or have Nix with flakes enabled). Then:

```sh
devenv shell
```

This provides the full toolchain: Rust, Node.js, Yarn, cargo-tauri, GTK/WebKit dev libs, Python + Go (for docs), and icon generation tools.

### Development

```sh
# Build the docs (one-time, or after Incus doc changes)
bash scripts/build-docs.sh

# Run with hot-reload
cargo tauri dev
```

### Release build

```sh
cargo tauri build
```

Outputs are in `src-tauri/target/release/bundle/` (deb, rpm, AppImage).

### Nix flake

```sh
# Build the package
nix build

# Build with native window decorations
nix build .#incus-manager-with-titlebar

# Run directly
nix run
```

The flake builds everything from source in a hermetic environment: Sphinx docs from the Incus repo, the UI SPA from incus-ui-canonical, the React settings shell, and the Rust binary. No network access during build.

## Configuration

Settings are stored in:
- Linux: `~/.local/share/app.incus.manager/incus-settings.json`
- macOS: `~/Library/Application Support/app.incus.manager/incus-settings.json`
- Windows: `%APPDATA%\app.incus.manager\incus-settings.json`

On first run, the app auto-detects a local Unix socket if present and opens the settings panel.

## Project structure

```
├── src/                     # React settings shell (sidebar UI)
├── src-tauri/
│   ├── src/
│   │   ├── proxy.rs         # Axum proxy, asset serving, WS relay
│   │   ├── commands.rs      # Tauri IPC commands
│   │   ├── config.rs        # Settings persistence
│   │   ├── menu.rs          # System tray
│   │   └── lib.rs           # Tauri setup and window management
│   ├── icons/               # App icons (all platforms)
│   └── build.rs             # Version metadata embedding
├── incus-ui-canonical/      # Upstream UI (git submodule)
├── scripts/build-docs.sh    # Sphinx docs builder
├── flake.nix                # Nix build (hermetic, reproducible)
└── devenv.nix               # Development environment
```

## License

Apache-2.0
