//! Launch arguments: the modern `arguments` model, the legacy
//! `minecraftArguments` string, and `${variable}` substitution.

use serde::{Deserialize, Serialize};

use super::rules::{allowed, Rule, RuleContext};

/// One entry in a `game`/`jvm` argument array: either a bare string or a
/// conditional group gated by rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Constant(String),
    Conditional {
        rules: Vec<Rule>,
        #[serde(deserialize_with = "string_or_seq")]
        value: Vec<String>,
    },
}

fn string_or_seq<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(s) => vec![s],
        OneOrMany::Many(v) => v,
    })
}

/// The modern `arguments` object.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArgumentSet {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

/// Flatten a raw argument list to the strings whose rules pass, in order.
pub fn resolve(args: &[Argument], ctx: &RuleContext) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            Argument::Constant(s) => out.push(s.clone()),
            Argument::Conditional { rules, value } => {
                if allowed(rules, ctx) {
                    out.extend(value.iter().cloned());
                }
            }
        }
    }
    out
}

/// Substitute every `${name}` in `input` using `lookup`. Unknown variables
/// are left untouched (rather than blanked), which surfaces mistakes instead
/// of hiding them.
pub fn substitute(input: &str, lookup: &impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end) = input[i + 2..].find('}') {
                let name = &input[i + 2..i + 2 + end];
                match lookup(name) {
                    Some(value) => out.push_str(&value),
                    None => out.push_str(&input[i..i + 2 + end + 1]),
                }
                i += 2 + end + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Split a legacy `minecraftArguments` string into individual tokens. These
/// strings never contain quoted spaces, so whitespace splitting is correct.
pub fn split_legacy(minecraft_arguments: &str) -> Vec<String> {
    minecraft_arguments
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::rules::FeatureSet;

    fn ctx() -> RuleContext {
        RuleContext {
            os_name: "windows",
            os_arch: "x86_64",
            features: FeatureSet::default(),
        }
    }

    #[test]
    fn substitute_replaces_known_and_keeps_unknown() {
        let vars = |name: &str| match name {
            "auth_player_name" => Some("Faerie".to_string()),
            "game_directory" => Some(r"C:\games\inst".to_string()),
            _ => None,
        };
        assert_eq!(
            substitute(
                "--username ${auth_player_name} --dir ${game_directory}",
                &vars
            ),
            r"--username Faerie --dir C:\games\inst"
        );
        // Unknown variable is preserved verbatim.
        assert_eq!(substitute("${unknown_thing}", &vars), "${unknown_thing}");
    }

    #[test]
    fn conditional_arguments_respect_rules() {
        let raw = r#"[
            "--always",
            { "rules": [{ "action": "allow", "features": { "is_demo_user": true } }],
              "value": "--demo" },
            { "rules": [{ "action": "allow", "os": { "name": "windows" } }],
              "value": ["--windowsOnly", "yes"] }
        ]"#;
        let args: Vec<Argument> = serde_json::from_str(raw).unwrap();
        let resolved = resolve(&args, &ctx());
        assert_eq!(resolved, vec!["--always", "--windowsOnly", "yes"]);
    }

    #[test]
    fn legacy_arguments_split_on_whitespace() {
        let tokens = split_legacy("--username ${auth_player_name} --version 1.8.9");
        assert_eq!(
            tokens,
            vec!["--username", "${auth_player_name}", "--version", "1.8.9"]
        );
    }

    #[test]
    fn conditional_value_accepts_string_or_array() {
        let raw = r#"[
            { "rules": [], "value": "single" },
            { "rules": [], "value": ["a", "b"] }
        ]"#;
        let args: Vec<Argument> = serde_json::from_str(raw).unwrap();
        assert_eq!(resolve(&args, &ctx()), vec!["single", "a", "b"]);
    }
}
