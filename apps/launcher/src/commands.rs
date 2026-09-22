//! IPC surface exposed to the webview. Commands stay thin: validate at the
//! crate boundary, translate errors to strings, emit events.

use std::collections::HashMap;
use std::path::PathBuf;

use faerie_auth::device_code::DeviceCodePrompt;
use faerie_auth::store::AccountRecord;
use faerie_auth::{AuthFlow, Endpoints};
use faerie_core::config::{self, RecoveryNotice, SettingDef};
use faerie_core::Event;
use faerie_minecraft::launch::Session;
use serde::Serialize;
use serde_json::Value;
use tauri::State;
use tokio_util::sync::CancellationToken;

use crate::launching::{self, PlayRequest};
use crate::state::AppState;
use crate::themes::{self, ThemeDto};

#[tauri::command]
pub fn settings_schema() -> Vec<SettingDef> {
    config::settings().to_vec()
}

#[tauri::command]
pub fn settings_values(state: State<'_, AppState>) -> HashMap<String, Value> {
    state.config.read().unwrap().values()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetOutcome {
    pub restart_required: bool,
}

#[tauri::command]
pub fn set_setting(
    state: State<'_, AppState>,
    id: String,
    value: Value,
) -> Result<SetOutcome, String> {
    let mut cfg = state.config.write().unwrap();
    let def = cfg.set(&id, value.clone()).map_err(|e| e.to_string())?;
    cfg.save_file(def.file).map_err(|e| e.to_string())?;
    // Never log the value of a credential-ish setting.
    if def.id == "accounts.client_id" {
        tracing::info!("setting {id} changed");
    } else {
        tracing::info!("setting {id} changed to {value}");
    }
    state.bus.emit(Event::SettingChanged {
        id,
        value,
        restart_required: def.restart_required,
    });
    Ok(SetOutcome {
        restart_required: def.restart_required,
    })
}

#[tauri::command]
pub fn recovery_notices(state: State<'_, AppState>) -> Vec<RecoveryNotice> {
    state.config.read().unwrap().notices().to_vec()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: &'static str,
    pub data_dir: String,
    pub logs_dir: String,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        data_dir: state.paths.root.display().to_string(),
        logs_dir: state.paths.logs_dir.display().to_string(),
    }
}

#[tauri::command]
pub fn list_themes(state: State<'_, AppState>) -> Vec<String> {
    themes::list(&state.paths)
}

#[tauri::command]
pub fn get_theme(state: State<'_, AppState>, name: String) -> ThemeDto {
    let overrides = state
        .config
        .read()
        .unwrap()
        .get_str("ui.layout_overrides")
        .unwrap_or_default();
    themes::load(&state.paths, &name, &overrides)
}

/// Reveal a launcher directory in the OS file manager (§28: never hide
/// where user files live). Only launcher-owned directories are accepted,
/// so this cannot be turned into "open anything on disk".
#[tauri::command]
pub fn open_folder(state: State<'_, AppState>, which: String) -> Result<(), String> {
    let paths = &state.paths;
    let target = match which.as_str() {
        "data" => paths.root.clone(),
        "logs" => paths.logs_dir.clone(),
        "themes" => paths.themes_dir.clone(),
        "instances" => paths.instances_dir.clone(),
        "config" => paths.config_dir.clone(),
        other => return Err(format!("unknown folder `{other}`")),
    };
    if !target.is_dir() {
        return Err(format!("{} does not exist", target.display()));
    }

    #[cfg(windows)]
    let mut command = {
        let mut c = std::process::Command::new("explorer");
        c.arg(&target);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = std::process::Command::new("open");
        c.arg(&target);
        c
    };
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&target);
        c
    };

    // `explorer` returns a non-zero exit code even on success, so only a
    // spawn failure is treated as an error.
    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ---- Localization (§49) ----

#[tauri::command]
pub fn list_locales(state: State<'_, AppState>) -> Vec<String> {
    crate::locales::list(&state.paths)
}

#[tauri::command]
pub fn get_locale(state: State<'_, AppState>, name: String) -> crate::locales::LocaleBundle {
    crate::locales::load(&state.paths, &name)
}

// ---- Minecraft metadata / system detection ----

#[tauri::command]
pub async fn list_minecraft_versions(
    state: State<'_, AppState>,
    force: bool,
) -> Result<faerie_minecraft::manifest::ManifestResult, String> {
    let manifest = state.manifest.clone();
    manifest.fetch(force).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn detect_java(
    state: State<'_, AppState>,
) -> Result<Vec<faerie_minecraft::java::JavaInstallation>, String> {
    let managed = state.paths.data_dir.join("java");
    Ok(faerie_minecraft::java::detect_installations(&managed).await)
}

#[tauri::command]
pub async fn detect_hardware(
    state: State<'_, AppState>,
) -> Result<faerie_minecraft::hardware::HardwareInfo, String> {
    let data_root = state.paths.root.clone();
    Ok(faerie_minecraft::hardware::detect(&data_root).await)
}

// ---- Instances ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceListDto {
    pub instances: Vec<faerie_instances::Instance>,
    /// Human-readable descriptions of unreadable instances (§41: shown, not hidden).
    pub problems: Vec<String>,
}

#[tauri::command]
pub fn list_instances(state: State<'_, AppState>) -> InstanceListDto {
    let (instances, problems) = state.instances.list();
    InstanceListDto {
        instances,
        problems,
    }
}

#[tauri::command]
pub fn create_instance(
    state: State<'_, AppState>,
    name: String,
    minecraft_version: String,
) -> Result<faerie_instances::Instance, String> {
    let instance = state
        .instances
        .create(&name, &minecraft_version)
        .map_err(|e| e.to_string())?;
    crate::defaults::apply_servers(&instance);
    Ok(instance)
}

#[tauri::command]
pub fn rename_instance(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<faerie_instances::Instance, String> {
    state
        .instances
        .rename(&id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duplicate_instance(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<faerie_instances::Instance, String> {
    state
        .instances
        .duplicate(&id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_instance(state: State<'_, AppState>, id: String) -> Result<String, String> {
    state
        .instances
        .delete(&id)
        .map(|trash| trash.display().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_instance_java(
    state: State<'_, AppState>,
    id: String,
    max_ram_mb: Option<u32>,
    min_ram_mb: Option<u32>,
    java_path: Option<String>,
    extra_args: Option<String>,
) -> Result<faerie_instances::Instance, String> {
    state
        .instances
        .update(&id, |config| {
            config.java.max_ram_mb = max_ram_mb;
            config.java.min_ram_mb = min_ram_mb;
            config.java.path_override = java_path
                .filter(|p| !p.trim().is_empty())
                .map(PathBuf::from);
            if let Some(args) = extra_args {
                config.java.extra_args = args.split_whitespace().map(str::to_string).collect();
            }
        })
        .map_err(|e| e.to_string())
}

// ---- Accounts ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsDto {
    pub accounts: Vec<AccountRecord>,
    pub active: Option<String>,
    /// False until a Microsoft client id is configured.
    pub sign_in_available: bool,
}

#[tauri::command]
pub fn list_accounts(state: State<'_, AppState>) -> Result<AccountsDto, String> {
    // A failure here shows up in the UI as "offline session", which is easy
    // to mistake for a deliberate state — leave a trace in the log.
    let accounts = state.accounts.list().map_err(|e| {
        tracing::warn!(target: "authentication", "could not read accounts: {e}");
        e.to_string()
    })?;
    let active = state.accounts.active_id().map_err(|e| e.to_string())?;
    tracing::info!(
        target: "authentication",
        "listed {} account(s) from {} (active: {})",
        accounts.len(),
        state.paths.config_dir.join("accounts.json").display(),
        active.as_deref().unwrap_or("none")
    );
    Ok(AccountsDto {
        accounts,
        active,
        sign_in_available: !state.client_id().trim().is_empty(),
    })
}

fn auth_flow(state: &AppState) -> AuthFlow {
    AuthFlow::new(state.http.clone(), Endpoints::default(), state.client_id())
}

/// Start sign-in: returns the code and URL to show the user. The device code
/// itself is retained in app state and never serialized to the webview.
#[tauri::command]
pub async fn begin_sign_in(state: State<'_, AppState>) -> Result<DeviceCodePrompt, String> {
    let prompt = auth_flow(&state)
        .start_device_code()
        .await
        .map_err(|e| e.to_string())?;
    *state.pending_sign_in.lock().await = Some(prompt.clone());
    Ok(prompt)
}

/// Poll until the user completes sign-in in their browser, then store the
/// account. Takes no arguments: the pending prompt from [`begin_sign_in`]
/// supplies the device code, so the handle never leaves the backend.
#[tauri::command]
pub async fn complete_sign_in(state: State<'_, AppState>) -> Result<AccountRecord, String> {
    let prompt = state
        .pending_sign_in
        .lock()
        .await
        .clone()
        .ok_or("no sign-in is in progress; start one first")?;

    let account = auth_flow(&state)
        .complete_device_code(&prompt, |_| true)
        .await;
    // The code is single-use either way: clear it before reporting.
    state.pending_sign_in.lock().await.take();

    // Failures are logged as well as returned: a toast is easy to miss, and
    // `AuthError` messages carry no tokens by construction.
    let account = account
        .inspect_err(|e| tracing::warn!(target: "authentication", "sign-in failed: {e}"))
        .map_err(|e| e.to_string())?;
    let record = state.accounts.upsert(&account).map_err(|e| e.to_string())?;
    tracing::info!(target: "authentication", "signed in as {}", record.name);
    Ok(record)
}

/// Abandon an in-flight sign-in (the user closed the prompt).
#[tauri::command]
pub async fn cancel_sign_in(state: State<'_, AppState>) -> Result<(), String> {
    state.pending_sign_in.lock().await.take();
    Ok(())
}

#[tauri::command]
pub fn set_active_account(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.accounts.set_active(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_account(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.accounts.remove(&id).map_err(|e| e.to_string())
}

// ---- Play ----

/// Resolve the session to launch with: the active account (refreshed if its
/// token expired), or an offline session when no account is signed in.
async fn resolve_session(state: &AppState, offline_name: &str) -> Result<Session, String> {
    let Some(record) = state.accounts.active().map_err(|e| e.to_string())? else {
        tracing::info!(
            target: "authentication",
            "no account is signed in; launching offline as {offline_name}"
        );
        return Ok(Session::offline(offline_name));
    };
    let (mc_token, refresh_token) = match state.accounts.secrets_for(&record.id) {
        Ok(secrets) => secrets,
        Err(e) => {
            tracing::warn!(target: "authentication", "stored credentials unavailable: {e}");
            return Ok(Session::offline(&record.name));
        }
    };

    let account = if record.is_expired() {
        tracing::info!(target: "authentication", "refreshing expired token for {}", record.name);
        let refreshed = auth_flow(state)
            .refresh(&refresh_token)
            .await
            .map_err(|e| e.to_string())?;
        state
            .accounts
            .upsert(&refreshed)
            .map_err(|e| e.to_string())?;
        refreshed
    } else {
        tracing::info!(target: "authentication", "launching as {} (token still valid)", record.name);
        faerie_auth::store::account_from_parts(&record, mc_token, refresh_token)
    };

    Ok(Session {
        player_name: account.profile.name.clone(),
        uuid: account.profile.id.clone(),
        access_token: account.minecraft_token.expose().clone(),
        xuid: account.profile.xuid.clone(),
        user_type: "msa".into(),
    })
}

#[tauri::command]
pub async fn play_instance(state: State<'_, AppState>, id: String) -> Result<u32, String> {
    if state.running_game.lock().await.is_some() {
        return Err("Minecraft is already running. Stop it before launching again.".into());
    }

    let instance = state.instances.get(&id).map_err(|e| e.to_string())?;
    let session = resolve_session(&state, "Player").await?;

    let version_id = match &instance.config.loader {
        Some(loader) => {
            let kind = faerie_modding::scan::LoaderKind::from_id(&loader.kind)
                .ok_or_else(|| format!("unknown loader `{}`", loader.kind))?;
            faerie_modding::loader::installed_version_id(
                kind,
                &instance.config.minecraft_version,
                &loader.version,
            )
            .ok_or_else(|| {
                format!(
                    "launching {} instances is not supported yet",
                    kind.display()
                )
            })?
        }
        None => instance.config.minecraft_version.clone(),
    };
    tracing::info!(
        target: "minecraft",
        "{}: launching version {version_id}",
        instance.config.name
    );

    // Defaults from settings; the instance's own values win when set.
    let (default_min, default_max, default_args) = {
        let config = state.config.read().unwrap();
        (
            config.get_u64("java.default_min_ram_mb").unwrap_or(512) as u32,
            config.get_u64("java.default_max_ram_mb").unwrap_or(0) as u32,
            config.get_str("java.default_args").unwrap_or_default(),
        )
    };
    let max_ram_mb = match instance.config.java.max_ram_mb {
        Some(v) => Some(v),
        None if default_max > 0 => Some(default_max),
        None => {
            // Derive from hardware when nothing is configured (§9).
            let hw = faerie_minecraft::hardware::detect(&state.paths.root).await;
            Some(hw.recommended_heap_mb as u32)
        }
    };
    let extra_jvm_args = if instance.config.java.extra_args.is_empty() {
        default_args
            .split_whitespace()
            .map(str::to_string)
            .collect()
    } else {
        instance.config.java.extra_args.clone()
    };

    let cancel = CancellationToken::new();
    *state.play_cancel.write().unwrap() = Some(cancel.clone());

    let request = PlayRequest {
        instance_id: instance.id.clone(),
        instance_name: instance.config.name.clone(),
        game_dir: instance.dir.clone(),
        version_id,
        session,
        min_ram_mb: instance.config.java.min_ram_mb.or(Some(default_min)),
        max_ram_mb,
        extra_jvm_args,
        java_override: instance.config.java.path_override.clone(),
    };

    let result = launching::play(&state, request, cancel).await;
    state.play_cancel.write().unwrap().take();
    match result {
        Ok(pid) => {
            let _ = state.instances.update(&id, |config| {
                config.last_played_at_secs = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                );
            });
            Ok(pid)
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn cancel_play(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(cancel) = state.play_cancel.read().unwrap().clone() {
        cancel.cancel();
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningGameDto {
    pub instance_id: String,
    pub pid: Option<u32>,
}

#[tauri::command]
pub async fn running_game(state: State<'_, AppState>) -> Result<Option<RunningGameDto>, String> {
    Ok(state
        .running_game
        .lock()
        .await
        .as_ref()
        .map(|g| RunningGameDto {
            instance_id: g.instance_id.clone(),
            pid: g.pid,
        }))
}

#[tauri::command]
pub async fn stop_game(state: State<'_, AppState>) -> Result<(), String> {
    let mut slot = state.running_game.lock().await;
    if let Some(game) = slot.as_mut() {
        if let Some(process) = game.process.as_mut() {
            process.kill().await.map_err(|e| e.to_string())?;
        } else if let Some(pid) = game.pid {
            kill_by_pid(pid)?;
        }
    }
    Ok(())
}

/// Terminate by pid: the process handle lives in the exit-watcher task, so
/// the stop command signals the OS directly.
fn kill_by_pid(pid: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("the game process could not be stopped".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let status = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("the game process could not be stopped".into());
        }
        Ok(())
    }
}
