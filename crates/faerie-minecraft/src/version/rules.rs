//! Rule evaluation for libraries and arguments.
//!
//! Mojang version JSON gates libraries and argument fragments behind rule
//! arrays. Each rule allows or disallows based on the current OS/arch and a
//! set of feature flags (demo mode, custom resolution, quick-play). The
//! effective result is: start disallowed, apply each *matching* rule in
//! order, the last one wins.

use serde::{Deserialize, Serialize};

/// The launch environment a rule is evaluated against.
#[derive(Debug, Clone)]
pub struct RuleContext {
    /// Mojang OS name: `windows`, `osx`, or `linux`.
    pub os_name: &'static str,
    /// Mojang OS arch: `x86`, `x86_64`, or `arm64`.
    pub os_arch: &'static str,
    pub features: FeatureSet,
}

impl RuleContext {
    /// The context for the machine this launcher is running on.
    pub fn current(features: FeatureSet) -> Self {
        Self {
            os_name: current_os_name(),
            os_arch: current_os_arch(),
            features,
        }
    }
}

/// Feature flags referenced by feature rules. All default to `false`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FeatureSet {
    pub is_demo_user: bool,
    pub has_custom_resolution: bool,
    pub has_quick_plays_support: bool,
    pub is_quick_play_singleplayer: bool,
    pub is_quick_play_multiplayer: bool,
    pub is_quick_play_realms: bool,
}

impl FeatureSet {
    fn get(&self, key: &str) -> Option<bool> {
        Some(match key {
            "is_demo_user" => self.is_demo_user,
            "has_custom_resolution" => self.has_custom_resolution,
            "has_quick_plays_support" => self.has_quick_plays_support,
            "is_quick_play_singleplayer" => self.is_quick_play_singleplayer,
            "is_quick_play_multiplayer" => self.is_quick_play_multiplayer,
            "is_quick_play_realms" => self.is_quick_play_realms,
            _ => return None, // unknown feature: never matches
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    /// A regex against the OS version. We do not evaluate it (extremely rare
    /// in practice, and pulling a regex engine in for it is not worth it);
    /// its presence alone does not fail the match.
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: std::collections::BTreeMap<String, bool>,
}

impl Rule {
    fn matches(&self, ctx: &RuleContext) -> bool {
        if let Some(os) = &self.os {
            if let Some(name) = &os.name {
                if name != ctx.os_name {
                    return false;
                }
            }
            if let Some(arch) = &os.arch {
                if arch != ctx.os_arch {
                    return false;
                }
            }
        }
        for (key, expected) in &self.features {
            // Unknown feature keys never match, so a rule requiring one is
            // inert — matching how launchers treat capabilities they lack.
            if ctx.features.get(key) != Some(*expected) {
                return false;
            }
        }
        true
    }
}

/// Evaluate a rule list. An empty list is unconditionally allowed.
pub fn allowed(rules: &[Rule], ctx: &RuleContext) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allow = false;
    for rule in rules {
        if rule.matches(ctx) {
            allow = matches!(rule.action, RuleAction::Allow);
        }
    }
    allow
}

pub fn current_os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        _ => "linux",
    }
}

pub fn current_os_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        "x86" => "x86",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(os: &'static str, arch: &'static str) -> RuleContext {
        RuleContext {
            os_name: os,
            os_arch: arch,
            features: FeatureSet::default(),
        }
    }

    fn os_rule(action: RuleAction, name: Option<&str>, arch: Option<&str>) -> Rule {
        Rule {
            action,
            os: Some(OsRule {
                name: name.map(str::to_string),
                arch: arch.map(str::to_string),
                version: None,
            }),
            features: Default::default(),
        }
    }

    #[test]
    fn empty_rules_allow() {
        assert!(allowed(&[], &ctx("windows", "x86_64")));
    }

    #[test]
    fn os_specific_allow_only_on_that_os() {
        let rules = vec![os_rule(RuleAction::Allow, Some("osx"), None)];
        assert!(allowed(&rules, &ctx("osx", "x86_64")));
        assert!(!allowed(&rules, &ctx("windows", "x86_64")));
    }

    #[test]
    fn last_matching_rule_wins() {
        // Allow everywhere, then disallow on windows.
        let rules = vec![
            Rule {
                action: RuleAction::Allow,
                os: None,
                features: Default::default(),
            },
            os_rule(RuleAction::Disallow, Some("windows"), None),
        ];
        assert!(!allowed(&rules, &ctx("windows", "x86_64")));
        assert!(allowed(&rules, &ctx("linux", "x86_64")));
    }

    #[test]
    fn arch_gating() {
        let rules = vec![os_rule(RuleAction::Allow, Some("windows"), Some("arm64"))];
        assert!(allowed(&rules, &ctx("windows", "arm64")));
        assert!(!allowed(&rules, &ctx("windows", "x86_64")));
    }

    #[test]
    fn feature_rules_match_flags() {
        let mut features = std::collections::BTreeMap::new();
        features.insert("is_demo_user".to_string(), true);
        let rules = vec![Rule {
            action: RuleAction::Allow,
            os: None,
            features,
        }];

        let mut demo = ctx("windows", "x86_64");
        demo.features.is_demo_user = true;
        assert!(allowed(&rules, &demo));

        let not_demo = ctx("windows", "x86_64");
        assert!(!allowed(&rules, &not_demo));
    }
}
