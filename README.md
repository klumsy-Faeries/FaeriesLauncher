# Faeries Launcher

A custom, high-performance Minecraft Java Edition launcher built from the
ground up. Rust backend, Tauri 2 shell, SolidJS frontend, and a token-based
theme engine where virtually every visual value is user-editable JSON.

> **Status: Phase 3 — Minecraft launches.** Create an instance, press Play,
> and the launcher installs the version (verified downloads), fetches the
> Java runtime that version requires, and starts the game with a live
> console. Verified end-to-end against real Minecraft 26.2. Modding arrives
> in Phase 4 — see the roadmap in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
>
> Online sign-in additionally needs a Mojang-approved Microsoft client ID —
> see [docs/ACCOUNTS.md](docs/ACCOUNTS.md). Without one, instances launch
> with an offline session (singleplayer/LAN only).

## Scope

This is a **launcher**: instance management, mod-loader installation
(Fabric/Quilt/Forge/NeoForge), official Microsoft authentication, and
performance-focused JVM configuration. It does not include and will not
include gameplay cheat modules or anything that circumvents server anti-cheat.

## Quick start

Prerequisites: Rust (stable), Node.js 20+, Windows 10/11 with WebView2
(preinstalled on Windows 11).

```
run.cmd
```

That builds the frontend if needed and starts the launcher. For development
with hot reload, use `dev.cmd`.

Prefer to drive it manually?

```
cargo run -p faerie-launcher
```

If PowerShell reports that running scripts is disabled when you use `npm` or
`npx`, that is a Windows execution-policy default, not a project problem —
see [PowerShell script execution](docs/BUILDING.md#powershell-script-execution).

## Layout

| Path | Purpose |
|---|---|
| `crates/faerie-core` | Config schema/store, events, tasks, logging, paths |
| `apps/launcher` | Tauri shell: IPC commands, event forwarding, theme resolution |
| `ui/` | SolidJS frontend (routes, components, command registry, i18n) |
| `themes/` | Built-in themes (embedded at compile time) |
| `locales/` | UI strings (`en-US.json`) |
| `docs/` | Architecture, building, theming |

User data lives in `%APPDATA%\FaerieLauncher` (override with the
`FAERIE_DATA_DIR` environment variable).

## Documents

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — design, boundaries, roadmap
- [docs/BUILDING.md](docs/BUILDING.md) — build, test, develop
- [docs/PERFORMANCE.md](docs/PERFORMANCE.md) — measured numbers and how to reproduce them
- [docs/THEMING.md](docs/THEMING.md) — make a theme without touching code
- [CONTRIBUTING.md](CONTRIBUTING.md) · [CHANGELOG.md](CHANGELOG.md)
