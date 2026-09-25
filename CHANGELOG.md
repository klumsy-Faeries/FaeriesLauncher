# Changelog

## 0.7.2 — Fix: window never appeared (2026-08-31)

- **The Fairy Castle GUI pack is enabled by default.** The pastel HUD and
  menu textures ship in the launcher next to the server pack and go on top
  of the pack stack in every instance the Optimized set is installed into.
- **The theme and language pickers show what is set.** Settings applied the
  saved value to the dropdown before the list of themes had loaded, so the
  picker fell back to the first entry (SMP) whenever the page opened, even
  with Skyblock active. The matching entry is now marked selected once the
  list is there.
- **A Windows installer for players.** `build-installer.cmd` produces
  `Faeries Launcher_0.7.2_x64-setup.exe` (NSIS, per-user install, no admin
  prompt, Start menu entry). The mods are not inside it: the first-run
  wizard installs Minecraft 26.2, Fabric, the mod set, the Faeries mods and
  the server pack, so a player is ready to join after one download. The
  Tauri CLI must run from the repository root, so `dev.cmd` now does too.
- **The Optimized set now carries every mod of Fabulously Optimized.** The
  default instance gets the 46 mods of Fabulously Optimized 14.1 (its
  Minecraft 26.2 release) on top of BadOptimizations: the performance
  stack it already had, plus Iris shaders, the OptiFine-style
  resource pack features (Continuity, Entity Texture and Model Features,
  Animatica, BetterGrassify, Skyboxify, Polytone, OptiGUI, Puzzle),
  LambDynamicLights, capes, Mod Menu, Zoomify, Controlify, borderless
  fullscreen, FastQuit, and the rest. Left out on purpose: the pack's two
  resource packs, its Config Manager and Main Menu Credits (they carry the
  pack's own files), and MixinTrace Reborn (no 26.2 build). Mods the pack
  ships as betas may fall back to a beta; everything else must be a release.
  About 45 MB of downloads instead of 8.
- **Krypton is retired from the set.** Its build for Minecraft 26.2 fails a
  mixin on the login packet handler; Controlify loads that class at startup,
  so the game crashed before the title screen. A preset can now retire mods:
  installing it removes any build of a retired mod from the instance's
  profile (the store keeps the jar) and reports it, so instances that
  already had Krypton lose it on the next *Add optimized mods*.
- **A newer build of a mod replaces the older one.** Adding a jar whose mod
  id is already in the profile (re-running the preset after Modrinth shipped
  updates, or dropping in a Sodium update by hand) used to put both builds
  in `mods/`, which Fabric refuses to start. The older jar now leaves the
  profile (the store keeps it) and the log names the replacement.
- **Descriptors with raw line breaks inside a string now parse.** Fabric
  Loader accepts them, strict JSON does not, and BetterGrassify, Entity
  Texture Features and Entity Model Features all ship one in their
  description. They showed as "no recognizable mod descriptor" on the Mods
  page and were flagged by the compatibility check; now they scan like any
  other mod.
- **Themes are now SMP and Skyblock.** The built-in themes are named after
  the Faeries worlds: **SMP** is the castle look that was called Faerie and
  stays the default; **Skyblock** replaces Dark with a pixel sky (clouds,
  floating islands, the rainbow strip, grass) and the pink slot frames of
  the Skyblock GUI, with squarer corners and chunkier borders to match.
  Both share the emblem. Settings files that still say `faerie` or `dark`
  open as SMP without a warning.
- **The compatibility check understands bundled jars and build metadata.**
  It reported seven errors on a set that runs fine: "Sodium requires
  fabric-resource-loader-v0, which is not installed" (it is — inside Fabric
  API, which is forty-odd modules in one jar, and Sodium bundles its own
  copies too) and "Reese's Sodium Options needs sodium >=0.9.1, but
  0.9.1+mc26.2 is installed" (`+mc26.2` is build metadata, not a
  pre-release). The scanner now reads jars bundled inside jars — Fabric and
  Quilt `jars`, Forge and NeoForge `META-INF/jarjar/metadata.json`, at any
  depth — and counts what they provide at their own versions, keeping the
  newest copy of a library that several mods carry, as the loader does.
  Versions drop `+build` metadata before comparison, pre-release identifiers
  compare the semver way (`beta.2 < beta.10 < rc`), and Fabric's trailing
  dash (`~26.2-`, "26.2 or any pre-release of it") is honoured.
- **`run.cmd` no longer relinks a second time after an interface change.**
  Tauri's code generator writes its embedded-asset cache while the launcher
  compiles, which left those files newer than cargo's freshness reference,
  so the next start rebuilt the launcher again (about 1.5 minutes) for
  nothing. `run.cmd` now settles those timestamps after each build
  (`scripts/backdate-codegen-assets.mjs`) and starts the built launcher
  directly.
- **Faeries SMP is on the server list by default.** New instances (and
  existing ones when the preset is installed) get `faeriessmp.com` in
  `servers.dat` with server resource packs enabled, next to whatever the
  player already had; an existing entry for the address is left alone. The
  file is the game's NBT, read and written without losing icons or settings.
- **The Faeries SMP pack is installed by default.** Nexo's server pack
  (27.5 MB) ships in the launcher; the Optimized preset copies it into the
  instance, enables it on top of the pack stack, and seeds the Pack Vault
  with it under its SHA-1 so a join to `mc.faeriessmp.com` announcing that
  build needs no download.
- The Optimized preset can carry **resource packs**: copied into the
  instance and enabled at the top of the stack in `options.txt` (other lines
  preserved, existing packs such as the Faeries menu kept). Adding the set is
  refused while the game is running, since the game would overwrite the
  file on exit.
- **"No account, no client ID, no instance" explained.** A launcher started
  from inside an MSIX-packaged app (a Store app's terminal, for example)
  runs with that package's identity, and Windows redirects its
  `AppData` writes into the package's private overlay. Two launchers then
  disagree about what is on disk: the one started from the packaged shell
  sees a signed-in account, the client ID and instances that the one started
  from Explorer cannot. Startup now checks where a write under AppData
  really lands and logs it (the real folder, or the overlay path), plus the
  resolved config folder and its listing, which config files it found with
  sizes and whether a client ID is set; every account listing logs the
  count and the file it read. See docs/BUILDING.md, "Testing from inside a
  packaged app".
- Startup keeps watching for config files it could not see and, when one
  that predates the start becomes readable, reloads settings and refreshes
  the window. Settings saves are read-modify-write, so a file that was
  unreadable at load is never overwritten with defaults.
- The instance pickers on Home and Mods could display one instance while the
  page acted on another (the selection was applied before the options had
  loaded); they now always show the instance in use.
- Each instance row has a **Mods** link, and Home's mod links open the
  instance being shown (`/mods?instance=<id>`), instead of the Mods page
  always landing on the newest instance.
- Preset installs reuse jars already in the mod store instead of downloading
  them again, so adding the set to a second instance costs no bandwidth.
- **Modded instances launched vanilla.** Play installed and started the
  instance's Minecraft version and never looked at its loader, so a Fabric
  instance with every mod in place ran without a single mod loading. Play
  now launches the loader's version (`fabric-loader-<loader>-<mc>`), which
  the installer resolves through `inheritsFrom` as before. Loaders whose
  install is not implemented (Forge, NeoForge) now say so at Play instead of
  silently running vanilla. The session choice (signed-in / refreshed /
  offline, and why) is logged on every launch.
- **Faeries companion mods ship with the launcher.** The Optimized preset
  now also installs **Faeries Theme** (Faeries logo as the game window icon,
  loading screen, title panorama, and menu buttons) and **Faeries Pack
  Vault** (server resource packs kept locally, hash-verified), taken from
  the FaeriesClient repository's `apps/theme-mod` and `apps/vault-mod` builds
  for 26.2 and compiled into the launcher (`mods/`). A bundled jar is only
  installed into instances on the version it was built for.
- **"No Minecraft version available" when creating an instance.** The
  version list reached the interface with Mojang's field name (`type`)
  while every page filters on `kind`, so the Instances page, the setup
  wizard, and the Versions page (releases-only view) all saw an empty list.
  The manifest now serialises `kind`; a test pins the wire shape. Both
  create paths also wait for the list instead of failing if Create is
  pressed before it has arrived (the dropdown reads "Loading versions…"
  meanwhile), and the wizard no longer silently skips creating the instance
  in that case.
- With no instances yet, Home shows a **Create your first instance** action
  in place of an empty "Select instance" picker, and creating an instance
  without typing a name uses "Faeries" instead of rejecting the request.
- **The castle painting is the background scene.** The built-in theme now
  embeds raster artwork (a 1.3 MB JPEG copy of `art/background-5296.png`)
  in place of the hand-drawn SVG placeholder.
- **`run.cmd` is fast when nothing changed.** It used to rebuild the
  interface on every run, and because that rewrote `ui/dist`, Tauri relinked
  the launcher (~1.5 min) for nothing. The interface now rebuilds only when
  its sources changed (`ui/scripts/build-if-stale.mjs`), so an unchanged
  launcher starts in about a second.
- A failure to read the account list is now logged and shown, instead of
  silently rendering as "Offline session".
- **Optimized preset (§13).** New instances start as Fabric plus the standard
  performance stack — Sodium, Lithium, FerriteCore, ImmediatelyFast, Entity
  Culling, Krypton, Dynamic FPS, BadOptimizations, Reese's Sodium Options,
  Sodium Extra, and Fabric API — on by default in the setup wizard and the
  Instances page, and available for any existing instance from the Mods page.
  Mods are resolved against the instance's Minecraft version on Modrinth at
  install time (release builds preferred; Sodium may use its beta for a brand
  new game version), downloaded with hash verification, and enabled in the
  active profile. Required dependencies are pulled in automatically. A mod
  with no build for the version is reported by name, never substituted.
  Nothing in the set changes gameplay.
- **The Faeries emblem is the logo.** It replaces the text wordmark on
  Home, the sidebar glyph, and the window/taskbar icon. The built-in theme
  embeds sized copies (768px hero, 256px mark) of the artwork kept in
  `art/logo-2500.png`; the icon set is regenerated from the full-resolution
  original.
- Theme artwork slots are now reactive: the hero and sidebar used to read
  the `--fae-asset-*` custom properties once at mount, before the theme had
  been applied, so a theme's logo and mark never appeared. A theme switch
  also clears tokens the new theme does not define.
- **Hero centred.** The logo and Play button now sit on the centre of the
  page (symmetric side columns: account and status cards stacked on the
  left, news on the right), and the background scene is centred on the
  content area rather than the window so the castle lines up underneath.
- **Renamed to Faeries Launcher** in the window title, sidebar, wordmark,
  setup wizard, User-Agent, and the launcher name passed to the game.
  Internal identifiers (data folder `FaerieLauncher`, crate and theme
  names) are unchanged so existing data stays where it is.
- **Sign-in ownership check no longer rejects Game Pass accounts.** The
  store entitlement list (`entitlements/mcstore`) is empty for accounts that
  have Java Edition through Game Pass; the profile endpoint is now the
  authority (404 there means the account has no Java Edition).
- **Sign-in failed at the last step with "secure credential storage failed:
  … longer than platform limit of 2560 chars".** Windows Credential Manager
  caps one credential at 2560 bytes and the keyring stores UTF-16, so a
  single entry holds ~1280 characters — less than one Minecraft access
  token. Tokens are now split across numbered credential entries
  (`<id>` holds the count, `<id>/<n>` the pieces); reads reassemble them,
  removal deletes every piece, and an interrupted write reads as "not
  stored" rather than as garbage.
- Sign-in failures stay on the Accounts page until dismissed instead of
  only flashing as a toast, and are logged at WARN (messages carry no
  tokens) so they can be diagnosed after the fact.
- **The launcher would not open a window.** Setting `decorations: false` to
  draw a custom titlebar stopped this Tauri/Windows build from creating the
  window at all — the process started and logged a clean boot, but only
  Tao's internal 16x16 message window existed. Confirmed by bisection; the
  OS frame is back on.
- Window controls now render only when the window reports itself as
  undecorated, so they can never duplicate the system buttons.
- `run.cmd` always rebuilds the interface. It previously skipped the build
  whenever `ui/dist` existed, so UI changes silently ran a stale bundle.
- **"localhost refused to connect" on start.** Tauri only embeds the built
  interface when the `custom-protocol` feature is on; a plain `cargo run`
  (any profile) points the window at the Vite dev server instead. The
  launcher crate now declares the feature and `run.cmd` enables it.
- **Sign-in prompt showed "Open undefined in your browser"** with no code.
  The device-code prompt was serialized with Microsoft's snake_case field
  names while the UI, like every other IPC payload, reads camelCase. It now
  serializes camelCase (deserialization of Microsoft's response is
  unchanged), a test pins the wire shape, and the browser-preview mock
  returns a realistic prompt instead of always failing, so this path is
  exercised outside the real launcher too.

## 0.7.1 — Reference visual match (2026-08-31)

- **Custom window chrome**: OS decorations off, pink minimize/maximize/close
  buttons, and a drag region in the top bar.
- **Sidebar**: brand card with mark, name, and version; chunkier uppercase
  navigation; social links as line icons.
- **Home dashboard**: account card, instance status card with a count
  metric, an outlined wordmark that reads over the background scene, the
  Play button with quick-action buttons and instance selector beneath it,
  a What's New column with per-item icons and a footer action, and a
  four-card bottom row with count badges and footer buttons.
- **Status bar**: state dot with live text (up to date / installing /
  running), data folder path, and an Open data folder action.
- New `open_folder` command (§28) for revealing launcher directories.
- Cards for features the launcher does not have (cosmetics, friends) show
  an honest empty state rather than invented data.

## 0.7.0 — Phase 7 optimization (2026-08-31)

- **Benchmarks** (§43) for the paths that run against user-sized data: mod
  scanning and compatibility checking, version-JSON parsing and inheritance,
  argument building, instance listing, version-range matching. Run with
  `cargo bench --workspace`; Criterion flags regressions against the
  previous run.
- **Startup instrumentation**: every launch logs a per-stage breakdown, so a
  slow start names the stage responsible.
- **Performance dashboard** (§27): live launcher and game memory/CPU, the
  startup breakdown, and transfer activity. Sampling runs only while the page
  is open and stops on close.
- **Instance listing is 55% faster** — it made three filesystem round-trips
  per instance where one suffices. The Home page also fetched the list twice;
  it now fetches once.
- Measured results, including a memory finding that contradicts the original
  architecture estimate, are recorded in
  [docs/PERFORMANCE.md](docs/PERFORMANCE.md).

## 0.6.0 — Phase 6 customization (2026-08-30)

- **Theme hot reload** (§31): the themes folder is watched, so editing a
  theme's JSON or artwork updates the running launcher without a restart.
  Events are debounced, and the watch uses OS notifications rather than
  polling, so an untouched folder costs nothing.
- **Asset system** (§32): a theme can supply `assets/background.*`,
  `logo.*`, `mark.*`, and `assets/icons/<name>.*` in SVG, PNG, WebP, JPEG,
  or GIF. Each becomes a `--fae-*` token; missing slots fall back to the
  built-in drawing, so a theme can replace one icon and inherit the rest.
- **Layout overrides** (§6): show, hide, and reorder sidebar items and home
  dashboard cards from Settings. Overrides are stored in config and merged
  over the theme's `layout.json`, so they survive a theme switch and never
  write into a built-in theme.
- **Localization** (§49): languages are plain JSON files in
  `<data>/locales/`, discovered at runtime and merged over English, so a
  partial translation shows translated text where it exists and English
  elsewhere. Unknown keys are reported rather than silently kept.
- Fixed: `launcher.language` was a fixed-option setting, which would have
  rejected any locale the user added themselves.

## 0.5.0 — Phase 5 full UI (2026-08-30)

- **Command system** (§23): every action is a registry entry, dispatched the
  same way from buttons, shortcuts, and the palette. Live entries come from
  *providers* consulted at search time, so instances and settings are never
  stale.
- **Global search** (§21), Ctrl+K: one box over actions, settings,
  instances, and Minecraft versions, grouped by category with current values
  shown as hints. Typing "RAM" finds the RAM settings and Enter jumps to and
  focuses the field.
- **Configurable shortcuts** (§22): declared in the settings registry, so
  they get validation, persistence, and a Settings UI for free. Ctrl and Cmd
  are treated alike; bare-letter bindings are ignored while typing.
- **Downloads page**: live task feed from the event bus, with install
  progress, active tasks, and a bounded recent-history list.
- **Virtualized lists** (§7, §43): the ~900-entry version list keeps ~26 rows
  in the DOM regardless of length.
- **First-launch wizard** (§37): welcome, detected hardware and Java, theme,
  optional sign-in, and a first instance — then it never appears again.

## 0.4.0 — Phase 4 modding (2026-08-30)

- New `faerie-modding` crate.
- **Mod scanning**: reads `fabric.mod.json`, `quilt.mod.json`,
  `META-INF/mods.toml`, and `META-INF/neoforge.mods.toml` straight from jars,
  in parallel. Jars with no descriptor are reported, never hidden.
- **Version ranges**: both dialects — Fabric/Quilt semver predicates
  (`>=1.20`, `^1.0`, `1.2.x`, `||` alternatives) and Maven intervals
  (`[1.20,1.21)`, `[47,)`). Unparseable requirements are flagged rather than
  turned into false incompatibility verdicts.
- **Compatibility engine**: missing dependencies, version mismatches,
  explicit conflicts, duplicate mod ids, and wrong-loader detection, each
  reported as what happened / why / how to fix (§47). A wrong-loader mod
  reports only the root cause, not the dependency cascade beneath it.
- **Mod store and profiles**: content-addressed `mod-store/<sha1>.jar` shared
  across instances; profiles are manifests materialized via hardlinks, so
  switching is a link swap and disabling never deletes a jar (§13, §14).
- **Loader adapters**: one trait; Fabric and Quilt install fully (their
  profile JSONs layer onto vanilla through the Phase 3 `inheritsFrom`
  pipeline). Forge and NeoForge list versions but decline installation with
  an explanation — their installer processor pipeline is not implemented.
- Mods page: instance picker, loader install, profile switching, per-mod
  enable/disable/remove, and the compatibility report.

## 0.3.0 — Phase 3: Minecraft launches (2026-08-30)

- **Version pipeline**: full version-JSON model with `inheritsFrom`
  resolution, OS/arch/feature rule evaluation, modern + legacy argument
  models, and `${variable}` substitution. No Minecraft version is hard-coded.
- **Installer**: resolves a version, then downloads and SHA-1 verifies the
  client jar, rule-filtered libraries, native classifiers, asset index, every
  asset object, and the logging config through the Phase-2 download manager;
  extracts natives; materializes legacy/virtual asset trees.
- **Java provisioning**: fetches the exact runtime a version names
  (`javaVersion.component`, e.g. `java-runtime-epsilon` for Java 25) from
  Mojang's runtime manifest when no installed JVM qualifies.
- **Launching**: builds the classpath and full argument vector, spawns the
  JVM in the instance directory, streams stdout/stderr as bounded console
  events, and classifies exits as launcher / Java / Minecraft / mod errors.
- **`faerie-auth`**: Microsoft device-code sign-in → Xbox Live → XSTS →
  Minecraft services → entitlement → profile, with token refresh. Tokens are
  `Secret`-wrapped and stored in the OS credential manager; the metadata file
  never contains them. Specific errors for no-Xbox-account, child accounts,
  and missing game entitlement.
- **UI**: Play flow with live install progress and cancel, a searchable
  Minecraft console, exit reporting, and an Accounts page.
- Verified live: real 26.2 metadata (Java 25, 131 libraries), a full 5147-file
  install, Java 25 auto-provisioned, and the real game JVM reaching
  `Setting user` in Minecraft's own render thread.

## 0.2.0 — Phase 2 launcher core (2026-08-30)

- `faerie-net`: download manager — parallel transfers with configurable
  concurrency, resume via HTTP Range from `.part` files, SHA-1 verification
  before atomic rename, exponential-backoff retries (permanent errors fail
  fast), skip-if-already-valid, cancellation, live progress with speed.
- `faerie-minecraft`: Mojang version-manifest service (ETag revalidation,
  30-minute freshness window, stale-cache offline fallback); Java detection
  (JAVA_HOME, PATH, registry, vendor dirs, managed runtimes — every
  candidate probed by execution); hardware detection with a conservative
  heap recommendation.
- `faerie-instances`: instance store — create/list/rename/duplicate/delete
  with stable folder ids, schema-versioned `instance.json`, standard
  subdirectory layout, and delete-to-trash instead of permanent removal.
- `test-support`: in-process HTTP server with Range support and
  connection-drop fault injection for hermetic network tests.
- UI: functional Instances page (create/rename/duplicate/two-step delete)
  and Versions page (release/snapshot filter, refresh, offline notice).
- New setting: `downloads.retries`. Live (`--ignored`) tests verify the real
  Mojang manifest and real JVM detection.

## 0.1.0 — Phase 1 skeleton (2026-08-30)

- Cargo workspace: `faerie-core` + `faerie-launcher` (Tauri 2 shell) + SolidJS UI.
- Settings system: single Rust schema registry; validated, atomic JSON
  persistence; corrupt-file backup and recovery with user notification;
  unknown keys preserved.
- Event bus and task scheduler with progress/cancellation, forwarded to the
  webview as one event stream.
- Logging via `tracing` to `logs/launcher.log`; `Secret<T>` redaction type.
- Theme engine: built-in `faerie` (default) and `dark` themes; user theme
  folders override by name; tokens applied as `--fae-*` CSS custom
  properties; layout config (sidebar items/width, status bar, home sections).
- Schema-driven Settings page with live theme/font-scale/reduced-motion
  application; i18n layer with `en-US`; command registry groundwork.
- Browser-preview mode: the UI runs without the shell against a mock backend.
