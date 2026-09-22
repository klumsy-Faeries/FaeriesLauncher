//! Performance dashboard data (§27).
//!
//! Sampling a process costs a syscall or two, so nothing here runs on its
//! own: a snapshot is taken only when the dashboard asks for one, and the
//! dashboard only asks while it is on screen. Closed, this module costs
//! exactly nothing — which is the requirement, since §7 rules out
//! background polling.

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStats {
    pub pid: u32,
    /// Resident set size in MB.
    pub memory_mb: u64,
    /// Percent of one core; can exceed 100 on multi-threaded work.
    pub cpu_percent: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceSnapshot {
    pub launcher: Option<ProcessStats>,
    /// Present only while a game is running.
    pub game: Option<ProcessStats>,
    /// Milliseconds per startup stage, from this session's boot.
    pub startup_ms: Vec<(String, f64)>,
    pub startup_total_ms: f64,
    /// Seconds since the launcher started.
    pub uptime_secs: u64,
}

fn stats_for(system: &System, pid: Pid) -> Option<ProcessStats> {
    let process = system.process(pid)?;
    Some(ProcessStats {
        pid: pid.as_u32(),
        memory_mb: process.memory() / (1024 * 1024),
        cpu_percent: process.cpu_usage(),
    })
}

/// Sample the launcher and, if one is running, the game.
///
/// CPU percentage needs two observations to mean anything, so the sampler
/// lives in app state across calls rather than being created fresh here —
/// a one-shot `System` always reports 0% CPU.
#[tauri::command]
pub fn performance_snapshot(state: State<'_, AppState>) -> PerformanceSnapshot {
    // `try_lock` rather than blocking: a dashboard sample must never wait on
    // the launch path. If the slot is momentarily busy we simply report no
    // game for this tick.
    let game_pid = state
        .running_game
        .try_lock()
        .ok()
        .and_then(|slot| slot.as_ref().and_then(|game| game.pid))
        .map(Pid::from_u32);

    let mut sampler = state.perf_sampler.lock().expect("sampler lock");
    let launcher_pid = sysinfo::get_current_pid().ok();

    let mut wanted: Vec<Pid> = Vec::new();
    if let Some(pid) = launcher_pid {
        wanted.push(pid);
    }
    if let Some(pid) = game_pid {
        wanted.push(pid);
    }
    sampler.refresh_processes_specifics(
        ProcessesToUpdate::Some(&wanted),
        true,
        ProcessRefreshKind::nothing().with_memory().with_cpu(),
    );

    PerformanceSnapshot {
        launcher: launcher_pid.and_then(|pid| stats_for(&sampler, pid)),
        game: game_pid.and_then(|pid| stats_for(&sampler, pid)),
        startup_ms: state.startup_timings.clone(),
        startup_total_ms: state.startup_total_ms,
        uptime_secs: state.started_at.elapsed().as_secs(),
    }
}
