use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

/// Application-wide events. The Tauri layer forwards these to the webview as
/// one `faerie://event` stream; backend crates subscribe where useful.
///
/// Everything is serializable so the frontend receives the same shape the
/// backend emits — no translation layer to drift.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    SettingChanged {
        id: String,
        value: Value,
        restart_required: bool,
    },
    TaskStarted {
        id: u64,
        name: String,
    },
    TaskProgress {
        id: u64,
        /// 0.0..=1.0
        progress: f32,
        message: Option<String>,
    },
    TaskFinished {
        id: u64,
        outcome: TaskOutcome,
    },
    /// Version/asset/runtime download progress for a Play flow.
    InstallProgress {
        instance_id: String,
        /// Human-readable stage, e.g. `downloading` or `extracting natives`.
        phase: String,
        files_done: usize,
        files_total: usize,
        bytes_done: u64,
        bytes_total: Option<u64>,
        bytes_per_sec: u64,
    },
    GameStarted {
        instance_id: String,
        pid: u32,
    },
    /// One line of game output (§26).
    GameLog {
        instance_id: String,
        stderr: bool,
        text: String,
    },
    /// The game ended. `class` is one of `normal`, `killed`, `java`,
    /// `minecraft`, `mod`, `launcher` (§25).
    GameExited {
        instance_id: String,
        code: Option<i32>,
        class: String,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TaskOutcome {
    Completed,
    Cancelled,
    Failed { error: String },
}

/// Broadcast bus. Cloning is cheap; every clone shares the same channel.
/// Emitting with no subscribers is a no-op, never an error.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx }
    }

    pub fn emit(&self, event: Event) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribers_receive_emitted_events() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        bus.emit(Event::TaskStarted {
            id: 1,
            name: "demo".into(),
        });
        match rx.recv().await.unwrap() {
            Event::TaskStarted { id, name } => {
                assert_eq!(id, 1);
                assert_eq!(name, "demo");
            }
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn emit_without_subscribers_is_a_noop() {
        let bus = EventBus::new();
        bus.emit(Event::TaskFinished {
            id: 1,
            outcome: TaskOutcome::Completed,
        });
    }
}
