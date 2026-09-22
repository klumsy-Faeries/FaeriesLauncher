//! The "what's new" feed.
//!
//! Rather than invent a news service, the launcher shows its own release
//! notes: `CHANGELOG.md` is embedded at compile time and parsed into
//! entries. The panel therefore always tells the truth about the build the
//! user is actually running.

use serde::Serialize;

const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogEntry {
    /// Version heading, e.g. `0.4.0`.
    pub version: String,
    /// Short title after the version, e.g. `Phase 4 modding`.
    pub title: String,
    /// Release date as written in the heading, when present.
    pub date: String,
    /// The first few bullet points, flattened to plain sentences.
    pub highlights: Vec<String>,
}

/// Parse `## <version> — <title> (<date>)` sections and their bullets.
pub fn parse(markdown: &str, max_entries: usize, max_highlights: usize) -> Vec<ChangelogEntry> {
    let mut entries: Vec<ChangelogEntry> = Vec::new();

    for line in markdown.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            if entries.len() == max_entries {
                break;
            }
            entries.push(parse_heading(heading.trim()));
            continue;
        }
        // Bullets belong to the most recent heading. Nested continuation
        // lines are ignored; the first line of a bullet is the headline.
        let Some(current) = entries.last_mut() else {
            continue;
        };
        if current.highlights.len() >= max_highlights {
            continue;
        }
        if let Some(bullet) = line.strip_prefix("- ") {
            current.highlights.push(clean_markdown(bullet.trim()));
        }
    }
    entries
}

fn parse_heading(heading: &str) -> ChangelogEntry {
    // `0.4.0 — Phase 4 modding (2026-08-30)`
    let (version, rest) = match heading.split_once('—') {
        Some((v, r)) => (v.trim().to_string(), r.trim()),
        None => (heading.to_string(), ""),
    };
    let (title, date) = match rest.rsplit_once('(') {
        Some((t, d)) => (
            t.trim().to_string(),
            d.trim_end_matches(')').trim().to_string(),
        ),
        None => (rest.to_string(), String::new()),
    };
    ChangelogEntry {
        version,
        title,
        date,
        highlights: Vec::new(),
    }
}

/// Strip the markdown a one-line summary does not need.
fn clean_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // `**bold**` and `*emphasis*` both reduce to plain text.
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                }
            }
            '`' => {}
            other => out.push(other),
        }
    }
    // Keep it to one sentence so the card stays a summary, not an essay.
    let trimmed = out.trim();
    match trimmed.split_once(". ") {
        Some((first, _)) => format!("{first}."),
        None => trimmed.to_string(),
    }
}

/// The entries shown in the home page's news panel.
#[tauri::command]
pub fn changelog() -> Vec<ChangelogEntry> {
    parse(CHANGELOG, 4, 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headings_into_version_title_and_date() {
        let entries = parse(
            "# Changelog\n\n\
             ## 0.4.0 — Phase 4 modding (2026-08-30)\n\n\
             - **Mod scanning**: reads descriptors. More detail here.\n\
             - Second point.\n\n\
             ## 0.3.0 — Phase 3 (2026-08-29)\n\n\
             - Older thing.\n",
            10,
            10,
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].version, "0.4.0");
        assert_eq!(entries[0].title, "Phase 4 modding");
        assert_eq!(entries[0].date, "2026-08-30");
        assert_eq!(entries[0].highlights.len(), 2);
        assert_eq!(entries[1].version, "0.3.0");
    }

    #[test]
    fn highlights_lose_markdown_and_keep_one_sentence() {
        let entries = parse(
            "## 1.0.0 — Test (2026-01-01)\n- **Bold thing**: does `code` stuff. Extra detail.\n",
            1,
            5,
        );
        let highlight = &entries[0].highlights[0];
        assert!(!highlight.contains('*'), "no markdown: {highlight}");
        assert!(!highlight.contains('`'));
        assert!(
            !highlight.contains("Extra detail"),
            "trimmed to one sentence"
        );
        assert!(highlight.starts_with("Bold thing"));
    }

    #[test]
    fn limits_are_respected() {
        let entries = parse(
            "## 2.0 — A (d)\n- one\n- two\n- three\n## 1.0 — B (d)\n- x\n## 0.9 — C (d)\n- y\n",
            2,
            2,
        );
        assert_eq!(entries.len(), 2, "max_entries caps the list");
        assert_eq!(
            entries[0].highlights.len(),
            2,
            "max_highlights caps bullets"
        );
    }

    #[test]
    fn the_real_changelog_parses() {
        let entries = changelog();
        assert!(!entries.is_empty(), "the shipped changelog has entries");
        let newest = &entries[0];
        assert!(!newest.version.is_empty());
        assert!(!newest.highlights.is_empty());
        // The newest entry should match the version the app reports.
        assert_eq!(newest.version, env!("CARGO_PKG_VERSION"));
    }
}
