use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use crate::events::{Event, EventBus, TaskOutcome};

pub type TaskError = Box<dyn std::error::Error + Send + Sync>;
pub type TaskResult = Result<(), TaskError>;

/// Runs named async tasks, reports their lifecycle on the [`EventBus`], and
/// supports cancellation. This is what the downloads UI and progress
/// indicators will observe in later phases.
#[derive(Clone)]
pub struct TaskScheduler {
    bus: EventBus,
    next_id: Arc<AtomicU64>,
    active: Arc<Mutex<HashMap<u64, String>>>,
}

/// Handed to every task body: progress reporting and cooperative cancellation.
#[derive(Clone)]
pub struct TaskContext {
    pub id: u64,
    bus: EventBus,
    cancel: CancellationToken,
}

impl TaskContext {
    /// Report progress (`0.0..=1.0`, clamped) with an optional status message.
    pub fn progress(&self, progress: f32, message: Option<&str>) {
        self.bus.emit(Event::TaskProgress {
            id: self.id,
            progress: progress.clamp(0.0, 1.0),
            message: message.map(str::to_string),
        });
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

/// Handle returned by [`TaskScheduler::spawn`].
pub struct TaskHandle {
    pub id: u64,
    cancel: CancellationToken,
    join: tokio::task::JoinHandle<()>,
}

impl TaskHandle {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Wait until the task has fully finished (including its Finished event).
    pub async fn wait(self) {
        let _ = self.join.await;
    }
}

impl TaskScheduler {
    pub fn new(bus: EventBus) -> Self {
        Self {
            bus,
            next_id: Arc::new(AtomicU64::new(1)),
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Spawn a named task. Cancellation via [`TaskHandle::cancel`] aborts the
    /// task at its next await point; the task can also poll
    /// [`TaskContext::is_cancelled`] to stop at clean boundaries.
    pub fn spawn<F, Fut>(&self, name: impl Into<String>, f: F) -> TaskHandle
    where
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: Future<Output = TaskResult> + Send + 'static,
    {
        let name = name.into();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancel = CancellationToken::new();
        let ctx = TaskContext {
            id,
            bus: self.bus.clone(),
            cancel: cancel.clone(),
        };

        self.active.lock().unwrap().insert(id, name.clone());
        self.bus.emit(Event::TaskStarted { id, name });

        let bus = self.bus.clone();
        let active = Arc::clone(&self.active);
        let token = cancel.clone();
        let join = tokio::spawn(async move {
            let outcome = tokio::select! {
                _ = token.cancelled() => TaskOutcome::Cancelled,
                result = f(ctx) => match result {
                    Ok(()) => TaskOutcome::Completed,
                    Err(e) => TaskOutcome::Failed { error: e.to_string() },
                },
            };
            active.lock().unwrap().remove(&id);
            bus.emit(Event::TaskFinished { id, outcome });
        });

        TaskHandle { id, cancel, join }
    }

    /// Names of currently running tasks (id, name).
    pub fn active(&self) -> Vec<(u64, String)> {
        let mut tasks: Vec<_> = self
            .active
            .lock()
            .unwrap()
            .iter()
            .map(|(id, name)| (*id, name.clone()))
            .collect();
        tasks.sort_by_key(|(id, _)| *id);
        tasks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_outcome(rx: &mut tokio::sync::broadcast::Receiver<Event>) -> Option<TaskOutcome> {
        while let Ok(event) = rx.try_recv() {
            if let Event::TaskFinished { outcome, .. } = event {
                return Some(outcome);
            }
        }
        None
    }

    #[tokio::test]
    async fn task_lifecycle_emits_started_progress_finished() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let scheduler = TaskScheduler::new(bus);

        let handle = scheduler.spawn("demo", |ctx| async move {
            ctx.progress(0.5, Some("halfway"));
            Ok(())
        });
        handle.wait().await;

        let mut saw_started = false;
        let mut saw_progress = false;
        let mut outcome = None;
        while let Ok(event) = rx.try_recv() {
            match event {
                Event::TaskStarted { .. } => saw_started = true,
                Event::TaskProgress { progress, .. } => {
                    saw_progress = true;
                    assert_eq!(progress, 0.5);
                }
                Event::TaskFinished { outcome: o, .. } => outcome = Some(o),
                _ => {}
            }
        }
        assert!(saw_started && saw_progress);
        assert_eq!(outcome, Some(TaskOutcome::Completed));
        assert!(scheduler.active().is_empty());
    }

    #[tokio::test]
    async fn cancellation_produces_cancelled_outcome() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let scheduler = TaskScheduler::new(bus);

        let handle = scheduler.spawn("stuck", |_ctx| async move {
            // A future that only ends via cancellation.
            std::future::pending::<()>().await;
            Ok(())
        });
        handle.cancel();
        handle.wait().await;

        assert_eq!(collect_outcome(&mut rx), Some(TaskOutcome::Cancelled));
        assert!(scheduler.active().is_empty());
    }

    #[tokio::test]
    async fn failure_carries_the_error_message() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let scheduler = TaskScheduler::new(bus);

        let handle = scheduler.spawn("broken", |_ctx| async move { Err("disk exploded".into()) });
        handle.wait().await;

        assert_eq!(
            collect_outcome(&mut rx),
            Some(TaskOutcome::Failed {
                error: "disk exploded".into()
            })
        );
    }
}
