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

    // Build a ROM identifier -> Game lookup from VPS DB romFiles.
    // The `version` field contains the ROM ID (e.g., "mm_109c", "hook_501").
    // The `name` field sometimes also contains a ROM ID.
    let rom_games: std::collections::HashMap<String, &Game> = games
        .iter()
        .flat_map(|g| {
            let from_version = g.rom_files
                .iter()
                .filter_map(|r| r.version.as_ref())
                .map(move |rom| (rom.to_lowercase(), g));
            let from_name = g.rom_files
                .iter()
                .filter_map(|r| r.name.as_ref())
                .map(move |rom| (rom.to_lowercase(), g));
            from_version.chain(from_name)
        })
        .collect();

    // Also collect ROM names from VPX metadata we've already read,
    // so we can match ROM zip files against games found via VPX tables.
    let vpx_rom_games: std::collections::HashMap<String, &Game> = files
        .iter()
        .filter_map(|f| {
            let rom = f.vpx_metadata.as_ref()?.rom_name.as_ref()?;
            let normalized = normalize(&f.stem);
            let (game, _) = find_best_match(&normalized, &normalized_games)?;
            Some((rom.to_lowercase(), game))
        })
        .collect();

    let mut matches = Vec::new();
    let mut unmatched = Vec::new();

    for file in files {
        // Try VPX metadata first (most accurate)
        if let Some(result) = try_vpx_metadata_match(file, &normalized_games, &rom_games) {
            matches.push(result);
            continue;
        }

        // For ROM files, try matching stem against known ROM identifiers
        if file.resource_type == crate::config::ResourceType::Roms {
            let lower_stem = file.stem.to_lowercase();
            if let Some(&game) = rom_games.get(&lower_stem).or_else(|| vpx_rom_games.get(&lower_stem)) {
                matches.push(MatchResult {
                    file,
                    game,
                    confidence: Confidence::High,
                    score: 1.0,
                });
                continue;
            }
        }

        // Fall back to filename-based matching
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

/// Try to match using VPX metadata (table name from file, ROM name).
fn try_vpx_metadata_match<'a>(
    file: &'a ScannedFile,
    normalized_games: &[(&'a Game, String)],
    rom_games: &std::collections::HashMap<String, &'a Game>,
) -> Option<MatchResult<'a>> {
    let meta = file.vpx_metadata.as_ref()?;

    // Try ROM name match first (most precise)
    if let Some(ref rom) = meta.rom_name {
        if let Some(&game) = rom_games.get(&rom.to_lowercase()) {
            return Some(MatchResult {
                file,
                game,
                confidence: Confidence::High,
                score: 1.0,
            });
        }
    }

    // Try table name from VPX metadata
    if let Some(ref table_name) = meta.table_name {
        let normalized = normalize(table_name);
        if !normalized.is_empty() {
            if let Some((game, score)) = find_best_match(&normalized, normalized_games) {
                let confidence = if score >= 0.9 {
                    Confidence::High
                } else if score >= 0.65 {
                    Confidence::Medium
                } else {
                    Confidence::Low
                };
                return Some(MatchResult {
                    file,
                    game,
                    confidence,
                    score,
                });
            }
        }
    }

    None
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
/// - Remove all parenthesized content like "(Williams 1992)" or "(Author Mod)"
/// - Replace separators (underscores, hyphens, dots) with spaces
/// - Strip common suffixes and tokens
/// - Remove version patterns
/// - Collapse whitespace
/// - Trim
fn normalize(name: &str) -> String {
    let mut s = name.to_lowercase();

    // Remove all parenthesized content: "(Williams 1992)", "(Author Mod)", etc.
    while let Some(start) = s.find('(') {
        if let Some(end) = s[start..].find(')') {
            s.replace_range(start..start + end + 1, "");
        } else {
            s.truncate(start);
            break;
        }
    }

    // Replace separators with spaces
    s = s.replace(['_', '-', '.'], " ");

    // Remove common non-game-name tokens wherever they appear as whole words
    let strip_words = [
        "vpx", "vpt", "fx", "fx2", "fx3", "fpt", "vpw",
        "mod", "premium", "le", "pro", "se", "ce",
        "4k", "2k", "1080p",
        "table", "ultradmd", "flexdmd",
        "frankenstein", "release",
    ];

    let words: Vec<&str> = s.split_whitespace().collect();
    let filtered: Vec<&str> = words
        .into_iter()
        .filter(|w| !strip_words.contains(w))
        .collect();
    s = filtered.join(" ");

    // Remove trailing version patterns like "v1.0.0", "1.0", "3 1", "v1 2 2"
    // After separator replacement, these look like "v1 0 0" or "1 2"
    // Also handles no-space versions like "tablev1 0" from "tablev1.0"
    let version_pattern = regex_lite::Regex::new(
        r"\s*v\d+(\s+\d+)*\s*$"
    ).unwrap();
    s = version_pattern.replace(&s, "").to_string();

    // Also strip trailing bare numbers like " 3 1" or " 2 0"
    let trailing_numbers = regex_lite::Regex::new(
        r"\s+\d+(\s+\d+)*\s*$"
    ).unwrap();
    s = trailing_numbers.replace(&s, "").to_string();

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
            vpx_metadata: None,
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
        assert_eq!(normalize("Some.Table.Name"), "some name");
    }

    #[test]
    fn normalize_strips_version_suffixes() {
        assert_eq!(normalize("Medieval_Madness_VPX"), "medieval madness");
        assert_eq!(normalize("Hook_Mod"), "hook");
    }

    #[test]
    fn normalize_strips_parenthesized() {
        assert_eq!(
            normalize("Medieval Madness (Bigus Mod)"),
            "medieval madness"
        );
    }

    #[test]
    fn normalize_real_world_filenames() {
        // Common VPX naming patterns from real collections
        assert_eq!(
            normalize("Fish Tales (Williams 1992) VPW 1.1"),
            "fish tales"
        );
        assert_eq!(
            normalize("Goldeneye (Sega 1996) VPW 1.2"),
            "goldeneye"
        );
        assert_eq!(
            normalize("Judge Dredd (Bally 1993) 3.1"),
            "judge dredd"
        );
        assert_eq!(
            normalize("Metallica Premium Monsters (Stern 2013) VPW 2.0"),
            "metallica monsters"
        );
        assert_eq!(
            normalize("Catacomb (Stern 1981) v2.0.1"),
            "catacomb"
        );
        assert_eq!(
            normalize("Fathom (Bally 1981) FrankEnstein 3.0.2"),
            "fathom"
        );
        assert_eq!(
            normalize("KILLERINSTINCTv1.0"),
            "killerinstinct"
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

    fn make_rom_scanned(stem: &str) -> ScannedFile {
        ScannedFile {
            path: PathBuf::from(format!("/roms/{stem}.zip")),
            resource_type: ResourceType::Roms,
            stem: stem.to_string(),
            size: 500,
            vpx_metadata: None,
        }
    }

    fn make_game_with_roms(id: &str, name: &str, rom_ids: &[&str]) -> Game {
        let mut game = make_game(id, name);
        game.rom_files = rom_ids
            .iter()
            .map(|rom| crate::vpsdb::models::RomFile {
                id: rom.to_string(),
                version: Some(rom.to_string()),
                authors: vec![],
                urls: vec![],
                comment: None,
                name: None,
                created_at: None,
                updated_at: None,
                game: None,
            })
            .collect();
        game
    }

    #[test]
    fn rom_files_matched_by_identifier() {
        let files = vec![
            make_rom_scanned("mm_109c"),
            make_rom_scanned("hook_501"),
            make_rom_scanned("unknown_rom"),
        ];
        let games = vec![
            make_game_with_roms("g1", "Medieval Madness", &["mm_109c", "mm_109b"]),
            make_game_with_roms("g2", "Hook", &["hook_501", "hook_500"]),
        ];

        let results = match_files(&files, &games);
        assert_eq!(results.matches.len(), 2);
        assert_eq!(results.unmatched.len(), 1);

        let mm = results.matches.iter().find(|m| m.game.id == "g1").unwrap();
        assert_eq!(mm.confidence, Confidence::High);
        assert_eq!(mm.file.stem, "mm_109c");

        let hook = results.matches.iter().find(|m| m.game.id == "g2").unwrap();
        assert_eq!(hook.confidence, Confidence::High);
        assert_eq!(hook.file.stem, "hook_501");
    }

    #[test]
    fn rom_match_is_case_insensitive() {
        let files = vec![make_rom_scanned("MM_109C")];
        let games = vec![make_game_with_roms("g1", "Medieval Madness", &["mm_109c"])];

        let results = match_files(&files, &games);
        assert_eq!(results.matches.len(), 1);
        assert_eq!(results.matches[0].confidence, Confidence::High);
    }
}
