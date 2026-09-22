# Bundled mods

Jars in this folder are compiled into the launcher and installed by the
**Optimized** preset alongside the mods it fetches from Modrinth. They are
the Faeries companion mods, built from the `FaeriesClient` repository
(`apps/theme-mod` and `apps/vault-mod`), MIT-licensed, client-side only.

| File | Mod | What it does |
|---|---|---|
| `faeries-theme-0.1.0+mc26.2.jar` | Faeries Theme | Faeries logo as the game window/taskbar icon, Faeries loading screen, title-screen panorama and logo, and the "Faeries Menu" resource pack (button sprites). Cosmetic; remove it and vanilla comes back. Needs Fabric API (`fabric-resource-loader-v0`). |
| `faeries-vault-0.1.0+mc26.2.jar` | Faeries Pack Vault | Keeps server resource packs stored locally so joining is instant — verifies by hash, falls back to a normal download when anything differs. Works on any server. |

Each jar targets **one** Minecraft version (the `+mc26.2` suffix). The preset
installs a bundled jar only into instances on that version and reports it as
skipped otherwise — it never installs a build for the wrong version.

## Updating

1. In `FaeriesClient/apps/theme-mod` (or `vault-mod`): `.\gradlew.bat build`
   with the target Minecraft version set in `gradle.properties`.
2. Copy `build/libs/<mod>-<version>+mc<mc>.jar` here.
3. Point the entry in `crates/faerie-modding/src/presets.rs` (`BUNDLED`) at
   the new file name and game version. The launcher embeds the bytes at
   compile time, so a rebuild is required.

The theme jar is ~10 MB (title-screen panoramas); that size lands in the
launcher binary.
