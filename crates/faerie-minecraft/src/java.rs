//! Java Runtime Manager — detection half (§10 of the spec).
//!
//! Candidates come from `JAVA_HOME`, the `PATH`, the Windows registry,
//! well-known vendor directories, and the launcher's own managed runtimes
//! directory. Every candidate is then *probed* — actually executed with
//! `-XshowSettings:properties` — because paths lie and probes don't.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaInstallation {
    /// Path to the `java` executable itself.
    pub path: PathBuf,
    /// Full version string, e.g. `21.0.3` or `1.8.0_401`.
    pub version: String,
    /// Feature release number: 8, 17, 21, …
    pub major: u32,
    pub vendor: String,
    pub arch: String,
    /// Where detection found it (`env`, `path`, `registry`, `vendor-dir`, `managed`).
    pub source: &'static str,
}

#[derive(Debug, Clone)]
struct Candidate {
    path: PathBuf,
    source: &'static str,
}

#[cfg(windows)]
const JAVA_EXE: &str = "java.exe";
#[cfg(not(windows))]
const JAVA_EXE: &str = "java";

/// Detect every working Java installation. `managed_dir` is the launcher's
/// own runtime directory (`<data>/java`), whose entries are listed first.
pub async fn detect_installations(managed_dir: &Path) -> Vec<JavaInstallation> {
    let candidates = gather_candidates(managed_dir);
    let mut found: Vec<JavaInstallation> = futures_util::stream::iter(candidates)
        .map(probe)
        .buffer_unordered(4)
        .filter_map(|result| async move { result })
        .collect()
        .await;

    // The same runtime often shows up via several roads (PATH + registry +
    // vendor dir). Keep one entry per (version, vendor, arch).
    let mut seen = HashSet::new();
    found
        .retain(|java| seen.insert((java.version.clone(), java.vendor.clone(), java.arch.clone())));
    found.sort_by(|a, b| b.major.cmp(&a.major).then(a.version.cmp(&b.version)));
    found
}

fn gather_candidates(managed_dir: &Path) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    let push = |path: PathBuf, source: &'static str, out: &mut Vec<Candidate>| {
        if path.is_file() {
            out.push(Candidate { path, source });
        }
    };

    // Launcher-managed runtimes: <data>/java/<runtime>/bin/java
    if let Ok(entries) = std::fs::read_dir(managed_dir) {
        for entry in entries.flatten() {
            push(
                entry.path().join("bin").join(JAVA_EXE),
                "managed",
                &mut candidates,
            );
        }
    }

    // JAVA_HOME
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        if !home.is_empty() {
            push(
                PathBuf::from(home).join("bin").join(JAVA_EXE),
                "env",
                &mut candidates,
            );
        }
    }

    // PATH
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            push(dir.join(JAVA_EXE), "path", &mut candidates);
        }
    }

    // Windows registry
    #[cfg(windows)]
    for home in registry_java_homes() {
        push(home.join("bin").join(JAVA_EXE), "registry", &mut candidates);
    }

    // Well-known vendor directories
    for root in vendor_roots() {
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                push(
                    entry.path().join("bin").join(JAVA_EXE),
                    "vendor-dir",
                    &mut candidates,
                );
            }
        }
    }

    // Dedupe by canonical path so one runtime is probed once.
    let mut seen = HashSet::new();
    candidates.retain(|c| {
        let key = std::fs::canonicalize(&c.path).unwrap_or_else(|_| c.path.clone());
        seen.insert(key)
    });
    candidates
}

#[cfg(windows)]
fn vendor_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for env in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(programs) = std::env::var_os(env) {
            let programs = PathBuf::from(programs);
            for vendor in [
                "Java",
                "Eclipse Adoptium",
                "Eclipse Foundation",
                "Microsoft",
                "Zulu",
                "Amazon Corretto",
                "BellSoft",
                "Semeru",
            ] {
                roots.push(programs.join(vendor));
            }
        }
    }
    roots
}

#[cfg(not(windows))]
fn vendor_roots() -> Vec<PathBuf> {
    ["/usr/lib/jvm", "/Library/Java/JavaVirtualMachines"]
        .iter()
        .map(|root| PathBuf::from(*root))
        .collect()
}

#[cfg(windows)]
fn registry_java_homes() -> Vec<PathBuf> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let mut homes = Vec::new();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    // Oracle-style layout: SOFTWARE\JavaSoft\{JDK,Java Runtime Environment}\<v>\JavaHome
    for family in [
        r"SOFTWARE\JavaSoft\JDK",
        r"SOFTWARE\JavaSoft\Java Runtime Environment",
    ] {
        if let Ok(key) = hklm.open_subkey(family) {
            for version in key.enum_keys().flatten() {
                if let Ok(vkey) = key.open_subkey(&version) {
                    if let Ok(home) = vkey.get_value::<String, _>("JavaHome") {
                        homes.push(PathBuf::from(home));
                    }
                }
            }
        }
    }

    // Adoptium layout: SOFTWARE\Eclipse Adoptium\JDK\<v>\hotspot\MSI\Path
    for family in [
        r"SOFTWARE\Eclipse Adoptium\JDK",
        r"SOFTWARE\Eclipse Adoptium\JRE",
    ] {
        if let Ok(key) = hklm.open_subkey(family) {
            for version in key.enum_keys().flatten() {
                if let Ok(msi) = key.open_subkey(format!(r"{version}\hotspot\MSI")) {
                    if let Ok(home) = msi.get_value::<String, _>("Path") {
                        homes.push(PathBuf::from(home));
                    }
                }
            }
        }
    }
    homes
}

/// Run the candidate and parse its self-reported properties. Returns `None`
/// for anything that is not a working Java executable.
async fn probe(candidate: Candidate) -> Option<JavaInstallation> {
    let mut command = tokio::process::Command::new(&candidate.path);
    command
        .args(["-XshowSettings:properties", "-version"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: no console flash from a GUI process.
        command.creation_flags(0x0800_0000);
    }

    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .ok()?
        .ok()?;
    // The JVM prints -XshowSettings and -version output to stderr.
    let text = String::from_utf8_lossy(&output.stderr);
    let properties = parse_properties(&text)?;
    Some(JavaInstallation {
        path: candidate.path,
        major: major_of(&properties.version)?,
        version: properties.version,
        vendor: properties.vendor,
        arch: properties.arch,
        source: candidate.source,
    })
}

struct ProbedProperties {
    version: String,
    vendor: String,
    arch: String,
}

fn parse_properties(text: &str) -> Option<ProbedProperties> {
    let field = |name: &str| -> Option<String> {
        text.lines().find_map(|line| {
            let (key, value) = line.trim().split_once('=')?;
            (key.trim() == name).then(|| value.trim().to_string())
        })
    };
    Some(ProbedProperties {
        version: field("java.version")?,
        vendor: field("java.vendor").unwrap_or_else(|| "unknown".into()),
        arch: field("os.arch").unwrap_or_else(|| "unknown".into()),
    })
}

/// `1.8.0_401` → 8 (legacy scheme), `21.0.3` → 21, `26` → 26.
fn major_of(version: &str) -> Option<u32> {
    let mut parts = version.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn major_versions_cover_both_naming_eras() {
        assert_eq!(major_of("1.8.0_401"), Some(8));
        assert_eq!(major_of("17.0.10"), Some(17));
        assert_eq!(major_of("21.0.3"), Some(21));
        assert_eq!(major_of("26"), Some(26));
        assert_eq!(major_of("junk"), None);
    }

    #[test]
    fn properties_parse_from_xshowsettings_output() {
        let stderr = r#"
Property settings:
    file.encoding = UTF-8
    java.home = C:\Program Files\Eclipse Adoptium\jdk-21.0.3.9-hotspot
    java.vendor = Eclipse Adoptium
    java.version = 21.0.3
    os.arch = amd64
    os.name = Windows 11

openjdk version "21.0.3" 2024-04-16 LTS
"#;
        let props = parse_properties(stderr).unwrap();
        assert_eq!(props.version, "21.0.3");
        assert_eq!(props.vendor, "Eclipse Adoptium");
        assert_eq!(props.arch, "amd64");
    }

    #[test]
    fn gather_scans_managed_dir_and_tolerates_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let managed = tmp.path().join("java");
        // A managed runtime layout with a fake java executable file…
        let bin = managed.join("jdk-21").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join(JAVA_EXE), "not a real exe").unwrap();
        // …and a runtime folder with no binary at all.
        std::fs::create_dir_all(managed.join("empty")).unwrap();

        let candidates = gather_candidates(&managed);
        assert!(candidates
            .iter()
            .any(|c| c.source == "managed" && c.path.ends_with(Path::new("bin").join(JAVA_EXE))));
    }

    #[tokio::test]
    async fn probing_a_fake_executable_yields_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = tmp.path().join(JAVA_EXE);
        std::fs::write(&fake, "definitely not a JVM").unwrap();
        let result = probe(Candidate {
            path: fake,
            source: "managed",
        })
        .await;
        assert!(result.is_none());
    }
}
