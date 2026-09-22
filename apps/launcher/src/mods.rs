//! Mod-related IPC commands (Phase 4).
//!
//! The app layer stays thin: it locates directories, calls `faerie-modding`,
//! and translates errors to strings. All modding knowledge lives in the crate.

use std::path::{Path, PathBuf};

use faerie_core::Event;
use faerie_modding::compat::{CompatReport, Environment};
use faerie_modding::loader::{self, LoaderVersion};
use faerie_modding::modrinth::ModrinthClient;
use faerie_modding::presets;
use faerie_modding::scan::{self, LoaderKind};
use faerie_modding::store::{self, ModProfile, ModStore, ProfileMod, ProfileSet};
use faerie_net::download::{DownloadConfig, DownloadRequest, Downloader};
use serde::Serialize;
use tauri::State;
use tokio_util::sync::CancellationToken;

use crate::state::AppState;

/// The shared, content-addressed mod store lives beside the other shared
/// game data so every instance and profile references one copy of a jar.
fn mod_store(state: &AppState) -> ModStore {
    ModStore::new(state.paths.data_dir.join("mod-store"))
}

fn instance_dir(state: &AppState, id: &str) -> Result<PathBuf, String> {
    state
        .instances
        .get(id)
        .map(|i| i.dir)
        .map_err(|e| e.to_string())
}

/// One mod as the UI shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModEntry {
    pub file_name: String,
    pub mod_id: String,
    pub name: String,
    pub version: String,
    pub loader: LoaderKind,
    pub description: String,
    pub authors: Vec<String>,
    pub enabled: bool,
    pub size: u64,
    /// SHA-1, present when the mod is tracked in the store.
    pub sha1: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModsView {
    pub instance_id: String,
    pub active_profile: String,
    pub profiles: Vec<ProfileSummary>,
    pub mods: Vec<ModEntry>,
    pub report: CompatReport,
    /// Jars that could not be read at all.
    pub problems: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub key: String,
    pub name: String,
    pub mod_count: usize,
    pub enabled_count: usize,
}

fn summarize(set: &ProfileSet) -> Vec<ProfileSummary> {
    set.profiles
        .iter()
        .map(|(key, profile)| ProfileSummary {
            key: key.clone(),
            name: profile.name.clone(),
            mod_count: profile.mods.len(),
            enabled_count: profile.mods.iter().filter(|m| m.enabled).count(),
        })
        .collect()
}

/// Scan an instance's mods and check them against its configured loader.
#[tauri::command]
pub async fn list_mods(state: State<'_, AppState>, id: String) -> Result<ModsView, String> {
    let instance = state.instances.get(&id).map_err(|e| e.to_string())?;
    let mods_dir = instance.dir.join("mods");
    let (scanned, problems) = scan::scan_directory(&mods_dir).await;

    let set = store::load_profiles(&instance.dir).map_err(|e| e.to_string())?;
    // Map file name -> hash so the UI can act on store entries.
    let active = set.profiles.get(&set.active);
    let hash_of = |file_name: &str| -> Option<String> {
        active.and_then(|p| {
            p.mods
                .iter()
                .find(|m| m.file_name == file_name)
                .map(|m| m.sha1.clone())
        })
    };

    let (loader, loader_version) = match &instance.config.loader {
        Some(l) => (
            LoaderKind::from_id(&l.kind).unwrap_or(LoaderKind::Unknown),
            l.version.clone(),
        ),
        // A vanilla instance has no loader; mods cannot load at all, which
        // the compatibility report will say plainly.
        None => (LoaderKind::Unknown, String::new()),
    };
    let env = Environment {
        loader,
        loader_version,
        minecraft_version: instance.config.minecraft_version.clone(),
    };
    let report = faerie_modding::check(&env, &scanned);

    let mods = scanned
        .into_iter()
        .map(|m| ModEntry {
            sha1: hash_of(m.file_name.trim_end_matches(".disabled")),
            enabled: !m.disabled,
            file_name: m.file_name,
            mod_id: m.metadata.mod_id,
            name: m.metadata.name,
            version: m.metadata.version,
            loader: m.metadata.loader,
            description: m.metadata.description,
            authors: m.metadata.authors,
            size: m.size,
            warnings: m.warnings,
        })
        .collect();

    Ok(ModsView {
        instance_id: id,
        active_profile: set.active.clone(),
        profiles: summarize(&set),
        mods,
        report,
        problems,
    })
}

/// Add jars to the instance's active profile, copying them into the store.
#[tauri::command]
pub async fn add_mods(
    state: State<'_, AppState>,
    id: String,
    paths: Vec<String>,
) -> Result<usize, String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    add_jars(state.inner(), &id, &paths)
}

/// Copy jars into the store and enable them in the instance's active
/// profile. Shared by manual adds and preset installs so both take the same
/// verified path; returns how many were new to the profile.
fn add_jars(state: &AppState, id: &str, paths: &[PathBuf]) -> Result<usize, String> {
    let dir = instance_dir(state, id)?;
    let store = mod_store(state);
    let mut set = store::load_profiles(&dir).map_err(|e| e.to_string())?;
    let active = set.active.clone();
    let profile = set
        .profiles
        .get_mut(&active)
        .ok_or_else(|| format!("profile `{active}` no longer exists"))?;

    // Which mod each jar already in the profile is, so a newer build of a
    // mod replaces the older one instead of sitting next to it (a loader
    // refuses to start with two builds of one mod).
    let mut ids = store::mod_ids(&store, profile);
    let mut added = 0;
    for path in paths {
        let Some(file_name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if !file_name.ends_with(".jar") {
            return Err(format!("{file_name} is not a .jar file"));
        }
        let sha1 = store.add(path).map_err(|e| e.to_string())?;
        // Re-adding a mod already in the profile just re-enables it.
        if let Some(existing) = profile.mods.iter_mut().find(|m| m.sha1 == sha1) {
            existing.enabled = true;
            continue;
        }
        // Only a jar with a known id can replace anything: an unreadable
        // descriptor must never make two unknowns look like one mod.
        let mod_id = scan::scan_jar(&store.path_for(&sha1))
            .ok()
            .map(|s| s.metadata.mod_id)
            .filter(|id| !id.is_empty());
        if let Some(mod_id) = mod_id {
            for old in store::supersede(profile, &ids, &sha1, &mod_id) {
                tracing::info!("{file_name} replaces {old} ({mod_id}) in instance {id}");
            }
            ids.insert(sha1.clone(), mod_id);
        }
        profile.mods.push(ProfileMod {
            sha1,
            file_name,
            enabled: true,
        });
        added += 1;
    }

    let profile = profile.clone();
    store::save_profiles(&dir, &set).map_err(|e| e.to_string())?;
    store::apply_profile(&store, &profile, &dir.join("mods")).map_err(|e| e.to_string())?;
    Ok(added)
}

/// Enable or disable one mod. Never deletes the jar (§13).
#[tauri::command]
pub async fn set_mod_enabled(
    state: State<'_, AppState>,
    id: String,
    file_name: String,
    enabled: bool,
) -> Result<(), String> {
    let dir = instance_dir(&state, &id)?;
    let store = mod_store(&state);
    let mut set = store::load_profiles(&dir).map_err(|e| e.to_string())?;
    let active = set.active.clone();
    let profile = set
        .profiles
        .get_mut(&active)
        .ok_or_else(|| format!("profile `{active}` no longer exists"))?;

    let target = file_name.trim_end_matches(".disabled");
    let entry = profile
        .mods
        .iter_mut()
        .find(|m| m.file_name == target)
        .ok_or_else(|| format!("{target} is not part of the active profile"))?;
    entry.enabled = enabled;

    let profile = profile.clone();
    store::save_profiles(&dir, &set).map_err(|e| e.to_string())?;
    store::apply_profile(&store, &profile, &dir.join("mods")).map_err(|e| e.to_string())?;
    Ok(())
}

/// Remove a mod from the profile. The jar stays in the shared store, so
/// re-adding it later costs no download (§13).
#[tauri::command]
pub async fn remove_mod(
    state: State<'_, AppState>,
    id: String,
    file_name: String,
) -> Result<(), String> {
    let dir = instance_dir(&state, &id)?;
    let store = mod_store(&state);
    let mut set = store::load_profiles(&dir).map_err(|e| e.to_string())?;
    let active = set.active.clone();
    let profile = set
        .profiles
        .get_mut(&active)
        .ok_or_else(|| format!("profile `{active}` no longer exists"))?;

    let target = file_name.trim_end_matches(".disabled").to_string();
    profile.mods.retain(|m| m.file_name != target);

    let profile = profile.clone();
    store::save_profiles(&dir, &set).map_err(|e| e.to_string())?;
    store::apply_profile(&store, &profile, &dir.join("mods")).map_err(|e| e.to_string())?;
    Ok(())
}

/// Adopt whatever is already sitting in `mods/` into the active profile.
#[tauri::command]
pub async fn import_existing_mods(state: State<'_, AppState>, id: String) -> Result<usize, String> {
    let dir = instance_dir(&state, &id)?;
    let store = mod_store(&state);
    let imported = store::import_existing(&store, &dir.join("mods")).map_err(|e| e.to_string())?;
    let count = imported.len();

    let mut set = store::load_profiles(&dir).map_err(|e| e.to_string())?;
    let active = set.active.clone();
    let profile = set
        .profiles
        .get_mut(&active)
        .ok_or_else(|| format!("profile `{active}` no longer exists"))?;
    for entry in imported {
        if !profile.mods.iter().any(|m| m.sha1 == entry.sha1) {
            profile.mods.push(entry);
        }
    }
    store::save_profiles(&dir, &set).map_err(|e| e.to_string())?;
    Ok(count)
}

// ---- Profiles (§14) ----

#[tauri::command]
pub async fn create_mod_profile(
    state: State<'_, AppState>,
    id: String,
    name: String,
    copy_from_active: bool,
) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("a profile name cannot be empty".into());
    }
    let dir = instance_dir(&state, &id)?;
    let mut set = store::load_profiles(&dir).map_err(|e| e.to_string())?;

    let key = slug(&name, &set);
    let mods = if copy_from_active {
        set.profiles
            .get(&set.active)
            .map(|p| p.mods.clone())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    set.profiles.insert(key.clone(), ModProfile { name, mods });
    store::save_profiles(&dir, &set).map_err(|e| e.to_string())?;
    Ok(key)
}

/// Switch profiles: a manifest swap plus re-linking, so this is fast even
/// for hundreds of mods (§14).
#[tauri::command]
pub async fn activate_mod_profile(
    state: State<'_, AppState>,
    id: String,
    profile: String,
) -> Result<(), String> {
    let dir = instance_dir(&state, &id)?;
    let store = mod_store(&state);
    let mut set = store::load_profiles(&dir).map_err(|e| e.to_string())?;
    let target = set
        .profiles
        .get(&profile)
        .cloned()
        .ok_or_else(|| format!("no profile named `{profile}`"))?;
    set.active = profile;
    store::save_profiles(&dir, &set).map_err(|e| e.to_string())?;
    store::apply_profile(&store, &target, &dir.join("mods")).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn delete_mod_profile(
    state: State<'_, AppState>,
    id: String,
    profile: String,
) -> Result<(), String> {
    let dir = instance_dir(&state, &id)?;
    let mut set = store::load_profiles(&dir).map_err(|e| e.to_string())?;
    if set.profiles.len() <= 1 {
        return Err("an instance must keep at least one mod profile".into());
    }
    if set.active == profile {
        return Err("switch to another profile before deleting this one".into());
    }
    set.profiles
        .remove(&profile)
        .ok_or_else(|| format!("no profile named `{profile}`"))?;
    store::save_profiles(&dir, &set).map_err(|e| e.to_string())?;
    Ok(())
}

// ---- Loaders ----

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderOption {
    pub kind: LoaderKind,
    pub display: &'static str,
    /// False when this launcher can list versions but not yet install them.
    pub installable: bool,
}

#[tauri::command]
pub fn supported_loaders() -> Vec<LoaderOption> {
    loader::supported_loaders()
        .into_iter()
        .map(|kind| LoaderOption {
            kind,
            display: kind.display(),
            installable: matches!(kind, LoaderKind::Fabric | LoaderKind::Quilt),
        })
        .collect()
}

#[tauri::command]
pub async fn loader_versions(
    state: State<'_, AppState>,
    loader: String,
    minecraft_version: String,
) -> Result<Vec<LoaderVersion>, String> {
    let kind = LoaderKind::from_id(&loader).ok_or_else(|| format!("unknown loader `{loader}`"))?;
    let adapter = loader::adapter_for(kind, state.http.clone())
        .ok_or_else(|| format!("no adapter for `{loader}`"))?;
    adapter
        .versions_for(&minecraft_version)
        .await
        .map_err(|e| e.to_string())
}

/// Install a loader into an instance: write its version JSON and record the
/// choice. The next launch installs and runs it through the normal pipeline.
#[tauri::command]
pub async fn install_loader(
    state: State<'_, AppState>,
    id: String,
    loader: String,
    loader_version: String,
) -> Result<String, String> {
    let instance = state.instances.get(&id).map_err(|e| e.to_string())?;
    let kind = LoaderKind::from_id(&loader).ok_or_else(|| format!("unknown loader `{loader}`"))?;
    let adapter = loader::adapter_for(kind, state.http.clone())
        .ok_or_else(|| format!("no adapter for `{loader}`"))?;

    let versions_dir = state.paths.data_dir.join("versions");
    let installed = adapter
        .install(
            &instance.config.minecraft_version,
            &loader_version,
            &versions_dir,
        )
        .await
        .map_err(|e| e.to_string())?;

    state
        .instances
        .update(&id, |config| {
            config.loader = Some(faerie_instances::LoaderRef {
                kind: kind.id().to_string(),
                version: installed.loader_version.clone(),
            });
        })
        .map_err(|e| e.to_string())?;

    tracing::info!(
        "installed {} {} for instance {id} as version {}",
        kind.display(),
        loader_version,
        installed.version_id
    );
    Ok(installed.version_id)
}

/// Remove the loader from an instance, returning it to vanilla.
#[tauri::command]
pub async fn remove_loader(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .instances
        .update(&id, |config| config.loader = None)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// A curated mod set as the UI lists it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetDto {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub loader: String,
    pub mods: &'static [presets::PresetMod],
    pub bundled: &'static [presets::BundledMod],
    pub packs: &'static [presets::BundledPack],
    pub retired: &'static [presets::RetiredMod],
}

#[tauri::command]
pub fn list_mod_presets() -> Vec<PresetDto> {
    presets::ALL
        .iter()
        .map(|p| PresetDto {
            id: p.id,
            name: p.name,
            description: p.description,
            loader: p.loader.id().to_string(),
            mods: p.mods,
            bundled: p.bundled,
            packs: p.packs,
            retired: p.retired,
        })
        .collect()
}

/// What a preset install did: which jars landed, which mods had no build
/// for this version, and whether a loader had to be installed first.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetOutcome {
    pub installed: Vec<String>,
    /// Resource packs copied in and enabled in `options.txt`.
    pub packs: Vec<String>,
    pub skipped: Vec<presets::Skipped>,
    /// Jars of mods the set has retired, dropped from the profile.
    pub removed: Vec<String>,
    pub loader_installed: Option<String>,
}

/// Drop from the active profile every mod the preset has retired, matched
/// by mod id. The store keeps the jars. Returns the file names removed.
fn retire_mods(
    state: &AppState,
    id: &str,
    retired: &[presets::RetiredMod],
) -> Result<Vec<String>, String> {
    if retired.is_empty() {
        return Ok(Vec::new());
    }
    let dir = instance_dir(state, id)?;
    let store = mod_store(state);
    let mut set = store::load_profiles(&dir).map_err(|e| e.to_string())?;
    let active = set.active.clone();
    let profile = set
        .profiles
        .get_mut(&active)
        .ok_or_else(|| format!("profile `{active}` no longer exists"))?;
    let ids = store::mod_ids(&store, profile);
    let mut removed = Vec::new();
    for entry in retired {
        // An empty `keep` matches no jar, so every build of the id goes.
        for file in store::supersede(profile, &ids, "", entry.mod_id) {
            tracing::info!(
                "{file} removed from instance {id}: {} is retired from the set ({})",
                entry.name,
                entry.reason
            );
            removed.push(file);
        }
    }
    if !removed.is_empty() {
        let profile = profile.clone();
        store::save_profiles(&dir, &set).map_err(|e| e.to_string())?;
        store::apply_profile(&store, &profile, &dir.join("mods")).map_err(|e| e.to_string())?;
    }
    Ok(removed)
}

/// Install a curated mod set into an instance: put the preset's loader in
/// place if the instance does not have it, resolve each mod against the
/// instance's Minecraft version on Modrinth, download with hash
/// verification, and enable everything in the active profile. Mods without
/// a build for the version are reported, never substituted.
#[tauri::command]
pub async fn install_mod_preset(
    state: State<'_, AppState>,
    id: String,
    preset: String,
) -> Result<PresetOutcome, String> {
    let preset = presets::by_id(&preset).ok_or_else(|| format!("unknown mod preset `{preset}`"))?;
    let instance = state.instances.get(&id).map_err(|e| e.to_string())?;
    let minecraft_version = instance.config.minecraft_version.clone();
    // The game rewrites options.txt on exit, which would undo the pack
    // enablement below; mods dropped in mid-session would not load either.
    if state.running_game.lock().await.is_some() {
        return Err("Minecraft is running. Close it first, then add the set.".into());
    }

    let has_loader = instance
        .config
        .loader
        .as_ref()
        .is_some_and(|l| LoaderKind::from_id(&l.kind) == Some(preset.loader));
    let mut loader_installed = None;
    if !has_loader {
        let adapter = loader::adapter_for(preset.loader, state.http.clone())
            .ok_or_else(|| format!("no adapter for {}", preset.loader.display()))?;
        let versions = adapter
            .versions_for(&minecraft_version)
            .await
            .map_err(|e| e.to_string())?;
        let chosen = versions
            .iter()
            .find(|v| v.stable)
            .or(versions.first())
            .ok_or_else(|| {
                format!(
                    "{} has no loader for Minecraft {minecraft_version}",
                    preset.loader.display()
                )
            })?;
        let installed = adapter
            .install(
                &minecraft_version,
                &chosen.version,
                &state.paths.data_dir.join("versions"),
            )
            .await
            .map_err(|e| e.to_string())?;
        state
            .instances
            .update(&id, |config| {
                config.loader = Some(faerie_instances::LoaderRef {
                    kind: preset.loader.id().to_string(),
                    version: installed.loader_version.clone(),
                });
            })
            .map_err(|e| e.to_string())?;
        tracing::info!(
            "installed {} {} for instance {id} (preset {})",
            preset.loader.display(),
            installed.loader_version,
            preset.id
        );
        loader_installed = Some(installed.loader_version);
    }

    let plan = presets::plan(
        &ModrinthClient::new(state.http.clone()),
        preset,
        &minecraft_version,
    )
    .await
    .map_err(|e| e.to_string())?;

    // Downloads stage in the cache and are sha1-checked by the downloader;
    // the store then copies them under their hash like any manually added jar.
    // A jar the store already holds (another instance installed it) is
    // copied from there instead of fetched again.
    let staging = state.paths.cache_dir.join("mod-downloads");
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let store = mod_store(state.inner());
    let mut from_store = 0usize;
    for f in plan.files.iter().filter(|f| store.contains(&f.sha1)) {
        std::fs::copy(store.path_for(&f.sha1), staging.join(&f.file_name))
            .map_err(|e| format!("could not reuse stored {}: {e}", f.file_name))?;
        from_store += 1;
    }
    let requests: Vec<DownloadRequest> = plan
        .files
        .iter()
        .filter(|f| !store.contains(&f.sha1))
        .map(|f| DownloadRequest {
            url: f.url.clone(),
            dest: staging.join(&f.file_name),
            sha1: Some(f.sha1.clone()),
            size: Some(f.size),
            label: f.file_name.clone(),
        })
        .collect();
    let bus = state.bus.clone();
    let instance_id = id.clone();
    let outcome = Downloader::new(state.http.clone(), DownloadConfig::default())
        .fetch_batch(requests, &CancellationToken::new(), |p| {
            bus.emit(Event::InstallProgress {
                instance_id: instance_id.clone(),
                phase: "downloading mods".into(),
                files_done: p.files_done,
                files_total: p.files_total,
                bytes_done: p.bytes_done,
                bytes_total: p.bytes_total,
                bytes_per_sec: p.bytes_per_sec,
            });
        })
        .await;
    if !outcome.is_success() {
        let failed: Vec<String> = outcome
            .failed
            .iter()
            .map(|f| format!("{} ({})", f.label, f.error))
            .collect();
        return Err(format!(
            "{} mod download(s) failed: {}",
            failed.len(),
            failed.join("; ")
        ));
    }

    // Bundled jars are written from the launcher's own bytes into the same
    // staging folder so they take the identical path into the store.
    for bundled in &plan.bundled {
        let path = staging.join(bundled.file_name);
        std::fs::write(&path, bundled.bytes)
            .map_err(|e| format!("could not stage {}: {e}", bundled.file_name))?;
    }
    let staged: Vec<PathBuf> = plan
        .files
        .iter()
        .map(|f| staging.join(&f.file_name))
        .chain(plan.bundled.iter().map(|b| staging.join(b.file_name)))
        .collect();
    add_jars(state.inner(), &id, &staged)?;
    for path in &staged {
        remove_staged(path);
    }
    let removed = retire_mods(state.inner(), &id, preset.retired)?;

    // Instances made before the default server list existed catch up here.
    crate::defaults::apply_servers(&instance);

    // Resource packs: into the instance folder, then on top of the stack.
    let mut packs = Vec::new();
    for pack in &plan.packs {
        let dest = instance.dir.join("resourcepacks").join(pack.file_name);
        std::fs::create_dir_all(dest.parent().expect("resourcepacks dir"))
            .map_err(|e| e.to_string())?;
        std::fs::write(&dest, pack.bytes)
            .map_err(|e| format!("could not write {}: {e}", pack.file_name))?;
        faerie_instances::options::enable_resource_pack(
            &instance.dir,
            &faerie_instances::options::file_pack_entry(pack.file_name),
        )
        .map_err(|e| e.to_string())?;
        packs.push(pack.name.to_string());

        // Also seed the Pack Vault: the vault mod then serves this build
        // instantly when the server announces its hash. Best effort — a
        // vault problem must not fail the install.
        if let Some(hint) = &pack.vault {
            match vault_dir() {
                Some(dir) => {
                    let sha1 = faerie_net::hash::sha1_of_bytes(pack.bytes);
                    let url = format!("{}/{sha1}", hint.url_base.trim_end_matches('/'));
                    match faerie_modding::vault::seed(
                        &dir,
                        pack.bytes,
                        Some(&url),
                        Some(hint.server),
                        Some(pack.name),
                    ) {
                        Ok(seeded) => tracing::info!(
                            "pack vault: {} {} as {}",
                            pack.name,
                            if seeded.blob_written {
                                "stored"
                            } else {
                                "already present"
                            },
                            seeded.sha1
                        ),
                        Err(e) => tracing::warn!("pack vault: could not seed {}: {e}", pack.name),
                    }
                }
                None => tracing::warn!("pack vault: no data directory on this platform"),
            }
        }
    }
    tracing::info!(
        "preset {} installed {} mod(s) into instance {id} ({} downloaded, {} reused from the store, {} bundled) and {} resource pack(s), {} skipped",
        preset.id,
        plan.files.len() + plan.bundled.len(),
        plan.files.len() - from_store,
        from_store,
        plan.bundled.len(),
        packs.len(),
        plan.skipped.len()
    );

    let installed = plan
        .files
        .into_iter()
        .map(|f| f.file_name)
        .chain(plan.bundled.into_iter().map(|b| b.file_name.to_string()))
        .collect();
    Ok(PresetOutcome {
        installed,
        packs,
        skipped: plan.skipped,
        removed,
        loader_installed,
    })
}

/// Where the Faeries Pack Vault mod keeps packs (its FORMAT.md): `%APPDATA%`
/// on Windows, `~/Library/Application Support` on macOS, `$XDG_DATA_HOME` on
/// Linux — each with a `FaeriesVault` folder.
fn vault_dir() -> Option<PathBuf> {
    let base = if cfg!(target_os = "linux") {
        dirs::data_dir()
    } else {
        dirs::config_dir()
    };
    base.map(|b| b.join("FaeriesVault"))
}

/// The store holds its own copy, so the staged download is just clutter;
/// a failure to delete it is not worth failing the install over.
fn remove_staged(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        tracing::debug!("could not remove staged {}: {e}", path.display());
    }
}

/// Derive a unique key for a profile from its display name.
fn slug(name: &str, set: &ProfileSet) -> String {
    let mut base = String::new();
    let mut last_dash = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            base.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            base.push('-');
            last_dash = true;
        }
    }
    let base = base.trim_end_matches('-').to_string();
    let base = if base.is_empty() {
        "profile".to_string()
    } else {
        base
    };
    if !set.profiles.contains_key(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !set.profiles.contains_key(candidate))
        .expect("an unused suffix always exists")
}
