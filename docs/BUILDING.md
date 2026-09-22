# Building

## Prerequisites

- Rust stable (1.85+) — `rustup` recommended
- Node.js 20+ and npm
- Windows 10/11. WebView2 runtime (preinstalled on Windows 11; on older
  Windows 10 the Evergreen runtime installs on demand)

## Quickest path (Windows)

Double-click or run from any shell:

```
run.cmd     # start the launcher; rebuilds only what changed (≈1 s when nothing did)
dev.cmd     # development mode with hot reload
```

These are batch files, so they work regardless of PowerShell's execution
policy — see [PowerShell script execution](#powershell-script-execution) below.

## Commands

```
# one-time
cd ui && npm install

# frontend: typecheck + production bundle (outputs ui/dist)
cd ui && npm run build

# run all Rust tests
cargo test --workspace

# run the launcher with the built UI embedded (build the frontend first)
cargo run -p faerie-launcher --features custom-protocol

# development with hot reload (starts vite + the shell together)
cd ui && npx tauri dev

# Windows installer for players (see "Installer" below)
build-installer.cmd

# UI-only preview in a normal browser (mocked backend, no Rust needed)
cd ui && npm run dev    # → http://localhost:1420
```

## Installer (what players download)

`build-installer.cmd` produces the Windows setup players run:

```
target\release\bundle\nsis\Faeries Launcher_<version>_x64-setup.exe
```

It is an NSIS installer configured in `apps/launcher/tauri.conf.json`
(`bundle`): installs **per user** into `%LOCALAPPDATA%\Programs` with no
admin prompt, adds a Start menu entry, and fetches the WebView2 runtime
only on a machine that lacks it (every Windows 11 has it). The version in
the file name comes from `tauri.conf.json`; bump it together with the
crate versions and the changelog for a release.

The mods are **not** in the installer. On first run the setup wizard
creates the Faeries instance with *Start optimized* ticked, which downloads
Minecraft 26.2, the Fabric loader and the mod set from Mojang and Modrinth
(about 640 MB, once), installs the server pack, and puts
`faeriessmp.com` on the server list. Fetching from Modrinth keeps the mods
current and avoids redistributing jars whose licences forbid it.

Windows SmartScreen warns about unsigned installers from the internet
until the file has a reputation; a code-signing certificate removes the
warning.

## Publishing on GitHub

The repository is set up for GitHub: the website deploys to GitHub Pages
and the installer is attached to a release.

1. **Repository.** `https://github.com/klumsy-Faeries/FaeriesLauncher` (public: GitHub
   Pages on a free account needs a public repository). `origin` points at
   it; `git push` signs in through Git Credential Manager's browser prompt
   the first time.

2. **Website.** `.github/workflows/pages.yml` publishes `site/` at
   `https://klumsy-faeries.github.io/FaeriesLauncher/` on every push that touches it, and
   enables Pages on the repository by itself the first time. The page uses
   relative asset paths, so it works under that sub-path, and it can be
   served from a custom domain (a `site/CNAME` file plus a DNS record) if
   the launcher page should live under `faeriessmp.com`.

3. **Installer.** Pushing a version tag builds it on GitHub and publishes
   the release (`.github/workflows/release.yml`, about ten minutes):

   ```
   git tag v0.7.2
   git push origin v0.7.2
   ```

   The asset is named without spaces (GitHub turns them into dots), for
   example `Faeries-Launcher-0.7.2-x64-setup.exe`, and the release notes
   carry its SHA-256. The site's download button points at the
   repository's *latest release* page, so it never goes stale when the
   version changes; the direct file link is
   `https://github.com/klumsy-Faeries/FaeriesLauncher/releases/latest/download/Faeries-Launcher-0.7.2-x64-setup.exe`.

   To publish a locally built installer instead, copy it to a name without
   spaces and attach it with the GitHub CLI (`winget install GitHub.cli`,
   `gh auth login`, then `gh release create v0.7.2 <file>`) or on the
   release page by hand.

## PowerShell script execution

Node installs `npm` and `npx` as three shims: `npm.ps1`, `npm.cmd`, and an
extensionless one. PowerShell prefers the `.ps1`, and Windows blocks it when
the execution policy is `Restricted` (the default when every scope is
`Undefined`), producing:

```
npm : File C:\Program Files\nodejs\npm.ps1 cannot be loaded because
running scripts is disabled on this system.
```

Three ways around it, in increasing order of permanence:

1. Use `run.cmd` / `dev.cmd` — they call `npm.cmd` internally and never
   involve PowerShell.
2. Add `.cmd` at the call site: `npm.cmd run build`, `npx.cmd tauri dev`.
3. Allow local scripts for your user account (a Windows security setting —
   run it yourself, and only if you want it changed system-wide):
   `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned`
   This permits scripts you wrote locally and signed remote ones. Check the
   current state with `Get-ExecutionPolicy -List`.

Cargo is unaffected — `cargo.exe` is a real executable, so
`cargo run -p faerie-launcher --features custom-protocol` always works once
`ui/dist` exists.

## "localhost refused to connect"

Tauri chooses between the Vite dev server (`devUrl`) and the embedded
`ui/dist` bundle by the `custom-protocol` Cargo feature — not by the build
profile. A plain `cargo run` (debug *or* `--release`) leaves the feature off,
so the window tries `http://localhost:1420` and shows this error unless
`npm run dev` happens to be running. `run.cmd` passes
`--features custom-protocol`; `dev.cmd` (`tauri dev`) deliberately leaves it
off so the UI hot-reloads.

## Data directory

All launcher data (config, logs, themes, instances, game data) lives in
`%APPDATA%\FaerieLauncher`. Set `FAERIE_DATA_DIR` to relocate it — useful
for portable installs and for testing against a scratch directory.

## Testing from inside a packaged app (Store apps and similar)

If you start the launcher from a shell that itself runs inside an MSIX or
AppX package — some editors and terminals ship that way — every process it
spawns carries that package's identity, and Windows redirects their writes
under `%APPDATA%` and `%LOCALAPPDATA%` into the package's private overlay
(`%LOCALAPPDATA%\Packages\<package>\LocalCache\Roaming\FaerieLauncher`).
Reads see the real folder and the overlay merged, so everything looks fine
from inside; a launcher started from Explorer or a normal terminal sees only
the real folder and finds no account, no client ID and no instances. Desktop
folders, `~/.cargo`, `~/.rustup` and Credential Manager are not affected.

Startup writes a marker through the normal path, looks for it under the
overlays, and logs `AppData writes land in the real folder` or `AppData
writes are REDIRECTED to …`, so the log of the process in question settles
it. (The package-identity API does not catch this: the processes in
question report no identity.) To test as the user does, start things from
Explorer — children of Explorer are unpackaged:

```powershell
Start-Process explorer.exe -ArgumentList 'C:\path\to\FaeriesClientV2\run.cmd'
```

Any write into the real data folder from a packaged shell has to go the
same way (a `.cmd` started via Explorer that runs `robocopy`, `copy`, …).

## A build right after an interface change used to relink twice

Tauri's code generator writes its embedded-asset cache
(`target/release/build/faerie-launcher-*/out/tauri-codegen-assets/`) while
the launcher crate compiles. Cargo dates its freshness reference to the
start of that compile, so those files come out newer than the reference and
the next build relinks the launcher again (about 1.5 minutes) for nothing —
whoever runs cargo second pays it. `run.cmd` runs
`scripts/backdate-codegen-assets.mjs` after every build to settle those
timestamps; do the same after a manual `cargo build` when the next start
should be instant. Cargo's own explanation, should it recur:

```powershell
$env:CARGO_LOG = 'cargo::core::compiler::fingerprint=info'
cargo build --release -p faerie-launcher --features custom-protocol
```

## Conventions

- `cargo fmt` and `cargo clippy --workspace --all-targets -- -D warnings`
  must pass (CI enforces both).
- Frontend: `npm run typecheck` must pass; visual values only via
  `--fae-*` tokens; user-facing strings only via `t("key")`.
