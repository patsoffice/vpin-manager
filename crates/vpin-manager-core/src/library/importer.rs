use crate::config::ResourceType;
use crate::library::scanner::ScannedFile;
use crate::vpsdb::models::{B2sFile, Game, ResourceFile, RomFile, TableFile};

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

/// A reference to a specific resource file entry within a VPS game.
#[derive(Debug, Clone)]
pub enum MatchedResource<'a> {
    Table(&'a TableFile),
    Backglass(&'a B2sFile),
    Rom(&'a RomFile),
    Other(&'a ResourceFile),
}

impl<'a> MatchedResource<'a> {
    pub fn id(&self) -> &str {
        match self {
            MatchedResource::Table(t) => &t.id,
            MatchedResource::Backglass(b) => &b.id,
            MatchedResource::Rom(r) => &r.id,
            MatchedResource::Other(o) => &o.id,
        }
    }

    pub fn version(&self) -> Option<&str> {
        match self {
            MatchedResource::Table(t) => t.version.as_deref(),
            MatchedResource::Backglass(b) => b.version.as_deref(),
            MatchedResource::Rom(r) => r.version.as_deref(),
            MatchedResource::Other(o) => o.version.as_deref(),
        }
    }

    pub fn authors(&self) -> &[String] {
        match self {
            MatchedResource::Table(t) => &t.authors,
            MatchedResource::Backglass(b) => &b.authors,
            MatchedResource::Rom(r) => &r.authors,
            MatchedResource::Other(o) => &o.authors,
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
    /// The specific resource file entry within the game, if identified.
    pub matched_resource: Option<MatchedResource<'a>>,
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
pub fn match_files<'a>(files: &'a [ScannedFile], games: &'a [Game]) -> ImportResults<'a> {
    // Pre-compute normalized game names for faster comparison.
    let normalized_games: Vec<(&Game, String)> =
        games.iter().map(|g| (g, normalize(&g.name))).collect();

    // Build a ROM identifier -> list of Games lookup from VPS DB romFiles.
    // Multiple games can share the same ROM (e.g., original vs. JP's retheme).
    let mut rom_games_multi: std::collections::HashMap<String, Vec<&Game>> =
        std::collections::HashMap::new();
    for g in games {
        for r in &g.rom_files {
            if let Some(ref v) = r.version {
                rom_games_multi.entry(v.to_lowercase()).or_default().push(g);
            }
            if let Some(ref n) = r.name {
                rom_games_multi.entry(n.to_lowercase()).or_default().push(g);
            }
        }
    }

    // Also collect ROM names from VPX metadata we've already read.
    let mut vpx_rom_games: std::collections::HashMap<String, Vec<&Game>> =
        std::collections::HashMap::new();
    for f in files {
        if let Some(meta) = f.vpx_metadata.as_ref()
            && let Some(rom) = meta.rom_name.as_ref()
        {
            let normalized = normalize(&f.stem);
            let author = meta.author_name.as_deref();
            if let Some((game, _)) =
                find_best_match_with_author(&normalized, &normalized_games, author)
            {
                vpx_rom_games
                    .entry(rom.to_lowercase())
                    .or_default()
                    .push(game);
            }
        }
    }

    let mut matches = Vec::new();
    let mut unmatched = Vec::new();

    for file in files {
        // Try VPX metadata first (most accurate)
        if let Some(result) = try_vpx_metadata_match(file, &normalized_games, &rom_games_multi) {
            matches.push(result);
            continue;
        }

        // Try B2S metadata — game_name is a ROM identifier
        if let Some(ref meta) = file.b2s_metadata
            && let Some(ref game_name) = meta.game_name
        {
            let lower = game_name.to_lowercase();
            let b2s_author = meta.author.as_deref();
            if let Some(game) = resolve_rom_game(&rom_games_multi, &lower, b2s_author)
                .or_else(|| resolve_rom_game(&vpx_rom_games, &lower, b2s_author))
            {
                let matched_resource = find_matching_b2s_file(file, game);
                matches.push(MatchResult {
                    file,
                    game,
                    confidence: Confidence::High,
                    score: 1.0,
                    matched_resource,
                });
                continue;
            }
        }

        // For ROM files, try matching stem against known ROM identifiers
        if file.resource_type == ResourceType::Roms {
            let lower_stem = file.stem.to_lowercase();
            if let Some(game) = resolve_rom_game(&rom_games_multi, &lower_stem, None)
                .or_else(|| resolve_rom_game(&vpx_rom_games, &lower_stem, None))
            {
                let matched_resource = find_matching_rom_file(file, game);
                matches.push(MatchResult {
                    file,
                    game,
                    confidence: Confidence::High,
                    score: 1.0,
                    matched_resource,
                });
                continue;
            }
        }

        // Fall back to filename-based matching
        let normalized_stem = normalize(&file.stem);
        let file_author = file
            .vpx_metadata
            .as_ref()
            .and_then(|m| m.author_name.as_deref())
            .or_else(|| file.b2s_metadata.as_ref().and_then(|m| m.author.as_deref()));

        if let Some((game, score)) =
            find_best_match_with_author(&normalized_stem, &normalized_games, file_author)
        {
            let confidence = if score >= 0.95 {
                Confidence::High
            } else if score >= 0.7 {
                Confidence::Medium
            } else {
                Confidence::Low
            };

            let matched_resource = find_resource_for_file(file, game);
            matches.push(MatchResult {
                file,
                game,
                confidence,
                score,
                matched_resource,
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
    rom_games: &std::collections::HashMap<String, Vec<&'a Game>>,
) -> Option<MatchResult<'a>> {
    let meta = file.vpx_metadata.as_ref()?;

    // Try ROM name match first (most precise)
    if let Some(ref rom) = meta.rom_name
        && let Some(game) =
            resolve_rom_game(rom_games, &rom.to_lowercase(), meta.author_name.as_deref())
    {
        let matched_resource = find_matching_table_file(file, game);
        return Some(MatchResult {
            file,
            game,
            confidence: Confidence::High,
            score: 1.0,
            matched_resource,
        });
    }

    // Try table name from VPX metadata
    if let Some(ref table_name) = meta.table_name {
        let normalized = normalize(table_name);
        if !normalized.is_empty() {
            let author = meta.author_name.as_deref();
            if let Some((game, score)) =
                find_best_match_with_author(&normalized, normalized_games, author)
            {
                let confidence = if score >= 0.9 {
                    Confidence::High
                } else if score >= 0.65 {
                    Confidence::Medium
                } else {
                    Confidence::Low
                };
                let matched_resource = find_matching_table_file(file, game);
                return Some(MatchResult {
                    file,
                    game,
                    confidence,
                    score,
                    matched_resource,
                });
            }
        }
    }

    None
}

// --- Resource-level matching ---

/// Dispatch to the appropriate resource matcher based on file type.
fn find_resource_for_file<'a>(file: &ScannedFile, game: &'a Game) -> Option<MatchedResource<'a>> {
    match file.resource_type {
        ResourceType::Tables => find_matching_table_file(file, game),
        ResourceType::Backglasses => find_matching_b2s_file(file, game),
        ResourceType::Roms => find_matching_rom_file(file, game),
        _ => None,
    }
}

/// Find the best matching TableFile within a game using VPX metadata.
fn find_matching_table_file<'a>(file: &ScannedFile, game: &'a Game) -> Option<MatchedResource<'a>> {
    if game.table_files.is_empty() {
        return None;
    }
    if game.table_files.len() == 1 {
        return Some(MatchedResource::Table(&game.table_files[0]));
    }

    let meta = file.vpx_metadata.as_ref();

    let mut best: Option<(&TableFile, u32)> = None;

    for tf in &game.table_files {
        let mut score: u32 = 0;

        // Format match (VPX files are always VPX format)
        if tf.table_format.as_deref() == Some("VPX") {
            score += 1;
        }

        // Author match
        if let Some(file_author) = meta.and_then(|m| m.author_name.as_ref()) {
            let lower = file_author.to_lowercase();
            if tf.authors.iter().any(|a| {
                let a_lower = a.to_lowercase();
                a_lower.contains(&lower) || lower.contains(&a_lower)
            }) {
                score += 3;
            }
        }

        // Version match
        if let Some(file_version) = meta.and_then(|m| m.table_version.as_ref())
            && let Some(ref tf_version) = tf.version
            && tf_version == file_version
        {
            score += 2;
        }

        if score > 0 && (best.is_none() || score > best.unwrap().1) {
            best = Some((tf, score));
        }
    }

    best.map(|(tf, _)| MatchedResource::Table(tf))
}

/// Find the best matching B2sFile within a game using B2S metadata.
fn find_matching_b2s_file<'a>(file: &ScannedFile, game: &'a Game) -> Option<MatchedResource<'a>> {
    if game.b2s_files.is_empty() {
        return None;
    }
    if game.b2s_files.len() == 1 {
        return Some(MatchedResource::Backglass(&game.b2s_files[0]));
    }

    let meta = file.b2s_metadata.as_ref();

    // Try author match
    if let Some(file_author) = meta.and_then(|m| m.author.as_ref()) {
        let lower = file_author.to_lowercase();
        for bf in &game.b2s_files {
            if bf.authors.iter().any(|a| {
                let a_lower = a.to_lowercase();
                a_lower.contains(&lower) || lower.contains(&a_lower)
            }) {
                return Some(MatchedResource::Backglass(bf));
            }
        }
    }

    None
}

/// Find the matching RomFile within a game by ROM identifier.
fn find_matching_rom_file<'a>(file: &ScannedFile, game: &'a Game) -> Option<MatchedResource<'a>> {
    let lower_stem = file.stem.to_lowercase();

    for rf in &game.rom_files {
        if rf
            .version
            .as_ref()
            .is_some_and(|v| v.to_lowercase() == lower_stem)
        {
            return Some(MatchedResource::Rom(rf));
        }
        if rf
            .name
            .as_ref()
            .is_some_and(|n| n.to_lowercase() == lower_stem)
        {
            return Some(MatchedResource::Rom(rf));
        }
    }

    None
}

// --- Matching helpers ---

/// Pick the best game from a ROM lookup, using author to disambiguate
/// when multiple games share the same ROM.
fn resolve_rom_game<'a>(
    rom_map: &std::collections::HashMap<String, Vec<&'a Game>>,
    rom_key: &str,
    author: Option<&str>,
) -> Option<&'a Game> {
    let candidates = rom_map.get(rom_key)?;
    if candidates.len() == 1 {
        return Some(candidates[0]);
    }
    // If we have author info, prefer the game that has a resource by this author
    if let Some(author) = author {
        for &game in candidates {
            if game_has_matching_author(game, author) {
                return Some(game);
            }
        }
    }
    // Fall back to first candidate
    Some(candidates[0])
}

/// Find the best matching game for a normalized file stem.
/// When `file_author` is provided, games whose resources list that author
/// get a score bonus to break ties between similarly-named games.
fn find_best_match_with_author<'a>(
    stem: &str,
    games: &[(&'a Game, String)],
    file_author: Option<&str>,
) -> Option<(&'a Game, f64)> {
    let mut best: Option<(&Game, f64)> = None;

    for (game, normalized_name) in games {
        let mut score = similarity(stem, normalized_name);

        if score >= 0.5 {
            // Boost score if author matches any resource in this game
            if let Some(author) = file_author
                && game_has_matching_author(game, author)
            {
                score += 0.1;
            }

            if best.is_none() || score > best.unwrap().1 {
                best = Some((game, score));
            }
        }
    }

    best
}

/// Check if any resource in a game lists the given author.
fn game_has_matching_author(game: &Game, author: &str) -> bool {
    let lower = author.to_lowercase();
    let check = |authors: &[String]| {
        authors.iter().any(|a| {
            let a_lower = a.to_lowercase();
            a_lower.contains(&lower) || lower.contains(&a_lower)
        })
    };

    game.table_files.iter().any(|r| check(&r.authors))
        || game.b2s_files.iter().any(|r| check(&r.authors))
        || game.rom_files.iter().any(|r| check(&r.authors))
}

/// Normalize a name for comparison.
fn normalize(name: &str) -> String {
    let mut s = name.to_lowercase();

    // Remove all parenthesized content
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

    // Remove common non-game-name tokens
    let strip_words = [
        "vpx",
        "vpt",
        "fx",
        "fx2",
        "fx3",
        "fpt",
        "vpw",
        "mod",
        "premium",
        "le",
        "pro",
        "se",
        "ce",
        "4k",
        "2k",
        "1080p",
        "table",
        "ultradmd",
        "flexdmd",
        "frankenstein",
        "release",
    ];

    let words: Vec<&str> = s.split_whitespace().collect();
    let filtered: Vec<&str> = words
        .into_iter()
        .filter(|w| !strip_words.contains(w))
        .collect();
    s = filtered.join(" ");

    // Remove trailing version patterns
    let version_pattern = regex_lite::Regex::new(r"\s*v\d+(\s+\d+)*\s*$").unwrap();
    s = version_pattern.replace(&s, "").to_string();

    let trailing_numbers = regex_lite::Regex::new(r"\s+\d+(\s+\d+)*\s*$").unwrap();
    s = trailing_numbers.replace(&s, "").to_string();

    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Compute similarity between two normalized strings.
fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    // Containment check
    if a.contains(b) || b.contains(a) {
        let shorter = a.len().min(b.len()) as f64;
        let longer = a.len().max(b.len()) as f64;
        let ratio = shorter / longer;
        return 0.75 + (ratio * 0.23);
    }

    // Word overlap (Jaccard similarity)
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
    use std::path::PathBuf;

    fn make_scanned(stem: &str) -> ScannedFile {
        ScannedFile {
            path: PathBuf::from(format!("/tables/{stem}.vpx")),
            resource_type: ResourceType::Tables,
            stem: stem.to_string(),
            size: 1000,
            vpx_metadata: None,
            b2s_metadata: None,
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

    fn make_table_file(id: &str, authors: &[&str], version: &str) -> TableFile {
        TableFile {
            id: id.to_string(),
            version: Some(version.to_string()),
            authors: authors.iter().map(|a| a.to_string()).collect(),
            features: vec![],
            urls: vec![],
            comment: None,
            img_url: None,
            table_format: Some("VPX".to_string()),
            edition: None,
            theme: vec![],
            game_file_name: None,
            parent_id: None,
            created_at: None,
            updated_at: None,
            game: None,
        }
    }

    fn make_b2s_file(id: &str, authors: &[&str], version: &str) -> B2sFile {
        B2sFile {
            id: id.to_string(),
            version: Some(version.to_string()),
            authors: authors.iter().map(|a| a.to_string()).collect(),
            features: vec![],
            urls: vec![],
            comment: None,
            img_url: None,
            created_at: None,
            updated_at: None,
            game: None,
        }
    }

    fn make_rom_file(id: &str, rom_id: &str) -> RomFile {
        RomFile {
            id: id.to_string(),
            version: Some(rom_id.to_string()),
            authors: vec![],
            urls: vec![],
            comment: None,
            name: None,
            created_at: None,
            updated_at: None,
            game: None,
        }
    }

    // --- Normalize tests ---

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
        assert_eq!(
            normalize("Fish Tales (Williams 1992) VPW 1.1"),
            "fish tales"
        );
        assert_eq!(normalize("Goldeneye (Sega 1996) VPW 1.2"), "goldeneye");
        assert_eq!(normalize("Judge Dredd (Bally 1993) 3.1"), "judge dredd");
        assert_eq!(
            normalize("Metallica Premium Monsters (Stern 2013) VPW 2.0"),
            "metallica monsters"
        );
        assert_eq!(normalize("Catacomb (Stern 1981) v2.0.1"), "catacomb");
        assert_eq!(
            normalize("Fathom (Bally 1981) FrankEnstein 3.0.2"),
            "fathom"
        );
        assert_eq!(normalize("KILLERINSTINCTv1.0"), "killerinstinct");
    }

    // --- Game-level matching tests ---

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
        assert!(results.matches[0].score >= 0.5);
    }

    #[test]
    fn multiple_files_matched() {
        let files = vec![
            make_scanned("Medieval_Madness"),
            make_scanned("Hook"),
            make_scanned("Unknown_Game_XYZ"),
        ];
        let games = vec![make_game("g1", "Medieval Madness"), make_game("g2", "Hook")];

        let results = match_files(&files, &games);
        assert_eq!(results.matches.len(), 2);
        assert_eq!(results.unmatched.len(), 1);
    }

    // --- Similarity tests ---

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

    // --- ROM matching tests ---

    fn make_rom_scanned(stem: &str) -> ScannedFile {
        ScannedFile {
            path: PathBuf::from(format!("/roms/{stem}.zip")),
            resource_type: ResourceType::Roms,
            stem: stem.to_string(),
            size: 500,
            vpx_metadata: None,
            b2s_metadata: None,
        }
    }

    fn make_game_with_roms(id: &str, name: &str, rom_ids: &[&str]) -> Game {
        let mut game = make_game(id, name);
        game.rom_files = rom_ids.iter().map(|rom| make_rom_file(rom, rom)).collect();
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
    fn rom_match_identifies_specific_rom_file() {
        let files = vec![make_rom_scanned("hook_501")];
        let games = vec![make_game_with_roms("g1", "Hook", &["hook_501", "hook_500"])];

        let results = match_files(&files, &games);
        let m = &results.matches[0];
        assert!(m.matched_resource.is_some());
        let resource = m.matched_resource.as_ref().unwrap();
        assert_eq!(resource.id(), "hook_501");
    }

    // --- B2S matching tests ---

    fn make_b2s_scanned(stem: &str, game_name: &str) -> ScannedFile {
        ScannedFile {
            path: PathBuf::from(format!("/tables/{stem}.directb2s")),
            resource_type: ResourceType::Backglasses,
            stem: stem.to_string(),
            size: 2000,
            vpx_metadata: None,
            b2s_metadata: Some(crate::b2s::B2sMetadata {
                name: Some(stem.to_string()),
                game_name: Some(game_name.to_string()),
                author: None,
            }),
        }
    }

    #[test]
    fn b2s_matched_by_rom_name() {
        let files = vec![
            make_b2s_scanned("SomeBackglass", "mm_109c"),
            make_b2s_scanned("AnotherGlass", "unknown_rom"),
        ];
        let games = vec![make_game_with_roms("g1", "Medieval Madness", &["mm_109c"])];

        let results = match_files(&files, &games);
        assert_eq!(results.matches.len(), 1);
        assert_eq!(results.matches[0].game.id, "g1");
        assert_eq!(results.matches[0].confidence, Confidence::High);
    }

    #[test]
    fn rom_match_is_case_insensitive() {
        let files = vec![make_rom_scanned("MM_109C")];
        let games = vec![make_game_with_roms("g1", "Medieval Madness", &["mm_109c"])];

        let results = match_files(&files, &games);
        assert_eq!(results.matches.len(), 1);
        assert_eq!(results.matches[0].confidence, Confidence::High);
    }

    // --- Resource-level matching tests ---

    #[test]
    fn single_table_file_always_matched() {
        let mut game = make_game("g1", "Hook");
        game.table_files = vec![make_table_file("tf1", &["Author1"], "1.0")];

        let files = vec![make_scanned("Hook")];
        let games = vec![game];

        let results = match_files(&files, &games);
        let m = &results.matches[0];
        assert!(m.matched_resource.is_some());
        assert_eq!(m.matched_resource.as_ref().unwrap().id(), "tf1");
    }

    #[test]
    fn table_file_matched_by_author() {
        let mut game = make_game("g1", "Hook");
        game.table_files = vec![
            make_table_file("tf1", &["Bigus1", "Javier15"], "3.0"),
            make_table_file("tf2", &["VPW", "Javier15"], "1.0"),
            make_table_file("tf3", &["Arconovum", "Javier15"], "1.21"),
        ];

        let mut file = make_scanned("Hook");
        file.vpx_metadata = Some(crate::vpx::VpxMetadata {
            table_name: Some("Hook".to_string()),
            author_name: Some("Bigus1".to_string()),
            table_version: None,
            rom_name: None,
            requires_pinmame: false,
            table_description: None,
            table_blurb: None,
            release_date: None,
        });

        let files = vec![file];
        let games = vec![game];

        let results = match_files(&files, &games);
        let m = &results.matches[0];
        assert!(m.matched_resource.is_some());
        assert_eq!(m.matched_resource.as_ref().unwrap().id(), "tf1");
    }

    #[test]
    fn table_file_matched_by_author_and_version() {
        let mut game = make_game("g1", "Hook");
        game.table_files = vec![
            make_table_file("tf1", &["Bigus1"], "3.0"),
            make_table_file("tf2", &["Bigus1"], "2.0"),
        ];

        let mut file = make_scanned("Hook");
        file.vpx_metadata = Some(crate::vpx::VpxMetadata {
            table_name: Some("Hook".to_string()),
            author_name: Some("Bigus1".to_string()),
            table_version: Some("3.0".to_string()),
            rom_name: None,
            requires_pinmame: false,
            table_description: None,
            table_blurb: None,
            release_date: None,
        });

        let files = vec![file];
        let games = vec![game];

        let results = match_files(&files, &games);
        let m = &results.matches[0];
        assert!(m.matched_resource.is_some());
        assert_eq!(m.matched_resource.as_ref().unwrap().id(), "tf1");
        assert_eq!(m.matched_resource.as_ref().unwrap().version(), Some("3.0"));
    }

    #[test]
    fn b2s_file_matched_by_author() {
        let mut game = make_game("g1", "Hook");
        game.rom_files = vec![make_rom_file("r1", "hook_501")];
        game.b2s_files = vec![
            make_b2s_file("b1", &["HauntFreaks"], "2.0"),
            make_b2s_file("b2", &["Wildman"], "1.0"),
        ];

        let file = ScannedFile {
            path: PathBuf::from("/tables/Hook.directb2s"),
            resource_type: ResourceType::Backglasses,
            stem: "Hook".to_string(),
            size: 2000,
            vpx_metadata: None,
            b2s_metadata: Some(crate::b2s::B2sMetadata {
                name: Some("Hook".to_string()),
                game_name: Some("hook_501".to_string()),
                author: Some("HauntFreaks".to_string()),
            }),
        };

        let files = vec![file];
        let games = vec![game];

        let results = match_files(&files, &games);
        let m = &results.matches[0];
        assert!(m.matched_resource.is_some());
        assert_eq!(m.matched_resource.as_ref().unwrap().id(), "b1");
    }

    #[test]
    fn matched_resource_provides_version_and_authors() {
        let mut game = make_game("g1", "Hook");
        game.table_files = vec![make_table_file("tf1", &["Author1", "Author2"], "2.5")];

        let files = vec![make_scanned("Hook")];
        let games = vec![game];

        let results = match_files(&files, &games);
        let resource = results.matches[0].matched_resource.as_ref().unwrap();
        assert_eq!(resource.version(), Some("2.5"));
        assert_eq!(resource.authors(), &["Author1", "Author2"]);
    }

    #[test]
    fn no_metadata_still_matches_on_format() {
        let mut game = make_game("g1", "Hook");
        game.table_files = vec![
            make_table_file("tf1", &["Author1"], "1.0"),
            make_table_file("tf2", &["Author2"], "2.0"),
        ];

        // No VPX metadata — both are VPX format so one gets picked
        let files = vec![make_scanned("Hook")];
        let games = vec![game];

        let results = match_files(&files, &games);
        let m = &results.matches[0];
        // Still picks a resource based on format match alone
        assert!(m.matched_resource.is_some());
    }

    #[test]
    fn no_resource_match_with_no_table_files() {
        let game = make_game("g1", "Hook");

        let files = vec![make_scanned("Hook")];
        let games = vec![game];

        let results = match_files(&files, &games);
        let m = &results.matches[0];
        assert!(m.matched_resource.is_none());
    }
}
