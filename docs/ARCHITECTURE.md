# Architecture

## Principles

- **Versions as data** — nothing assumes Minecraft 26.2; version behavior
  comes from Mojang metadata parsed generically.
- **Configuration over code** — every tunable lives in the settings registry
  or a theme file; components consume tokens/settings, never literals.
- **Commands, not buttons** — user actions are registry entries triggerable
  from UI, shortcuts, the palette, and future plugins.
- **No polling** — state changes propagate through one event bus; idle CPU
  stays at zero.
- **Thin shell** — the Tauri app layer holds no business logic; it adapts
  `faerie-core` (and future crates) to IPC.

## Crate map

```
ui/ (SolidJS)  ──IPC──  apps/launcher (Tauri shell)
                              │
        ┌──────────────┬──────┴───────┬──────────────────┐
        │ faerie-      │ faerie-      │ faerie-instances │
        │ minecraft    │ core         │ (instance store) │
        │ (manifest,   │ (config,     └──────────────────┘
        │  java, hw)   │  events,
        │      │       │  tasks, log)
        │ faerie-net   │
        │ (http, dl)   │
        └──────────────┘
   faerie-net is deliberately standalone (no faerie-core dependency):
   cancellation is a CancellationToken, progress is a callback. Later
   phases add faerie-auth, faerie-modding, faerie-diagnostics.
   test-support (dev-only) provides a fault-injecting HTTP server.
```

## Configuration system

`faerie-core/src/config/schema.rs` is the **single registry** of settings:
id, file, type, bounds, default, restart requirement. Everything derives from
it — the JSON files under `config/`, validation, the settings UI, and the
values IPC. To add a setting: add one entry there and two locale strings
(`setting.<id>.name` / `.description`). Nothing else.

Load behavior: missing files mean defaults; an unparseable file is backed up
to `config/corrupt/<name>.<timestamp>.json`, replaced with defaults, and
surfaced to the user (never silently destroyed); an invalid value falls back
to its default with a logged warning; unknown keys are preserved across
save/load for forward compatibility. Saves are atomic (temp file + rename).

## Events

`EventBus` is a broadcast channel of serializable `Event`s. The shell
forwards every event to the webview as the single `faerie://event` stream.
Task progress, setting changes, and later download/game state all use it.

## Tasks

`TaskScheduler::spawn(name, |ctx| async …)` runs named async jobs that report
`TaskStarted/Progress/Finished` on the bus and support cancellation. The
Phase 2 download manager and Phase 3 installers run on this.

## Theme engine

Built-in themes (`themes/<name>/`) are embedded at compile time; user themes
live in `<data>/themes/<name>/` and override by name. Files flatten to
kebab-case tokens (`colors.json: backgroundAlt` → `color-background-alt`)
which the frontend applies as `--fae-*` CSS custom properties. Resolution
always starts from the complete `faerie` token set so a partial or broken
theme degrades per-token, never to a blank window. `layout.json` controls
component visibility/order/sizes. See [THEMING.md](THEMING.md).

## IPC surface (Phase 1)

`settings_schema`, `settings_values`, `set_setting`, `recovery_notices`,
`app_info`, `list_themes`, `get_theme`, plus the `faerie://event` stream.
The frontend accesses all of it through `ui/src/ipc/backend.ts`, which
substitutes an in-memory mock outside Tauri so the UI runs in a plain
browser for development.

## Launching Minecraft

The Play flow (`apps/launcher/src/launching.rs`) is the one place that
combines every crate:

1. **Resolve** — the version manifest supplies the version JSON URL;
   `faerie-minecraft::install` parses it and walks `inheritsFrom` to a
   self-contained `VersionDetail`.
2. **Install** — the client jar, rule-filtered libraries, native classifiers,
   asset index, all asset objects, and the logging config become one download
   batch. Everything is SHA-1 verified and shared across instances, so a
   second instance on the same version downloads nothing.
3. **Java** — an installed JVM is used when it satisfies the version's
   required major; otherwise the runtime the metadata *names*
   (`javaVersion.component`) is fetched from Mojang's runtime manifest into
   `<data>/java/<component>`.
4. **Launch** — `launch::build_spec` produces the exact classpath and
   argument vector with all `${...}` substitutions applied, honoring both the
   modern `arguments` model and legacy `minecraftArguments`.
5. **Run** — `process::GameProcess` spawns the JVM, streams stdout/stderr as
   bounded `GameLog` events, and classifies the exit into launcher / Java /
   Minecraft / mod failures from the tail of the output (§25).

Authentication is separate: `faerie-auth` runs the Microsoft device-code
chain and hands the Play flow a `Session`. With no account signed in, an
offline session is used — playable locally, not on online servers.

## Logging

`tracing` with a non-blocking writer to `logs/launcher.log` plus stderr.
Level comes from the `advanced.log_level` setting. Sensitive values are typed
`Secret<T>`, which formats as `[REDACTED]` — the redaction is structural, not
disciplinary. Per-domain files (network.log, authentication.log,
minecraft.log) are added as target-filtered layers in the phases that
introduce those domains.

## Commands, search, and shortcuts

`ui/src/commands/registry.ts` holds every user-triggerable action. Buttons,
keyboard shortcuts, and the Ctrl+K palette all dispatch through it, so one
action stays reachable from everywhere.

Commands come in two kinds. **Static** ones are registered at startup
(navigation, launcher actions). **Provided** ones come from a *provider*
function consulted each time search runs, which is how live things —
instances, settings and their current values, Minecraft versions — are
searched without keeping a stale copy.

Shortcuts are `shortcuts.*` entries in the settings registry rather than a
separate keybindings file, so they inherit validation, defaults, corruption
recovery, and the Settings UI. `commands/shortcuts.ts` parses accelerators
(`Ctrl+Shift+R`), treats Ctrl and Cmd alike, and refuses to fire a
modifier-less binding while the user is typing.

## Roadmap

| Phase | Deliverable |
|---|---|
| 1 ✅ | Skeleton: workspace, core config/events/tasks/logging, themed shell, settings round-trip |
| 2 ✅ | Download manager, version manifest + cache, Java detection, hardware detection, instance store |
| 3 ✅ | Version pipeline, assets/libraries/natives, Java provisioning, MSA auth, **vanilla 26.2 launches** |
| 4 ✅ | Mod loaders (Fabric/Quilt), mod scanning/compat, mod store + profiles |
| 5 ✅ | Full UI: all routes, command palette, shortcuts, virtualized lists, first-launch wizard |
| 6 ✅ | Theme hot reload, asset system, layout overrides, i18n from disk |
| 7 ✅ | Benchmarks, startup instrumentation, perf dashboard, measured optimizations |
| 8 | Hardening: integration/stress/offline tests, crash reporting, backups |
| 9 | Installer, auto-update, release docs |
