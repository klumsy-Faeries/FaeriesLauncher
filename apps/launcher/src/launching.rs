//! Orchestrates the full Play flow: install → pick Java → launch → stream
//! console output → report the exit.
//!
//! Lives in the app layer because it is the one place that combines every
//! crate; the crates themselves stay independent.

use std::path::PathBuf;
use std::sync::Arc;

use faerie_core::{Event, EventBus};
use faerie_minecraft::install::Installer;
use faerie_minecraft::launch::{self, LaunchOptions, Session};
use faerie_minecraft::process::{ExitClass, GameProcess};
use faerie_minecraft::{java_runtime, GamePaths, McError};
use faerie_net::DownloadConfig;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::state::AppState;

/// A running (or recently finished) game session.
pub struct RunningGame {
    pub instance_id: String,
    pub pid: Option<u32>,
    pub process: Option<GameProcess>,
}

pub type GameSlot = Arc<Mutex<Option<RunningGame>>>;

/// Everything the Play flow needs, gathered by the caller from app state.
pub struct PlayRequest {
    pub instance_id: String,
    pub instance_name: String,
    pub game_dir: PathBuf,
    /// The version to install and launch: the loader's version id when the
    /// instance has a loader (`fabric-loader-0.19.5-26.2`), else the vanilla
    /// one. Launching the vanilla id on a modded instance silently runs the
    /// game with no mods — that is exactly the bug this field fixes.
    pub version_id: String,
    pub session: Session,
    pub min_ram_mb: Option<u32>,
    pub max_ram_mb: Option<u32>,
    pub extra_jvm_args: Vec<String>,
    pub java_override: Option<PathBuf>,
}

/// Install (if needed) and launch. Progress and console output are emitted on
/// the event bus; the returned pid is the spawned JVM's.
pub async fn play(
    state: &AppState,
    request: PlayRequest,
    cancel: CancellationToken,
) -> Result<u32, McError> {
    let bus = state.bus.clone();
    let paths = GamePaths::new(state.paths.data_dir.clone());
    paths
        .ensure_base_dirs()
        .map_err(|e| McError::io(&paths.root, e))?;

    let (concurrency, retries) = {
        let config = state.config.read().unwrap();
        (
            config.get_u64("downloads.concurrency").unwrap_or(4) as usize,
            config.get_u64("downloads.retries").unwrap_or(3) as u32,
        )
    };
    let downloads = DownloadConfig {
        concurrency,
        retries,
    };

    // --- Install -----------------------------------------------------
    emit_status(&bus, &request.instance_id, "resolving version metadata");
    let manifest = state
        .manifest
        .fetch(false)
        .await
        .map_err(|e| McError::ManifestInvalid(e.to_string()))?;

    let installer = Installer::new(state.http.clone(), paths.clone(), downloads);
    let instance_id = request.instance_id.clone();
    let progress_bus = bus.clone();
    let installed = installer
        .install(&request.version_id, &manifest.manifest, &cancel, move |p| {
            progress_bus.emit(Event::InstallProgress {
                instance_id: instance_id.clone(),
                phase: p.phase.to_string(),
                files_done: p.files_done,
                files_total: p.files_total,
                bytes_done: p.bytes_done,
                bytes_total: p.bytes_total,
                bytes_per_sec: p.bytes_per_sec,
            });
        })
        .await?;

    // --- Java --------------------------------------------------------
    emit_status(&bus, &request.instance_id, "selecting Java runtime");
    let detected = faerie_minecraft::java::detect_installations(&paths.java).await;
    let java_path = match launch::select_java(
        &detected,
        installed.required_java_major,
        request.java_override.as_ref(),
    ) {
        // A detected runtime that actually satisfies the requirement.
        Some(choice)
            if request.java_override.is_some()
                || meets_requirement(&choice, installed.required_java_major) =>
        {
            choice.path()
        }
        // Nothing suitable: fetch the runtime this version asks for.
        _ => {
            let component = installed
                .required_java_component
                .clone()
                .unwrap_or_else(|| {
                    java_runtime::component_for_major(installed.required_java_major.unwrap_or(21))
                        .to_string()
                });
            emit_status(
                &bus,
                &request.instance_id,
                &format!("downloading Java runtime ({component})"),
            );
            let provisioner = java_runtime::RuntimeProvisioner::new(
                state.http.clone(),
                faerie_net::Downloader::new(state.http.clone(), downloads),
                paths.clone(),
            );
            let instance_id = request.instance_id.clone();
            let progress_bus = bus.clone();
            let exe = provisioner
                .provision(&component, None, &cancel, move |done, total| {
                    progress_bus.emit(Event::InstallProgress {
                        instance_id: instance_id.clone(),
                        phase: "java runtime".into(),
                        files_done: done,
                        files_total: total,
                        bytes_done: 0,
                        bytes_total: None,
                        bytes_per_sec: 0,
                    });
                })
                .await?;
            exe
        }
    };

    // --- Launch ------------------------------------------------------
    emit_status(&bus, &request.instance_id, "starting Minecraft");
    tokio::fs::create_dir_all(&request.game_dir)
        .await
        .map_err(|e| McError::io(&request.game_dir, e))?;

    let options = LaunchOptions {
        game_dir: request.game_dir.clone(),
        java_path,
        session: request.session,
        min_ram_mb: request.min_ram_mb,
        max_ram_mb: request.max_ram_mb,
        extra_jvm_args: request.extra_jvm_args,
        env: vec![],
        resolution: None,
        launcher_name: "FaeriesLauncher".into(),
        launcher_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let spec = launch::build_spec(&installed, &paths, &options);
    tracing::info!(
        target: "minecraft",
        "launching {} with {} ({} args)",
        request.instance_name,
        spec.program.display(),
        spec.args.len()
    );

    let (tx, mut rx) = tokio::sync::mpsc::channel(512);
    let process = GameProcess::spawn(&spec, tx).map_err(|e| McError::io(&spec.program, e))?;
    let pid = process.id().unwrap_or_default();

    // Forward console output to the UI.
    let console_bus = bus.clone();
    let console_instance = request.instance_id.clone();
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            console_bus.emit(Event::GameLog {
                instance_id: console_instance.clone(),
                stderr: line.stderr,
                text: line.text,
            });
        }
    });

    // Watch for exit and report how it ended.
    let exit_bus = bus.clone();
    let exit_instance = request.instance_id.clone();
    let slot = Arc::clone(&state.running_game);
    tokio::spawn(async move {
        let report = process.wait().await;
        let (class, detail) = describe_exit(&report.class);
        tracing::info!(target: "minecraft", "game exited: {class} ({detail})");
        slot.lock().await.take();
        exit_bus.emit(Event::GameExited {
            instance_id: exit_instance,
            code: report.code,
            class: class.to_string(),
            detail,
        });
    });

    // `process` moved into the watcher; keep identity for the kill command.
    *state.running_game.lock().await = Some(RunningGame {
        instance_id: request.instance_id.clone(),
        pid: Some(pid),
        process: None,
    });

    bus.emit(Event::GameStarted {
        instance_id: request.instance_id,
        pid,
    });
    Ok(pid)
}

fn meets_requirement(choice: &launch::JavaChoice<'_>, required: Option<u32>) -> bool {
    let Some(required) = required else {
        return true;
    };
    match choice {
        launch::JavaChoice::Override(_) => true,
        launch::JavaChoice::Detected(java) => java.major >= required,
    }
}

fn describe_exit(class: &ExitClass) -> (&'static str, String) {
    match class {
        ExitClass::Normal => ("normal", "Minecraft closed normally.".into()),
        ExitClass::Killed => ("killed", "The game was stopped from the launcher.".into()),
        ExitClass::JavaError { detail } => ("java", detail.clone()),
        ExitClass::MinecraftCrash { detail } => ("minecraft", detail.clone()),
        ExitClass::ModError { detail } => ("mod", detail.clone()),
        ExitClass::LauncherError { detail } => ("launcher", detail.clone()),
    }
}

fn emit_status(bus: &EventBus, instance_id: &str, message: &str) {
    tracing::info!(target: "minecraft", "{instance_id}: {message}");
    bus.emit(Event::InstallProgress {
        instance_id: instance_id.to_string(),
        phase: message.to_string(),
        files_done: 0,
        files_total: 0,
        bytes_done: 0,
        bytes_total: None,
        bytes_per_sec: 0,
    });
}
