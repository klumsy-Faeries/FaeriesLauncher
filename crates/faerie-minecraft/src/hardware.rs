//! Hardware detection (§9): informs recommendations, never forces them.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub cpu_model: String,
    pub cpu_cores: Option<usize>,
    pub cpu_threads: usize,
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub os: String,
    pub arch: &'static str,
    /// Best effort; empty when the query fails or times out.
    pub gpus: Vec<String>,
    /// Free space on the volume holding the launcher's data directory.
    pub data_disk_free_gb: Option<u64>,
    /// Suggested `-Xmx` for new instances. A suggestion only (§9): every
    /// instance can override it.
    pub recommended_heap_mb: u64,
}

/// Conservative heap suggestion from total RAM. Deliberately simple and
/// deliberately capped: more heap than 8G rarely helps Minecraft and often
/// hurts GC pause times.
pub fn recommended_heap_mb(total_ram_mb: u64) -> u64 {
    match total_ram_mb {
        0..=4096 => 2048,
        4097..=8192 => 4096,
        8193..=16384 => 6144,
        _ => 8192,
    }
}

pub async fn detect(data_root: &Path) -> HardwareInfo {
    // sysinfo work is synchronous; keep it off the async threads.
    let data_root_owned = data_root.to_path_buf();
    let base = tokio::task::spawn_blocking(move || {
        let mut sys = sysinfo::System::new();
        sys.refresh_cpu_all();
        sys.refresh_memory();

        let cpu_model = sys
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .unwrap_or_else(|| "unknown".into());
        let total_ram_mb = sys.total_memory() / (1024 * 1024);

        let disks = sysinfo::Disks::new_with_refreshed_list();
        let data_disk_free_gb = disks
            .list()
            .iter()
            .filter(|disk| data_root_owned.starts_with(disk.mount_point()))
            .max_by_key(|disk| disk.mount_point().as_os_str().len())
            .map(|disk| disk.available_space() / (1024 * 1024 * 1024));

        HardwareInfo {
            cpu_model,
            cpu_cores: sys.physical_core_count(),
            cpu_threads: sys.cpus().len(),
            total_ram_mb,
            available_ram_mb: sys.available_memory() / (1024 * 1024),
            os: sysinfo::System::long_os_version().unwrap_or_else(|| "unknown".into()),
            arch: std::env::consts::ARCH,
            gpus: Vec::new(),
            data_disk_free_gb,
            recommended_heap_mb: recommended_heap_mb(total_ram_mb),
        }
    })
    .await
    .expect("hardware probe never panics");

    HardwareInfo {
        gpus: detect_gpus().await,
        ..base
    }
}

#[cfg(windows)]
async fn detect_gpus() -> Vec<String> {
    let mut command = tokio::process::Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-CimInstance Win32_VideoController).Name",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    match tokio::time::timeout(Duration::from_secs(6), command.output()).await {
        Ok(Ok(output)) => String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(not(windows))]
async fn detect_gpus() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_recommendation_is_tiered_and_capped() {
        assert_eq!(recommended_heap_mb(2048), 2048);
        assert_eq!(recommended_heap_mb(4096), 2048);
        assert_eq!(recommended_heap_mb(8192), 4096);
        assert_eq!(recommended_heap_mb(16384), 6144);
        assert_eq!(recommended_heap_mb(65536), 8192);
    }

    #[tokio::test]
    async fn detect_reports_plausible_numbers() {
        let tmp = tempfile::tempdir().unwrap();
        let info = detect(tmp.path()).await;
        assert!(info.cpu_threads >= 1);
        assert!(info.total_ram_mb > 256, "machines have more than 256 MB");
        assert!(!info.cpu_model.is_empty());
        assert_eq!(
            info.recommended_heap_mb,
            recommended_heap_mb(info.total_ram_mb)
        );
    }
}
