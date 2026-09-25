# Modding

How the launcher handles mod loaders, mod files, and compatibility.

## Loader support

| Loader | List versions | Install | Scan its mods |
|---|---|---|---|
| Fabric | ✅ | ✅ | ✅ |
| Quilt | ✅ | ✅ | ✅ |
| NeoForge | ✅ | ❌ | ✅ |
| Forge | ✅ | ❌ | ✅ |

Fabric and Quilt publish a ready-made *profile JSON* per (Minecraft, loader)
pair that layers onto vanilla through `inheritsFrom`. Installing them is
therefore: fetch that JSON, write it into the versions directory, and let the
normal version pipeline install and launch it.

Forge and NeoForge do not publish such a file. They ship an installer jar
containing `install_profile.json`, which declares a chain of **processors** —
Java programs (jar splitters, mapping mergers, binary patchers) that must run
in order, with `[maven:coordinate]` tokens resolved to local paths, to produce
the patched client. That pipeline is a subsystem of its own and its steps
change between versions, so it is not implemented yet. Installing them fails
with an explanation rather than producing an instance that installs cleanly
and then fails at launch.

Their **mods** are still fully understood: a Forge or NeoForge jar dropped
into any instance is scanned, its dependencies are checked, and a mod placed
in an instance whose loader cannot load it is reported clearly.

## The Optimized preset

A new instance starts as **Fabric + the Fabulously Optimized mod set**
unless you untick *Start optimized* (setup wizard and Instances page). An
existing instance gets the same set from **Mods → Add optimized mods**; a
vanilla instance has the newest stable Fabric loader installed first.

The list is the mods of [Fabulously Optimized](https://modrinth.com/modpack/fabulously-optimized)
14.1 (its Minecraft 26.2 release) plus BadOptimizations, which was in the
set before. It is grouped below as the preset lists it: the performance
stack, OptiFine-style resource pack features and looks, quality of life,
and the libraries those mods declare.

A preset can also **retire** mods (`retired` in `presets.rs`): installing
it removes any jar with that mod id from the instance's profile (the store
keeps the jar) and says so. Krypton is retired: its 26.2 build fails a
mixin on the login packet handler, which Controlify loads at startup, so
the game crashed before the title screen.

| Mod | Why it is in the set |
|---|---|
| Fabric API | Shared library most Fabric mods need |
| Sodium | Rewrites the renderer; the single biggest frame-rate gain |
| Lithium | Optimises game logic — AI, physics, ticking — without changing behaviour |
| FerriteCore | Cuts the memory used by block states and models |
| ImmediatelyFast | Speeds up immediate-mode rendering: text, GUI, entities |
| Entity Culling | Skips rendering entities you cannot see |
| Dynamic FPS | Idles the game while the window is unfocused or hidden |
| BadOptimizations | Removes redundant work in lighting, time, and rendering |
| Reese's Sodium Options | Searchable video settings screen for Sodium |
| Sodium Extra | Extra toggles for Sodium: animations, particles, fog |
| Better Block Entities | Renders chests, signs, and other block entities through Sodium's fast path |
| More Culling | Culls more of what is hidden: leaves, item frames, block faces |
| ModernFix-mVUS | Faster startup and less memory across many small fixes |
| Ixeris | Polls input on its own thread so the mouse stays smooth when frames dip |
| Language Reload | Faster resource reloads, and fallback languages for untranslated text |
| Iris Shaders | Loads OptiFine-format shader packs; off until you pick one |
| Continuity | Connected textures for glass, bookshelves, and packs that use them |
| Entity Texture Features | Random, emissive, and custom entity textures from resource packs |
| Entity Model Features | Custom entity models from resource packs, OptiFine format |
| Animatica | Animated textures in the OptiFine format |
| BetterGrassify | Grass and paths wrap down the side of the block, like OptiFine's Better Grass |
| Skyboxify | Custom skies from resource packs, OptiFine format |
| Polytone | Lets resource packs recolour biomes, blocks, maps, and dyes |
| OptiGUI | Custom container GUI textures from resource packs |
| Puzzle | One settings screen for the resource-pack feature mods |
| Sodium Shadowy Path Blocks | Restores vanilla shading on paths and other partial blocks under Sodium |
| LambDynamicLights | Held torches, glowing items, and burning mobs light their surroundings |
| Cape Provider | Shows capes from OptiFine, MinecraftCapes, and other providers |
| Mod Menu | Lists installed mods and opens their settings |
| Zoomify | A zoom key, with scroll-to-zoom and smoothing |
| Controlify | Full controller support with on-screen button prompts |
| Cubes Without Borders | Borderless fullscreen, so alt-tab is instant |
| FastQuit | Back to the title screen while the world saves in the background |
| Remove Reloading Screen | Resource packs load in the background instead of behind a blocking screen |
| More Chat History | Keeps far more chat lines than the vanilla limit |
| Paginated Advancements | A tidier advancements screen with pages and custom frames |
| Better Mount HUD | Shows your own hunger and experience while riding |
| Renice Shot | Takes screenshots at a higher resolution than the window |
| Debugify | Fixes vanilla bugs from the bug tracker that are still open |
| e4mc | Opens a LAN world to friends over the internet, no port forwarding |
| No Chat Reports | Turns off chat signing where the server allows it, as Fabulously Optimized ships |
| Crash Assistant | After a crash, shows the logs and what likely caused it |
| Cloth Config | Settings library for FastQuit, More Culling, and others |
| YetAnotherConfigLib | Settings library for Zoomify, Controlify, and Skyboxify |
| Fabric Language Kotlin | Kotlin runtime for Zoomify and OptiGUI |
| Forge Config API Port | Config library Remove Reloading Screen reads its settings through |
| Placeholder API | Text library Mod Menu needs |

Nothing in the set changes gameplay; everything is client-side or
behaviour-preserving, so it is safe on servers. Shaders (Iris) stay off
until you choose a pack, and No Chat Reports is the one entry with an
opinion: it disables chat signing where a server permits it, exactly as the
pack ships it, and can be disabled on the Mods page like any other mod.

Left out of Fabulously Optimized on purpose: its two **resource packs**
(Chat Reporting Helper, Translations for Sodium) are not mods; **Config
Manager** and **Main Menu Credits** only carry the pack's own config files
and title-screen credit, which do nothing outside the pack; and **MixinTrace
Reborn** has no 26.2 build and only decorates crash reports. When a 1.21
instance installs the set, mods without a build for that version are
reported as skipped, as always.

The set also carries two **bundled** mods compiled into the launcher (see
`mods/README.md`): **Faeries Theme** (Faeries logo as the window icon, the
loading screen, title-screen panorama, and menu buttons — cosmetic) and
**Faeries Pack Vault** (server resource packs kept locally so joining is
instant, hash-verified). Each bundled jar is built for one Minecraft version
and is installed only into instances on that version; elsewhere it is
reported as skipped.

The preset can also carry **resource packs** (`packs/` in the repository):
each is copied into the instance's `resourcepacks/` folder and enabled at
the top of the pack stack in `options.txt`, with every other line of that
file left untouched. Two ship today: the **Faeries SMP pack** (the server's
own art) and, on top of it, **Fairy Castle GUI** (pastel HUD and menus). Because the game rewrites `options.txt` when it exits,
the set cannot be added while Minecraft is running.

Every instance the launcher creates also starts with **Faeries SMP
(`faeriessmp.com`) on its multiplayer list**, with server resource packs set
to *Enabled* so the pack applies without a prompt on every join. The entry
is written to `servers.dat` (the game's own NBT format) alongside whatever
the player already has there — other servers, icons, and settings are
kept, and an existing entry for the address is never overridden. Older
instances pick it up when the Optimized preset is installed. The list lives
in `apps/launcher/src/defaults.rs`.

A pack that also lives on a server is **seeded into the Pack Vault** at the
same time (`%APPDATA%\FaeriesVault`, the folder the Faeries Pack Vault mod
reads): stored under its SHA-1 with the server and download URL recorded, so
the first join needs no download as long as the server still announces that
exact build.

How an install works:

1. Each mod is looked up on Modrinth for the instance's **exact Minecraft
   version and loader**. Release builds are preferred; only the mods
   Fabulously Optimized itself ships as betas on a new game version (Sodium,
   Better Block Entities, OptiGUI, Remove Reloading Screen) may fall back to
   one.
2. Required dependencies declared by those builds are resolved the same way
   and added (this is how Fabric API arrives even if you remove it from the
   list).
3. Jars download to `cache/mod-downloads/` with SHA-1 verification, are
   copied into the mod store, enabled in the active profile, and the staged
   copies are deleted.
4. A mod with **no build for the version** is skipped and named in the
   result. It is never replaced with a build for a different version.

Not included on purpose: C2ME and ScalableLux (alpha-only on current
versions), and Noisium and ThreadTweak (no build for the newest game
versions at the time of writing — worth adding once they ship one).
ModernFix arrives as the mVUS fork, which tracks new game versions.

`cargo test -p faerie-modding --test modrinth -- --ignored live_optimized_preset`
resolves the real list against Modrinth for Minecraft 26.2 and prints what
each mod would install; it fails if anything in the list has no build.

The list lives in `crates/faerie-modding/src/presets.rs`.

## Where mod files live

```
<data>/mod-store/<sha1>.jar        shared, content-addressed
<instance>/mod-profiles.json       which hashes each profile enables
<instance>/mods/                   materialized via hardlinks
```

Consequences:

- the same jar used by ten profiles occupies one copy on disk;
- switching profiles is a manifest swap plus re-linking — milliseconds, not
  a copy of hundreds of megabytes;
- **disabling never deletes.** A disabled mod is materialized as
  `name.jar.disabled`; removing it from a profile leaves the store copy
  intact, so re-adding costs no download.
- **a newer build replaces the older one.** Adding a jar whose mod id is
  already in the profile (a Sodium update, or the preset re-run after
  Modrinth shipped new builds) drops the older jar from the profile and
  logs the replacement, because Fabric refuses to start with two builds of
  one mod. The older jar stays in the store. A jar without readable
  metadata has no id and is never treated as a duplicate.

Hardlinks fall back to copies when the store and the instance are on
different volumes, which is correct but uses more disk.

## Mod profiles

Each instance has one or more profiles (§14) with independent mod sets. The
Mods page can duplicate the active profile, switch between them, and delete
any but the last. An instance always keeps at least one.

## Compatibility checks

Every check produces a finding with three parts (§47): what happened, why,
and how to fix it.

| Check | Severity |
|---|---|
| Required dependency missing | Error |
| Dependency present but wrong version | Error |
| Minecraft version outside the mod's range | Error |
| Explicit conflict (`breaks` / `incompatible`) with an enabled mod | Error |
| Same mod id installed twice | Error |
| Mod built for a different loader | Error |
| Optional dependency version mismatch | Warning |
| Jar with no recognizable descriptor | Warning |
| Requirement the launcher could not parse | Info |

Rules the engine follows:

- **Disabled mods are inert.** They cannot satisfy a dependency and cannot
  cause a conflict.
- **Bundled jars count.** A jar can carry other jars ("Jar-in-Jar": Fabric
  and Quilt list them under `jars`, Forge and NeoForge in
  `META-INF/jarjar/metadata.json`), and the loader loads those too. Fabric
  API is forty-odd such modules in one jar, and Sodium carries its own
  copies of the ones it needs. The scanner reads them at any depth, so a
  dependency on `fabric-resource-loader-v0` is satisfied by Fabric API
  being installed. Each bundled mod counts at its own version; when several
  jars carry the same library, the newest copy is the one considered, as the
  loader does. Bundled jars are not listed as installed mods and are never
  duplicates of a top-level one.
- **Root cause only.** A mod for the wrong loader reports exactly that, not
  the pile of missing dependencies that follows from it.
- **Never claim an unproven conflict.** A version requirement the launcher
  cannot parse is reported as unparsed and treated as satisfied, because a
  wrong "incompatible" verdict is worse than an honest "could not check".
- **Quilt accepts Fabric mods**, but not the reverse.

## Version range syntax

Both ecosystems' dialects are supported.

| Syntax | Meaning | Used by |
|---|---|---|
| `*`, `any` | anything | both |
| `1.20.1` | exactly (Fabric) / at least (Forge) | both |
| `>=1.20`, `<1.21`, `>1.0`, `<=2.0` | comparison | Fabric/Quilt |
| `>=1.20 <1.21` | all of | Fabric/Quilt |
| `1.20.1 \|\| 1.20.2` | any of | Fabric/Quilt |
| `^1.2.3` | `>=1.2.3, <2.0.0` | Fabric/Quilt |
| `~1.2.3` | `>=1.2.3, <1.3.0` | Fabric/Quilt |
| `1.20.x` | `>=1.20, <1.21` | Fabric/Quilt |
| `[1.20,1.21)` | interval, `[`/`]` inclusive, `(`/`)` exclusive | Forge/NeoForge |
| `[47,)` | 47 or newer | Forge/NeoForge |

Versions compare numerically per component (`1.10 > 1.9`), missing components
count as zero (`1.2` equals `1.2.0`), and a pre-release sorts before its
release (`1.0.0-beta < 1.0.0`). Pre-release identifiers compare the semver
way (`beta.2 < beta.10 < rc`). Build metadata after `+` is not part of the
version: `0.9.1+mc26.2` satisfies `>=0.9.1`. A trailing dash is Fabric's
"this version or any pre-release of it": `~26.2-` accepts `26.2-rc.1` and
`26.2`.
