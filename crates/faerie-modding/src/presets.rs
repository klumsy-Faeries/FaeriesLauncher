//! Curated mod sets.
//!
//! A preset is a list of Modrinth projects with the reason each one is
//! there. Resolution against a Minecraft version happens at install time, so
//! the list never pins versions and keeps working as new game versions ship.
//! A mod with no build for the target version is *skipped and reported*, not
//! silently dropped and not substituted.

use std::collections::HashSet;

use serde::Serialize;

use crate::modrinth::{ModFile, ModrinthClient};
use crate::scan::LoaderKind;
use crate::ModError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetMod {
    pub slug: &'static str,
    pub name: &'static str,
    /// Why it is in the set — shown to the user, so plain language.
    pub reason: &'static str,
    /// Accept a beta/alpha build when no release exists for the version.
    /// Only set for mods whose pre-releases are the normal way to get the
    /// newest game version supported.
    pub prerelease_ok: bool,
}

/// A jar compiled into the launcher rather than fetched from Modrinth: the
/// Faeries companion mods, built from `apps/theme-mod` and `apps/vault-mod`
/// in the FaeriesClient repository and checked in under `mods/`. Each build
/// targets one game version; an instance on any other version skips it,
/// with the reason reported, rather than getting a build for the wrong game.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundledMod {
    pub id: &'static str,
    pub name: &'static str,
    pub reason: &'static str,
    pub file_name: &'static str,
    pub game_version: &'static str,
    #[serde(skip)]
    pub bytes: &'static [u8],
}

/// Where a bundled pack also lives on the network, so the Pack Vault can be
/// seeded with it: the server that announces it and the host it is fetched
/// from (`<url_base>/<sha1>`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultHint {
    pub server: &'static str,
    pub url_base: &'static str,
}

/// A resource pack carried by the launcher (`packs/` in the repository).
/// Installing the preset copies it into the instance's `resourcepacks/` and
/// enables it at the top of the pack stack in `options.txt`; with a
/// `vault` hint it is also stored in the Pack Vault under its SHA-1 so the
/// first join to that server needs no download.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundledPack {
    pub id: &'static str,
    pub name: &'static str,
    pub reason: &'static str,
    pub file_name: &'static str,
    pub vault: Option<VaultHint>,
    #[serde(skip)]
    pub bytes: &'static [u8],
}

/// A mod the set used to carry and now removes. When the preset is
/// installed into an instance whose profile holds a jar with this mod id,
/// the jar leaves the profile (the store keeps it) and the reason is
/// reported. This is how a mod that turned out to break the game is taken
/// out of instances that already have it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetiredMod {
    pub mod_id: &'static str,
    pub name: &'static str,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub loader: LoaderKind,
    pub mods: &'static [PresetMod],
    pub bundled: &'static [BundledMod],
    pub packs: &'static [BundledPack],
    pub retired: &'static [RetiredMod],
}

const fn m(
    slug: &'static str,
    name: &'static str,
    reason: &'static str,
    prerelease_ok: bool,
) -> PresetMod {
    PresetMod {
        slug,
        name,
        reason,
        prerelease_ok,
    }
}

/// The Fabric performance stack plus the rest of the Fabulously Optimized
/// mod set (14.1, Minecraft 26.2): OptiFine-style resource pack features,
/// shaders, and quality-of-life mods. Nothing here changes gameplay; every
/// entry is client-side or behaviour-preserving.
///
/// Left out of Fabulously Optimized on purpose: its two resource packs
/// (Chat Reporting Helper, Translations for Sodium — not mods), Config
/// Manager and Main Menu Credits (they only carry the pack's own config
/// files and title-screen credit, which do nothing here), and MixinTrace
/// Reborn (no 26.2 build; it only decorates crash reports).
pub static OPTIMIZED: Preset = Preset {
    id: "optimized",
    name: "Optimized",
    description: "Fabric with the same mods as Fabulously Optimized: a faster renderer, \
                  lighter game logic, lower memory use, shaders and OptiFine-style \
                  resource pack features, controller support, zoom, and other \
                  quality-of-life mods, plus the Faeries companion mods. No gameplay \
                  changes.",
    loader: LoaderKind::Fabric,
    bundled: FAERIES_MODS,
    packs: FAERIES_PACKS,
    retired: &[RetiredMod {
        mod_id: "krypton",
        name: "Krypton",
        reason: "Its build for Minecraft 26.2 fails a mixin on the login packet handler, \
                 which Controlify loads at startup, so the game crashed before the title \
                 screen. Fabulously Optimized does not carry it either.",
    }],
    mods: &[
        m(
            "fabric-api",
            "Fabric API",
            "Shared library most Fabric mods need.",
            false,
        ),
        m(
            "sodium",
            "Sodium",
            "Rewrites the renderer; the single biggest frame-rate gain.",
            true,
        ),
        m(
            "lithium",
            "Lithium",
            "Optimises game logic — AI, physics, ticking — without changing behaviour.",
            false,
        ),
        m(
            "ferrite-core",
            "FerriteCore",
            "Cuts the memory used by block states and models.",
            false,
        ),
        m(
            "immediatelyfast",
            "ImmediatelyFast",
            "Speeds up immediate-mode rendering: text, GUI, entities.",
            false,
        ),
        m(
            "entityculling",
            "Entity Culling",
            "Skips rendering entities you cannot see.",
            false,
        ),
        m(
            "dynamic-fps",
            "Dynamic FPS",
            "Idles the game while the window is unfocused or hidden.",
            false,
        ),
        m(
            "badoptimizations",
            "BadOptimizations",
            "Removes redundant work in lighting, time, and rendering.",
            false,
        ),
        m(
            "reeses-sodium-options",
            "Reese's Sodium Options",
            "Searchable video settings screen for Sodium.",
            false,
        ),
        m(
            "sodium-extra",
            "Sodium Extra",
            "Extra toggles for Sodium: animations, particles, fog.",
            false,
        ),
        // The rest of Fabulously Optimized. More performance:
        m(
            "better-block-entities",
            "Better Block Entities",
            "Renders chests, signs, and other block entities through Sodium's fast path.",
            true,
        ),
        m(
            "moreculling",
            "More Culling",
            "Culls more of what is hidden: leaves, item frames, block faces.",
            false,
        ),
        m(
            "modernfix-mvus",
            "ModernFix-mVUS",
            "Faster startup and less memory across many small fixes.",
            false,
        ),
        m(
            "ixeris",
            "Ixeris",
            "Polls input on its own thread so the mouse stays smooth when frames dip.",
            false,
        ),
        m(
            "language-reload",
            "Language Reload",
            "Faster resource reloads, and fallback languages for untranslated text.",
            false,
        ),
        // OptiFine-style resource pack features and looks:
        m(
            "iris",
            "Iris Shaders",
            "Loads OptiFine-format shader packs; off until you pick one.",
            false,
        ),
        m(
            "continuity",
            "Continuity",
            "Connected textures for glass, bookshelves, and packs that use them.",
            false,
        ),
        m(
            "entitytexturefeatures",
            "Entity Texture Features",
            "Random, emissive, and custom entity textures from resource packs.",
            false,
        ),
        m(
            "entity-model-features",
            "Entity Model Features",
            "Custom entity models from resource packs, OptiFine format.",
            false,
        ),
        m(
            "animaticarefabricated",
            "Animatica",
            "Animated textures in the OptiFine format.",
            false,
        ),
        m(
            "bettergrassify",
            "BetterGrassify",
            "Grass and paths wrap down the side of the block, like OptiFine's Better Grass.",
            false,
        ),
        m(
            "skyboxify",
            "Skyboxify",
            "Custom skies from resource packs, OptiFine format.",
            false,
        ),
        m(
            "polytone",
            "Polytone",
            "Lets resource packs recolour biomes, blocks, maps, and dyes.",
            false,
        ),
        m(
            "optigui",
            "OptiGUI",
            "Custom container GUI textures from resource packs.",
            true,
        ),
        m(
            "puzzle",
            "Puzzle",
            "One settings screen for the resource-pack feature mods.",
            false,
        ),
        m(
            "sodium-shadowy-path-blocks",
            "Sodium Shadowy Path Blocks",
            "Restores vanilla shading on paths and other partial blocks under Sodium.",
            false,
        ),
        m(
            "lambdynamiclights",
            "LambDynamicLights",
            "Held torches, glowing items, and burning mobs light their surroundings.",
            false,
        ),
        m(
            "cape-provider",
            "Cape Provider",
            "Shows capes from OptiFine, MinecraftCapes, and other providers.",
            false,
        ),
        // Quality of life:
        m(
            "modmenu",
            "Mod Menu",
            "Lists installed mods and opens their settings.",
            false,
        ),
        m(
            "zoomify",
            "Zoomify",
            "A zoom key, with scroll-to-zoom and smoothing.",
            false,
        ),
        m(
            "controlify",
            "Controlify",
            "Full controller support with on-screen button prompts.",
            false,
        ),
        m(
            "cubes-without-borders",
            "Cubes Without Borders",
            "Borderless fullscreen, so alt-tab is instant.",
            false,
        ),
        m(
            "fastquit",
            "FastQuit",
            "Back to the title screen while the world saves in the background.",
            false,
        ),
        m(
            "rrls",
            "Remove Reloading Screen",
            "Resource packs load in the background instead of behind a blocking screen.",
            true,
        ),
        m(
            "morechathistory",
            "More Chat History",
            "Keeps far more chat lines than the vanilla limit.",
            false,
        ),
        m(
            "paginatedadvancements",
            "Paginated Advancements",
            "A tidier advancements screen with pages and custom frames.",
            false,
        ),
        m(
            "better-mount-hud",
            "Better Mount HUD",
            "Shows your own hunger and experience while riding.",
            false,
        ),
        m(
            "renice-shot",
            "Renice Shot",
            "Takes screenshots at a higher resolution than the window.",
            false,
        ),
        m(
            "debugify",
            "Debugify",
            "Fixes vanilla bugs from the bug tracker that are still open.",
            false,
        ),
        m(
            "e4mc",
            "e4mc",
            "Opens a LAN world to friends over the internet, no port forwarding.",
            false,
        ),
        m(
            "no-chat-reports",
            "No Chat Reports",
            "Turns off chat signing where the server allows it, as Fabulously Optimized ships.",
            false,
        ),
        m(
            "crash-assistant",
            "Crash Assistant",
            "After a crash, shows the logs and what likely caused it.",
            false,
        ),
        // Libraries the mods above declare as dependencies. Listed so a
        // missing build is reported by name rather than as a bare slug.
        m(
            "cloth-config",
            "Cloth Config",
            "Settings library for FastQuit, More Culling, and others.",
            false,
        ),
        m(
            "yacl",
            "YetAnotherConfigLib",
            "Settings library for Zoomify, Controlify, and Skyboxify.",
            false,
        ),
        m(
            "fabric-language-kotlin",
            "Fabric Language Kotlin",
            "Kotlin runtime for Zoomify and OptiGUI.",
            false,
        ),
        m(
            "forge-config-api-port",
            "Forge Config API Port",
            "Config library Remove Reloading Screen reads its settings through.",
            false,
        ),
        m(
            "placeholder-api",
            "Placeholder API",
            "Text library Mod Menu needs.",
            false,
        ),
    ],
};

/// The Faeries companion mods (see `mods/README.md`). Both are client-side.
///
/// `static`, not `const`: a `const` holding `include_bytes!` is copied into
/// every use site, which doubled the launcher binary.
pub static FAERIES_MODS: &[BundledMod] = &[
    BundledMod {
        id: "faeries-theme",
        name: "Faeries Theme",
        reason: "Faeries logo as the window icon, loading screen, title panorama, and menu \
                 buttons. Cosmetic only.",
        file_name: "faeries-theme-0.1.0+mc26.2.jar",
        game_version: "26.2",
        bytes: include_bytes!("../../../mods/faeries-theme-0.1.0+mc26.2.jar"),
    },
    BundledMod {
        id: "faeries-vault",
        name: "Faeries Pack Vault",
        reason: "Keeps server resource packs stored locally so joining is instant; \
                 verifies by hash and falls back to a normal download.",
        file_name: "faeries-vault-0.1.0+mc26.2.jar",
        game_version: "26.2",
        bytes: include_bytes!("../../../mods/faeries-vault-0.1.0+mc26.2.jar"),
    },
];

/// Resource packs enabled by default (see `packs/README.md`). Order matters:
/// each is enabled on top of the previous one, so the last entry wins.
pub static FAERIES_PACKS: &[BundledPack] = &[
    BundledPack {
        id: "faeries-smp",
        name: "Faeries SMP pack",
        reason: "The server's own item and block art, so custom items look right \
                 everywhere and joining needs no download.",
        file_name: "faeries-smp.zip",
        vault: Some(VaultHint {
            server: "mc.faeriessmp.com",
            url_base: "https://hermes.nexomc.com/pack",
        }),
        bytes: include_bytes!("../../../packs/faeries-smp.zip"),
    },
    BundledPack {
        id: "fairy-castle-gui",
        name: "Fairy Castle GUI",
        reason: "Pastel HUD and menus that match the launcher's castle look.",
        file_name: "fairy-castle-gui.zip",
        vault: None,
        bytes: include_bytes!("../../../packs/fairy-castle-gui.zip"),
    },
];

pub static ALL: &[&Preset] = &[&OPTIMIZED];

pub fn by_id(id: &str) -> Option<&'static Preset> {
    ALL.iter().copied().find(|p| p.id == id)
}

/// A preset mod that could not be resolved, and why.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub name: String,
    pub reason: String,
}

/// What installing a preset against one game version would put in place:
/// downloads from Modrinth, jars carried by the launcher, and what was left
/// out and why.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub files: Vec<ModFile>,
    pub bundled: Vec<BundledMod>,
    pub packs: Vec<BundledPack>,
    pub skipped: Vec<Skipped>,
}

/// Dependency resolution rounds. Real chains are one level (mod → Fabric
/// API); the bound only guards against a cycle in bad metadata.
const MAX_DEPENDENCY_ROUNDS: usize = 3;

/// Resolve every mod in the preset, then any required dependencies they
/// declare that the preset did not already cover. A mod with no build for
/// the version is reported in `skipped`; a network failure is an error,
/// because "everything was skipped" would be a misleading plan.
pub async fn plan(
    client: &ModrinthClient,
    preset: &Preset,
    game_version: &str,
) -> Result<Plan, ModError> {
    let mut plan = Plan::default();
    let mut have: HashSet<String> = HashSet::new();

    // Resource packs are not tied to a game version; they always go in.
    plan.packs = preset.packs.to_vec();

    // Bundled jars need no network, but they are version-specific.
    for bundled in preset.bundled {
        if bundled.game_version == game_version {
            plan.bundled.push(bundled.clone());
        } else {
            plan.skipped.push(Skipped {
                name: bundled.name.to_string(),
                reason: format!(
                    "the bundled build is for Minecraft {}, not {game_version}",
                    bundled.game_version
                ),
            });
        }
    }

    for entry in preset.mods {
        match client
            .resolve(entry.slug, game_version, preset.loader, entry.prerelease_ok)
            .await?
        {
            Some(file) => {
                have.insert(file.project_id.clone());
                plan.files.push(file);
            }
            None => plan.skipped.push(Skipped {
                name: entry.name.to_string(),
                reason: format!(
                    "no {} build for Minecraft {game_version} yet",
                    preset.loader.display()
                ),
            }),
        }
    }

    for _ in 0..MAX_DEPENDENCY_ROUNDS {
        let wanted: Vec<String> = plan
            .files
            .iter()
            .flat_map(|f| f.required.iter().cloned())
            .filter(|id| !have.contains(id))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if wanted.is_empty() {
            break;
        }
        for project_id in wanted {
            // Mark first so an unresolvable dependency is not retried every round.
            have.insert(project_id.clone());
            let slug = client.slug_of(&project_id).await?;
            match client
                .resolve(&slug, game_version, preset.loader, true)
                .await?
            {
                Some(file) => plan.files.push(file),
                None => plan.skipped.push(Skipped {
                    name: slug,
                    reason: format!(
                        "required by another mod but has no {} build for {game_version}",
                        preset.loader.display()
                    ),
                }),
            }
        }
    }

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimized_preset_is_well_formed() {
        let slugs: HashSet<&str> = OPTIMIZED.mods.iter().map(|m| m.slug).collect();
        assert_eq!(slugs.len(), OPTIMIZED.mods.len(), "duplicate slug");
        assert!(
            slugs.contains("fabric-api"),
            "the shared library must be explicit"
        );
        assert!(slugs.contains("sodium") && slugs.contains("lithium"));
        let names: HashSet<&str> = OPTIMIZED.mods.iter().map(|m| m.name).collect();
        assert_eq!(names.len(), OPTIMIZED.mods.len(), "duplicate name");
        for entry in OPTIMIZED.mods {
            assert!(!entry.reason.is_empty(), "{} needs a reason", entry.slug);
            assert!(
                entry.reason.ends_with('.'),
                "{}: reasons are sentences",
                entry.slug
            );
        }
        for retired in OPTIMIZED.retired {
            assert!(
                !slugs.contains(retired.mod_id),
                "{} is both listed and retired",
                retired.mod_id
            );
            assert!(
                retired.reason.ends_with('.'),
                "{}: reasons are sentences",
                retired.name
            );
        }
        assert!(
            OPTIMIZED.retired.iter().any(|r| r.mod_id == "krypton"),
            "Krypton crashes 26.2 with Controlify; it must stay retired"
        );
        assert_eq!(by_id("optimized").map(|p| p.name), Some("Optimized"));
        assert!(by_id("nope").is_none());
    }

    #[test]
    fn bundled_faeries_mods_are_real_jars_for_one_version() {
        let ids: HashSet<&str> = FAERIES_MODS.iter().map(|m| m.id).collect();
        assert_eq!(ids.len(), FAERIES_MODS.len(), "duplicate bundled id");
        for m in FAERIES_MODS {
            assert!(m.file_name.ends_with(".jar"), "{}", m.file_name);
            assert!(
                m.file_name.contains(&format!("+mc{}", m.game_version)),
                "{} must carry the game version it was built for",
                m.file_name
            );
            // A jar is a zip: "PK" magic. Catches a bad include path early.
            assert_eq!(&m.bytes[..2], b"PK", "{} is not a jar", m.file_name);
            assert!(m.reason.ends_with('.'));
        }
        assert!(
            OPTIMIZED.bundled.len() >= 2,
            "theme and vault ship by default"
        );
    }

    #[test]
    fn bundled_packs_are_zips_with_unique_ids() {
        let ids: HashSet<&str> = FAERIES_PACKS.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), FAERIES_PACKS.len(), "duplicate pack id");
        for pack in FAERIES_PACKS {
            assert!(pack.file_name.ends_with(".zip"), "{}", pack.file_name);
            assert_eq!(&pack.bytes[..2], b"PK", "{} is not a zip", pack.file_name);
            assert!(pack.reason.ends_with('.'));
        }
    }

    /// Pre-release fallback is opt-in per mod: only the ones Fabulously
    /// Optimized itself ships as betas on a new game version.
    #[test]
    fn only_mods_that_ship_as_betas_accept_prereleases() {
        let loose: Vec<&str> = OPTIMIZED
            .mods
            .iter()
            .filter(|m| m.prerelease_ok)
            .map(|m| m.slug)
            .collect();
        assert_eq!(
            loose,
            ["sodium", "better-block-entities", "optigui", "rrls"]
        );
    }
}
