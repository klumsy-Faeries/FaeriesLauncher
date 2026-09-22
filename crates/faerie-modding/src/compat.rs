//! Compatibility checking (§12): given an instance's loader, Minecraft
//! version, and installed mods, work out what will actually load.
//!
//! Every finding carries a severity, a human explanation, and a suggested
//! fix, following §47: what happened, why, and how to resolve it.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::scan::{DependencyKind, LoaderKind, ScannedMod};
use crate::version_range::Version;

/// Mod ids that are satisfied by the environment rather than by a jar.
const MINECRAFT_IDS: [&str; 1] = ["minecraft"];
const JAVA_IDS: [&str; 1] = ["java"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The game will very likely fail to start.
    Error,
    /// The game should start, but something is off.
    Warning,
    /// Informational only.
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    MissingDependency,
    WrongDependencyVersion,
    Conflict,
    DuplicateModId,
    WrongLoader,
    WrongMinecraftVersion,
    UnreadableMetadata,
    UnknownRequirement,
}

/// One compatibility finding.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub severity: Severity,
    pub kind: IssueKind,
    /// The mod the issue is about (file name, for identification in the UI).
    pub subject: String,
    /// What happened.
    pub summary: String,
    /// Why it happened.
    pub detail: String,
    /// How to fix it.
    pub fix: String,
}

/// The result of checking one instance's mod set.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatReport {
    pub loader: LoaderKind,
    pub minecraft_version: String,
    pub loader_version: String,
    /// Enabled mods considered by the check.
    pub mods_checked: usize,
    pub issues: Vec<Issue>,
}

impl CompatReport {
    pub fn errors(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count()
    }

    /// True when nothing should stop the game from starting.
    pub fn is_compatible(&self) -> bool {
        self.errors() == 0
    }
}

/// The environment mods are being checked against.
#[derive(Debug, Clone)]
pub struct Environment {
    pub loader: LoaderKind,
    pub loader_version: String,
    pub minecraft_version: String,
}

/// Check a set of scanned mods against an environment.
///
/// Disabled mods are skipped entirely: a `.disabled` jar is inert, so it can
/// neither satisfy a dependency nor cause a conflict.
pub fn check(env: &Environment, mods: &[ScannedMod]) -> CompatReport {
    let enabled: Vec<&ScannedMod> = mods.iter().filter(|m| !m.disabled).collect();
    let mut issues = Vec::new();

    // Index every id the environment provides: mods, their `provides`
    // aliases, the mods bundled inside them, plus the loader and Minecraft
    // themselves.
    let mut provided: BTreeMap<&str, &str> = BTreeMap::new(); // id -> version
    for m in &enabled {
        for (id, version) in m.provided_versions() {
            if !id.is_empty() {
                offer(&mut provided, id, version);
            }
        }
    }
    for id in MINECRAFT_IDS {
        provided.insert(id, env.minecraft_version.as_str());
    }
    // Loader ids as mods refer to them.
    let loader_alias = match env.loader {
        LoaderKind::Fabric => Some("fabricloader"),
        LoaderKind::Quilt => Some("quilt_loader"),
        LoaderKind::Forge => Some("forge"),
        LoaderKind::NeoForge => Some("neoforge"),
        LoaderKind::Unknown => None,
    };
    if let Some(alias) = loader_alias {
        provided.insert(alias, env.loader_version.as_str());
    }
    // Quilt provides the Fabric API surface, so `fabricloader` resolves too.
    if env.loader == LoaderKind::Quilt {
        provided.insert("fabricloader", env.loader_version.as_str());
    }

    detect_duplicates(&enabled, &mut issues);

    for m in &enabled {
        check_metadata_readable(m, &mut issues);
        // A mod for the wrong loader cannot load at all, so its dependency
        // list describes a different ecosystem. Checking it would bury the
        // real problem under a cascade of downstream "missing dependency"
        // errors; report the root cause alone (§47).
        if check_loader_match(env, m, &mut issues) {
            check_dependencies(m, &provided, &mut issues);
        }
    }

    // Most severe first, then alphabetical for a stable display order.
    issues.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then(a.subject.to_lowercase().cmp(&b.subject.to_lowercase()))
    });

    CompatReport {
        loader: env.loader,
        minecraft_version: env.minecraft_version.clone(),
        loader_version: env.loader_version.clone(),
        mods_checked: enabled.len(),
        issues,
    }
}

/// Record that `id` is available at `version`. Several jars may bring the
/// same library (Fabric API and Sodium both bundle `fabric-api-base`); the
/// loader keeps the newest copy, so the check does too.
fn offer<'a>(provided: &mut BTreeMap<&'a str, &'a str>, id: &'a str, version: &'a str) {
    let newer = match provided.get(id) {
        None => true,
        Some(existing) => match (Version::parse(version), Version::parse(existing)) {
            (Some(new), Some(old)) => new > old,
            (Some(_), None) => true,
            _ => false,
        },
    };
    if newer {
        provided.insert(id, version);
    }
}

fn check_metadata_readable(m: &ScannedMod, issues: &mut Vec<Issue>) {
    if m.metadata.loader == LoaderKind::Unknown {
        issues.push(Issue {
            severity: Severity::Warning,
            kind: IssueKind::UnreadableMetadata,
            subject: m.file_name.clone(),
            summary: format!("{} has no recognizable mod descriptor.", m.file_name),
            detail: "The jar contains no fabric.mod.json, quilt.mod.json, or \
                     mods.toml, so the launcher cannot tell what it is or what \
                     it needs. It may be a library, a resource pack placed in \
                     the wrong folder, or a mod for a loader this launcher \
                     does not know."
                .into(),
            fix: "Check that this file belongs in the mods folder. If it is a \
                  library shipped with another mod, this warning is harmless."
                .into(),
        });
    }
    for warning in &m.warnings {
        issues.push(Issue {
            severity: Severity::Warning,
            kind: IssueKind::UnreadableMetadata,
            subject: m.file_name.clone(),
            summary: format!("{} has a malformed descriptor.", m.file_name),
            detail: warning.clone(),
            fix: "Re-download the mod; the file may be corrupt or truncated.".into(),
        });
    }
}

/// Returns `true` when this mod can be loaded by the instance's loader.
fn check_loader_match(env: &Environment, m: &ScannedMod, issues: &mut Vec<Issue>) -> bool {
    let mod_loader = m.metadata.loader;
    if mod_loader == LoaderKind::Unknown {
        // Already reported as unreadable; nothing to check dependencies with.
        return false;
    }
    if !env.loader.accepts_mods_for(mod_loader) {
        issues.push(Issue {
            severity: Severity::Error,
            kind: IssueKind::WrongLoader,
            subject: m.file_name.clone(),
            summary: format!(
                "{} is a {} mod, but this instance uses {}.",
                m.metadata.name,
                mod_loader.display(),
                env.loader.display()
            ),
            detail: format!(
                "Mod loaders are not interchangeable: a {} mod cannot be loaded \
                 by {}.",
                mod_loader.display(),
                env.loader.display()
            ),
            fix: format!(
                "Download the {} build of this mod, or move it to a {} instance.",
                env.loader.display(),
                mod_loader.display()
            ),
        });
        return false;
    }
    true
}

fn check_dependencies(m: &ScannedMod, provided: &BTreeMap<&str, &str>, issues: &mut Vec<Issue>) {
    for dep in &m.metadata.dependencies {
        // Java version requirements are about the runtime, not a mod.
        if JAVA_IDS.contains(&dep.mod_id.as_str()) {
            continue;
        }
        let present = provided.get(dep.mod_id.as_str()).copied();

        match dep.kind {
            DependencyKind::Breaks => {
                if let Some(version) = present {
                    if dep.requirement.matches_str(version) {
                        issues.push(Issue {
                            severity: Severity::Error,
                            kind: IssueKind::Conflict,
                            subject: m.file_name.clone(),
                            summary: format!(
                                "{} is incompatible with {} {}.",
                                m.metadata.name, dep.mod_id, version
                            ),
                            detail: format!(
                                "{} declares that it breaks {} {}, and that mod is \
                                 installed and enabled.",
                                m.metadata.name, dep.mod_id, dep.raw
                            ),
                            fix: format!(
                                "Disable or remove either {} or {}.",
                                m.metadata.name, dep.mod_id
                            ),
                        });
                    }
                }
            }
            DependencyKind::Required => match present {
                None => issues.push(Issue {
                    severity: Severity::Error,
                    kind: IssueKind::MissingDependency,
                    subject: m.file_name.clone(),
                    summary: format!(
                        "{} requires {}, which is not installed.",
                        m.metadata.name, dep.mod_id
                    ),
                    detail: format!("Required: {} {}\nInstalled: (none)", dep.mod_id, dep.raw),
                    fix: format!(
                        "Install {} {} into this instance, or remove {}.",
                        dep.mod_id, dep.raw, m.metadata.name
                    ),
                }),
                Some(version) => {
                    if !dep.requirement.matches_str(version) {
                        let kind = if MINECRAFT_IDS.contains(&dep.mod_id.as_str()) {
                            IssueKind::WrongMinecraftVersion
                        } else {
                            IssueKind::WrongDependencyVersion
                        };
                        issues.push(Issue {
                            severity: Severity::Error,
                            kind,
                            subject: m.file_name.clone(),
                            summary: format!(
                                "{} needs {} {}, but {} is installed.",
                                m.metadata.name, dep.mod_id, dep.raw, version
                            ),
                            detail: format!(
                                "Required: {} {}\nInstalled: {} {}",
                                dep.mod_id, dep.raw, dep.mod_id, version
                            ),
                            fix: if kind == IssueKind::WrongMinecraftVersion {
                                format!(
                                    "Use a build of {} for Minecraft {}, or create an \
                                     instance on a Minecraft version it supports ({}).",
                                    m.metadata.name, version, dep.raw
                                )
                            } else {
                                format!("Update {} to {}.", dep.mod_id, dep.raw)
                            },
                        });
                    }
                    if dep.requirement.is_unparsed() {
                        issues.push(Issue {
                            severity: Severity::Info,
                            kind: IssueKind::UnknownRequirement,
                            subject: m.file_name.clone(),
                            summary: format!(
                                "Could not interpret {}'s requirement on {}.",
                                m.metadata.name, dep.mod_id
                            ),
                            detail: format!(
                                "The version requirement \"{}\" is not in a format the \
                                 launcher understands, so it was not checked.",
                                dep.raw
                            ),
                            fix: "No action needed unless the game fails to start.".into(),
                        });
                    }
                }
            },
            DependencyKind::Optional => {
                if let Some(version) = present {
                    if !dep.requirement.matches_str(version) {
                        issues.push(Issue {
                            severity: Severity::Warning,
                            kind: IssueKind::WrongDependencyVersion,
                            subject: m.file_name.clone(),
                            summary: format!(
                                "{} works best with {} {}, but {} is installed.",
                                m.metadata.name, dep.mod_id, dep.raw, version
                            ),
                            detail: format!(
                                "This dependency is optional, so the game should still \
                                 start; features that rely on {} may misbehave.",
                                dep.mod_id
                            ),
                            fix: format!("Update {} to {} when convenient.", dep.mod_id, dep.raw),
                        });
                    }
                }
            }
        }
    }
}

fn detect_duplicates(mods: &[&ScannedMod], issues: &mut Vec<Issue>) {
    let mut by_id: BTreeMap<&str, Vec<&ScannedMod>> = BTreeMap::new();
    for m in mods {
        if !m.metadata.mod_id.is_empty() {
            by_id.entry(m.metadata.mod_id.as_str()).or_default().push(m);
        }
    }
    for (mod_id, group) in by_id {
        if group.len() < 2 {
            continue;
        }
        let files: Vec<String> = group
            .iter()
            .map(|m| {
                format!(
                    "{} ({})",
                    m.file_name,
                    version_or_unknown(&m.metadata.version)
                )
            })
            .collect();
        issues.push(Issue {
            severity: Severity::Error,
            kind: IssueKind::DuplicateModId,
            subject: group[0].file_name.clone(),
            summary: format!("{} is installed {} times.", mod_id, group.len()),
            detail: format!(
                "Several jars declare the mod id \"{mod_id}\":\n  {}",
                files.join("\n  ")
            ),
            fix: "Keep one copy and disable or remove the others. Loaders refuse \
                  to start when a mod id appears twice."
                .into(),
        });
    }
}

fn version_or_unknown(version: &str) -> &str {
    if version.is_empty() {
        "version unknown"
    } else {
        version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{Dependency, ModMetadata, NestedMod};
    use crate::version_range::VersionReq;
    use std::path::PathBuf;

    fn env(loader: LoaderKind, mc: &str, loader_version: &str) -> Environment {
        Environment {
            loader,
            loader_version: loader_version.into(),
            minecraft_version: mc.into(),
        }
    }

    fn dep(id: &str, req: &str, kind: DependencyKind) -> Dependency {
        Dependency {
            mod_id: id.into(),
            kind,
            requirement: VersionReq::parse(req),
            raw: req.into(),
        }
    }

    fn make_mod(
        file: &str,
        id: &str,
        version: &str,
        loader: LoaderKind,
        deps: Vec<Dependency>,
    ) -> ScannedMod {
        ScannedMod {
            path: PathBuf::from(file),
            file_name: file.into(),
            size: 1,
            disabled: file.ends_with(".disabled"),
            metadata: ModMetadata {
                mod_id: id.into(),
                name: id.into(),
                version: version.into(),
                loader,
                description: String::new(),
                authors: Vec::new(),
                dependencies: deps,
                provides: Vec::new(),
            },
            nested: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn bundled(id: &str, version: &str) -> NestedMod {
        NestedMod {
            file: format!("META-INF/jars/{id}-{version}.jar"),
            mod_id: id.into(),
            version: version.into(),
            provides: Vec::new(),
        }
    }

    #[test]
    fn bundled_jars_satisfy_dependencies_at_their_own_version() {
        // Fabric API is one jar carrying forty-odd modules; Dynamic FPS
        // depends on two of them.
        let mut api = make_mod(
            "fabric-api.jar",
            "fabric-api",
            "0.160.0+26.2",
            LoaderKind::Fabric,
            vec![],
        );
        api.nested = vec![
            bundled("fabric-lifecycle-events-v1", "4.1.4+29b6eb019e"),
            bundled("fabric-resource-loader-v0", "3.3.20+4fc5413f9e"),
        ];
        let mods = vec![
            api,
            make_mod(
                "dynamic-fps.jar",
                "dynamic_fps",
                "3.11.9",
                LoaderKind::Fabric,
                vec![
                    dep("fabric-lifecycle-events-v1", "*", DependencyKind::Required),
                    dep(
                        "fabric-resource-loader-v0",
                        ">=3.0.0",
                        DependencyKind::Required,
                    ),
                ],
            ),
        ];
        let report = check(&env(LoaderKind::Fabric, "26.2", "0.19.5"), &mods);
        assert!(report.is_compatible(), "issues: {:?}", report.issues);
        assert_eq!(
            report.mods_checked, 2,
            "bundled mods are not counted as installed mods"
        );
    }

    #[test]
    fn the_newest_copy_of_a_bundled_library_wins() {
        // Sodium bundles an older fabric-api-base than Fabric API does; the
        // loader picks the newer one, so a mod needing the newer one is fine.
        let mut api = make_mod(
            "fabric-api.jar",
            "fabric-api",
            "0.160.0",
            LoaderKind::Fabric,
            vec![],
        );
        api.nested = vec![bundled("fabric-api-base", "2.0.4+ece063239e")];
        let mut sodium = make_mod(
            "sodium.jar",
            "sodium",
            "0.9.1+mc26.2",
            LoaderKind::Fabric,
            vec![],
        );
        sodium.nested = vec![bundled("fabric-api-base", "2.0.1+abc")];
        let mods = vec![
            sodium,
            api,
            make_mod(
                "consumer.jar",
                "consumer",
                "1.0",
                LoaderKind::Fabric,
                vec![dep("fabric-api-base", ">=2.0.4", DependencyKind::Required)],
            ),
        ];
        let report = check(&env(LoaderKind::Fabric, "26.2", "0.19.5"), &mods);
        assert!(report.is_compatible(), "issues: {:?}", report.issues);
    }

    #[test]
    fn build_metadata_on_the_installed_version_does_not_fail_a_minimum() {
        let mods = vec![
            make_mod(
                "sodium.jar",
                "sodium",
                "0.9.1+mc26.2",
                LoaderKind::Fabric,
                vec![],
            ),
            make_mod(
                "rso.jar",
                "reeses_sodium_options",
                "2.2.3+mc26.2",
                LoaderKind::Fabric,
                vec![dep("sodium", ">=0.9.1", DependencyKind::Required)],
            ),
        ];
        let report = check(&env(LoaderKind::Fabric, "26.2", "0.19.5"), &mods);
        assert!(report.is_compatible(), "issues: {:?}", report.issues);
    }

    #[test]
    fn a_bundled_mod_is_not_a_duplicate_of_a_top_level_one() {
        let mut carrier = make_mod("carrier.jar", "carrier", "1.0", LoaderKind::Fabric, vec![]);
        carrier.nested = vec![bundled("shared-lib", "1.0")];
        let mods = vec![
            carrier,
            make_mod(
                "shared-lib.jar",
                "shared-lib",
                "1.1",
                LoaderKind::Fabric,
                vec![],
            ),
        ];
        let report = check(&env(LoaderKind::Fabric, "26.2", "0.19.5"), &mods);
        assert!(report.is_compatible(), "issues: {:?}", report.issues);
    }

    #[test]
    fn a_satisfied_mod_set_is_compatible() {
        let mods = vec![make_mod(
            "sodium.jar",
            "sodium",
            "0.5.8",
            LoaderKind::Fabric,
            vec![
                dep("minecraft", ">=1.20", DependencyKind::Required),
                dep("fabricloader", ">=0.15", DependencyKind::Required),
            ],
        )];
        let report = check(&env(LoaderKind::Fabric, "1.20.4", "0.15.7"), &mods);
        assert!(report.is_compatible(), "issues: {:?}", report.issues);
        assert_eq!(report.mods_checked, 1);
    }

    #[test]
    fn missing_dependency_is_an_error_naming_the_fix() {
        let mods = vec![make_mod(
            "indium.jar",
            "indium",
            "1.0",
            LoaderKind::Fabric,
            vec![dep("sodium", ">=0.5", DependencyKind::Required)],
        )];
        let report = check(&env(LoaderKind::Fabric, "1.20.4", "0.15.7"), &mods);
        assert!(!report.is_compatible());
        let issue = &report.issues[0];
        assert_eq!(issue.kind, IssueKind::MissingDependency);
        assert!(issue.summary.contains("requires sodium"));
        assert!(issue.fix.contains("Install sodium"));
    }

    #[test]
    fn dependency_version_mismatch_reports_both_sides() {
        let mods = vec![
            make_mod(
                "a.jar",
                "a",
                "1.0",
                LoaderKind::Fabric,
                vec![dep("lib", ">=2.0", DependencyKind::Required)],
            ),
            make_mod("lib.jar", "lib", "1.5", LoaderKind::Fabric, vec![]),
        ];
        let report = check(&env(LoaderKind::Fabric, "1.20.4", "0.15.7"), &mods);
        let issue = report
            .issues
            .iter()
            .find(|i| i.kind == IssueKind::WrongDependencyVersion)
            .expect("version mismatch reported");
        assert!(issue.detail.contains("Required: lib >=2.0"));
        assert!(issue.detail.contains("Installed: lib 1.5"));
    }

    #[test]
    fn wrong_minecraft_version_is_called_out_specifically() {
        let mods = vec![make_mod(
            "old.jar",
            "old",
            "1.0",
            LoaderKind::Fabric,
            vec![dep("minecraft", "1.19.2", DependencyKind::Required)],
        )];
        let report = check(&env(LoaderKind::Fabric, "1.20.4", "0.15.7"), &mods);
        let issue = &report.issues[0];
        assert_eq!(issue.kind, IssueKind::WrongMinecraftVersion);
        assert!(issue.detail.contains("Required: minecraft 1.19.2"));
        assert!(issue.detail.contains("Installed: minecraft 1.20.4"));
    }

    #[test]
    fn explicit_conflict_between_installed_mods() {
        let mods = vec![
            make_mod(
                "sodium.jar",
                "sodium",
                "0.5.8",
                LoaderKind::Fabric,
                vec![dep("optifabric", "*", DependencyKind::Breaks)],
            ),
            make_mod(
                "optifabric.jar",
                "optifabric",
                "1.13.0",
                LoaderKind::Fabric,
                vec![],
            ),
        ];
        let report = check(&env(LoaderKind::Fabric, "1.20.4", "0.15.7"), &mods);
        let issue = report
            .issues
            .iter()
            .find(|i| i.kind == IssueKind::Conflict)
            .expect("conflict reported");
        assert!(issue.fix.contains("Disable or remove"));
    }

    #[test]
    fn conflict_is_silent_when_the_other_mod_is_absent() {
        let mods = vec![make_mod(
            "sodium.jar",
            "sodium",
            "0.5.8",
            LoaderKind::Fabric,
            vec![dep("optifabric", "*", DependencyKind::Breaks)],
        )];
        let report = check(&env(LoaderKind::Fabric, "1.20.4", "0.15.7"), &mods);
        assert!(report.is_compatible());
    }

    #[test]
    fn duplicate_mod_ids_are_an_error() {
        let mods = vec![
            make_mod(
                "sodium-0.5.8.jar",
                "sodium",
                "0.5.8",
                LoaderKind::Fabric,
                vec![],
            ),
            make_mod(
                "sodium-0.5.3.jar",
                "sodium",
                "0.5.3",
                LoaderKind::Fabric,
                vec![],
            ),
        ];
        let report = check(&env(LoaderKind::Fabric, "1.20.4", "0.15.7"), &mods);
        let issue = report
            .issues
            .iter()
            .find(|i| i.kind == IssueKind::DuplicateModId)
            .expect("duplicate reported");
        assert!(issue.detail.contains("sodium-0.5.8.jar"));
        assert!(issue.detail.contains("sodium-0.5.3.jar"));
    }

    #[test]
    fn wrong_loader_is_an_error_with_actionable_advice() {
        let mods = vec![make_mod(
            "forgemod.jar",
            "fm",
            "1.0",
            LoaderKind::Forge,
            vec![],
        )];
        let report = check(&env(LoaderKind::Fabric, "1.20.4", "0.15.7"), &mods);
        let issue = &report.issues[0];
        assert_eq!(issue.kind, IssueKind::WrongLoader);
        assert!(issue.summary.contains("Forge mod"));
        assert!(issue.fix.contains("Fabric build"));
    }

    #[test]
    fn quilt_instances_accept_fabric_mods() {
        let mods = vec![make_mod(
            "sodium.jar",
            "sodium",
            "0.5.8",
            LoaderKind::Fabric,
            vec![dep("fabricloader", ">=0.15", DependencyKind::Required)],
        )];
        let report = check(&env(LoaderKind::Quilt, "1.20.4", "0.23.0"), &mods);
        assert!(report.is_compatible(), "issues: {:?}", report.issues);
    }

    #[test]
    fn disabled_mods_are_ignored_entirely() {
        let mods = vec![
            make_mod(
                "sodium.jar",
                "sodium",
                "0.5.8",
                LoaderKind::Fabric,
                vec![dep("optifabric", "*", DependencyKind::Breaks)],
            ),
            // Disabled, so it neither conflicts nor counts as installed.
            make_mod(
                "optifabric.jar.disabled",
                "optifabric",
                "1.13.0",
                LoaderKind::Fabric,
                vec![],
            ),
        ];
        let report = check(&env(LoaderKind::Fabric, "1.20.4", "0.15.7"), &mods);
        assert!(report.is_compatible());
        assert_eq!(report.mods_checked, 1);
    }

    #[test]
    fn optional_dependency_mismatch_is_only_a_warning() {
        let mods = vec![
            make_mod(
                "a.jar",
                "a",
                "1.0",
                LoaderKind::Fabric,
                vec![dep("lib", ">=2.0", DependencyKind::Optional)],
            ),
            make_mod("lib.jar", "lib", "1.0", LoaderKind::Fabric, vec![]),
        ];
        let report = check(&env(LoaderKind::Fabric, "1.20.4", "0.15.7"), &mods);
        assert!(
            report.is_compatible(),
            "optional deps must not block launch"
        );
        assert_eq!(report.warnings(), 1);
    }

    #[test]
    fn provides_aliases_satisfy_dependencies() {
        let mut provider = make_mod("api.jar", "real-api", "1.0", LoaderKind::Fabric, vec![]);
        provider.metadata.provides = vec!["legacy-api".into()];
        let mods = vec![
            provider,
            make_mod(
                "consumer.jar",
                "consumer",
                "1.0",
                LoaderKind::Fabric,
                vec![dep("legacy-api", "*", DependencyKind::Required)],
            ),
        ];
        let report = check(&env(LoaderKind::Fabric, "1.20.4", "0.15.7"), &mods);
        assert!(report.is_compatible(), "issues: {:?}", report.issues);
    }
}
