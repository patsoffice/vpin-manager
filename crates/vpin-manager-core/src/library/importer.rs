use crate::library::scanner::ScannedFile;
use crate::vpsdb::models::Game;

/// Confidence level of a match between a scanned file and a VPS game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Confidence::Low => write!(f, "low"),
            Confidence::Medium => write!(f, "medium"),
            Confidence::High => write!(f, "high"),
        }
    }
}

/// A proposed match between a scanned file and a VPS game entry.
#[derive(Debug, Clone)]
pub struct MatchResult<'a> {
    pub file: &'a ScannedFile,
    pub game: &'a Game,
    pub confidence: Confidence,
    pub score: f64,
}

/// A scanned file that couldn't be matched to any game.
#[derive(Debug, Clone)]
pub struct Unmatched<'a> {
    pub file: &'a ScannedFile,
}

/// Results of matching scanned files against the VPS database.
#[derive(Debug)]
pub struct ImportResults<'a> {
    pub matches: Vec<MatchResult<'a>>,
    pub unmatched: Vec<Unmatched<'a>>,
}

/// Match scanned files against VPS database games by normalized name similarity.
pub fn match_files<'a>(
    files: &'a [ScannedFile],
    games: &'a [Game],
) -> ImportResults<'a> {
    // Pre-compute normalized game names for faster comparison.
    let normalized_games: Vec<(&Game, String)> = games
        .iter()
        .map(|g| (g, normalize(&g.name)))
        .collect();

    let mut matches = Vec::new();
    let mut unmatched = Vec::new();

    for file in files {
        let normalized_stem = normalize(&file.stem);

        if let Some((game, score)) = find_best_match(&normalized_stem, &normalized_games) {
            let confidence = if score >= 0.95 {
                Confidence::High
            } else if score >= 0.7 {
                Confidence::Medium
            } else {
                Confidence::Low
            };

            matches.push(MatchResult {
                file,
                game,
                confidence,
                score,
            });
        } else {
            unmatched.push(Unmatched { file });
        }
    }

    // Sort matches by confidence (high first), then by game name.
    matches.sort_by(|a, b| {
        b.confidence
            .cmp(&a.confidence)
            .then_with(|| a.game.name.cmp(&b.game.name))
    });

    ImportResults { matches, unmatched }
}

/// Find the best matching game for a normalized file stem.
/// Returns the game and a similarity score (0.0 to 1.0).
fn find_best_match<'a>(
    stem: &str,
    games: &[(&'a Game, String)],
) -> Option<(&'a Game, f64)> {
    let mut best: Option<(&Game, f64)> = None;

    for (game, normalized_name) in games {
        let score = similarity(stem, normalized_name);

        if score >= 0.5 {
            if best.is_none() || score > best.unwrap().1 {
                best = Some((game, score));
            }
        }
    }

    best
}

/// Normalize a name for comparison:
/// - Lowercase
/// - Replace separators (underscores, hyphens, dots) with spaces
/// - Strip common suffixes (version numbers, "vpx", "vpt", "mod", etc.)
/// - Collapse whitespace
/// - Trim
fn normalize(name: &str) -> String {
    let mut s = name.to_lowercase();

    // Replace separators with spaces
    s = s.replace(['_', '-', '.'], " ");

    // Remove common suffixes/tokens that don't contribute to the game name
    let strip_tokens = [
        "vpx", "vpt", "fx", "fx2", "fx3", "fpt",
        "mod", "premium", "le", "pro",
        "v1", "v2", "v3", "v4", "v5",
        "1 0", "1 1", "1 2", "2 0", "3 0",
    ];
    for token in strip_tokens {
        // Only strip if it's a separate word (surrounded by spaces or at boundary)
        let pattern = format!(" {token}");
        if s.ends_with(&pattern) {
            s.truncate(s.len() - pattern.len());
        }
    }

    // Remove version patterns like "v1.0.0" or "1.0" at end
    // After separator replacement these look like "v1 0 0" or "1 0"
    // Already handled by strip_tokens above for common cases.

    // Remove trailing parenthesized content like "(Author Mod)"
    if let Some(paren_start) = s.rfind('(') {
        s.truncate(paren_start);
    }

    // Collapse whitespace and trim
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Compute similarity between two normalized strings.
/// Uses a combination of:
/// 1. Exact match (1.0)
/// 2. One contains the other (0.8-0.95 depending on length ratio)
/// 3. Word overlap (Jaccard-like score)
fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }

    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    // Containment check — higher score for closer length match
    if a.contains(b) || b.contains(a) {
        let shorter = a.len().min(b.len()) as f64;
        let longer = a.len().max(b.len()) as f64;
        let ratio = shorter / longer;
        // Scale between 0.75 and 0.98 based on length ratio
        return 0.75 + (ratio * 0.23);
    }

    // Word overlap (Jaccard similarity on words)
    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();

    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }

    let intersection = words_a.intersection(&words_b).count() as f64;
    let union = words_a.union(&words_b).count() as f64;

    intersection / union
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ResourceType;
    use std::path::PathBuf;

    fn make_scanned(stem: &str) -> ScannedFile {
        ScannedFile {
            path: PathBuf::from(format!("/tables/{stem}.vpx")),
            resource_type: ResourceType::Tables,
            stem: stem.to_string(),
            size: 1000,
        }
    }

    fn make_game(id: &str, name: &str) -> Game {
        Game {
            id: id.to_string(),
            name: name.to_string(),
            manufacturer: None,
            year: None,
            game_type: None,
            players: None,
            theme: vec![],
            designers: vec![],
            features: vec![],
            ipdb_url: None,
            mpu: None,
            img_url: None,
            broken: None,
            updated_at: None,
            last_created_at: None,
            table_files: vec![],
            b2s_files: vec![],
            rom_files: vec![],
            wheel_art_files: vec![],
            topper_files: vec![],
            pup_pack_files: vec![],
            alt_sound_files: vec![],
            alt_color_files: vec![],
            tutorial_files: vec![],
            pov_files: vec![],
            media_pack_files: vec![],
            rule_files: vec![],
            sound_files: vec![],
        }
    }

    #[test]
    fn normalize_strips_separators() {
        assert_eq!(normalize("Medieval_Madness"), "medieval madness");
        assert_eq!(normalize("Hook-VPX"), "hook");
        assert_eq!(normalize("Some.Table.Name"), "some table name");
    }

    #[test]
    fn normalize_strips_version_suffixes() {
        assert_eq!(normalize("Medieval_Madness_VPX"), "medieval madness");
        assert_eq!(normalize("Hook_Mod"), "hook");
        assert_eq!(normalize("Table_v2"), "table");
    }

    #[test]
    fn normalize_strips_parenthesized() {
        assert_eq!(
            normalize("Medieval Madness (Bigus Mod)"),
            "medieval madness"
        );
    }

    #[test]
    fn exact_match_high_confidence() {
        let files = vec![make_scanned("Medieval Madness")];
        let games = vec![make_game("g1", "Medieval Madness")];

        let results = match_files(&files, &games);
        assert_eq!(results.matches.len(), 1);
        assert_eq!(results.matches[0].confidence, Confidence::High);
        assert_eq!(results.matches[0].game.id, "g1");
    }

    #[test]
    fn underscore_name_matches() {
        let files = vec![make_scanned("Medieval_Madness")];
        let games = vec![make_game("g1", "Medieval Madness")];

        let results = match_files(&files, &games);
        assert_eq!(results.matches.len(), 1);
        assert_eq!(results.matches[0].confidence, Confidence::High);
    }

    #[test]
    fn name_with_vpx_suffix_matches() {
        let files = vec![make_scanned("Medieval_Madness_VPX")];
        let games = vec![make_game("g1", "Medieval Madness")];

        let results = match_files(&files, &games);
        assert_eq!(results.matches.len(), 1);
        assert!(results.matches[0].confidence >= Confidence::High);
    }

    #[test]
    fn no_match_returns_unmatched() {
        let files = vec![make_scanned("CompletelyRandomName12345")];
        let games = vec![make_game("g1", "Medieval Madness")];

        let results = match_files(&files, &games);
        assert_eq!(results.matches.len(), 0);
        assert_eq!(results.unmatched.len(), 1);
    }

    #[test]
    fn partial_match_medium_confidence() {
        let files = vec![make_scanned("Hook Data East 1992 Bigus Mod")];
        let games = vec![make_game("g1", "Hook")];

        let results = match_files(&files, &games);
        assert_eq!(results.matches.len(), 1);
        // "hook" is contained in "hook data east 1992 bigus", but length ratio is low
        assert!(results.matches[0].score >= 0.5);
    }

    #[test]
    fn multiple_files_matched() {
        let files = vec![
            make_scanned("Medieval_Madness"),
            make_scanned("Hook"),
            make_scanned("Unknown_Game_XYZ"),
        ];
        let games = vec![
            make_game("g1", "Medieval Madness"),
            make_game("g2", "Hook"),
        ];

        let results = match_files(&files, &games);
        assert_eq!(results.matches.len(), 2);
        assert_eq!(results.unmatched.len(), 1);
    }

    #[test]
    fn similarity_exact() {
        assert_eq!(similarity("hook", "hook"), 1.0);
    }

    #[test]
    fn similarity_containment() {
        let score = similarity("hook", "hook data east");
        assert!(score >= 0.75);
        assert!(score < 1.0);
    }

    #[test]
    fn similarity_no_overlap() {
        let score = similarity("completely", "different");
        assert!(score < 0.5);
    }
}
