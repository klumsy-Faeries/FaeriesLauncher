use std::sync::{Arc, RwLock};

use faerie_auth::device_code::DeviceCodePrompt;
use faerie_auth::AccountStore;
use faerie_core::config::ConfigStore;
use faerie_core::{DataPaths, EventBus};
use faerie_instances::InstanceStore;
use faerie_minecraft::manifest::ManifestService;
use tokio_util::sync::CancellationToken;

use crate::launching::GameSlot;

/// Shared application state managed by Tauri and injected into commands.
pub struct AppState {
    pub paths: DataPaths,
    pub config: RwLock<ConfigStore>,
    pub bus: EventBus,
    pub http: reqwest::Client,
    pub manifest: ManifestService,
    pub instances: InstanceStore,
    pub accounts: AccountStore,
    /// The in-flight device-code sign-in. The device code is a bearer-like
    /// handle, so it stays here and never crosses the IPC boundary.
    pub pending_sign_in: tokio::sync::Mutex<Option<DeviceCodePrompt>>,
    /// The currently running game, if any (one at a time per launcher window).
    pub running_game: GameSlot,
    /// Cancels an in-flight install/launch.
    pub play_cancel: Arc<RwLock<Option<CancellationToken>>>,
    /// Kept across snapshots so CPU percentages have something to diff
    /// against — a fresh `System` always reports 0%.
    pub perf_sampler: std::sync::Mutex<sysinfo::System>,
    pub startup_timings: Vec<(String, f64)>,
    pub startup_total_ms: f64,
    pub started_at: std::time::Instant,
}

impl AppState {
    /// The Microsoft application (client) id from settings. Empty until the
    /// user supplies one — see the Accounts page.
    pub fn client_id(&self) -> String {
        self.config
            .read()
            .unwrap()
            .get_str("accounts.client_id")
            .unwrap_or_default()
    }
}
