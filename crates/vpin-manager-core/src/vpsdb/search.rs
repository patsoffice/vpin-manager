use crate::vpsdb::models::Game;

/// Criteria for filtering games.
#[derive(Debug, Default, Clone)]
pub struct SearchQuery {
    /// Free text search against game name (case-insensitive substring).
    pub text: Option<String>,
    /// Filter by manufacturer (case-insensitive substring).
    pub manufacturer: Option<String>,
    /// Filter by year (exact match).
    pub year: Option<u16>,
    /// Filter by game type: SS, EM, PM, DG.
    pub game_type: Option<String>,
    /// Filter to games that have at least one table in this format (VPX, VP9, FP, etc.).
    pub table_format: Option<String>,
    /// Filter by resource author (case-insensitive substring across all resource types).
    pub author: Option<String>,
}

/// Sort order for search results.
#[derive(Debug, Default, Clone, Copy)]
pub enum SortOrder {
    #[default]
    Name,
    Year,
    Manufacturer,
    LastUpdated,
}

/// Search the game list with filters, sorting, and pagination.
pub fn search<'a>(
    games: &'a [Game],
    query: &SearchQuery,
    sort: SortOrder,
    offset: usize,
    limit: usize,
) -> SearchResults<'a> {
    let mut results: Vec<&Game> = games.iter().filter(|g| matches_query(g, query)).collect();

    match sort {
        SortOrder::Name => {
            results.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }
        SortOrder::Year => results.sort_by(|a, b| a.year.cmp(&b.year)),
        SortOrder::Manufacturer => results.sort_by(|a, b| {
            let ma = a.manufacturer.as_deref().unwrap_or("");
            let mb = b.manufacturer.as_deref().unwrap_or("");
            ma.to_lowercase().cmp(&mb.to_lowercase())
        }),
        SortOrder::LastUpdated => {
            results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        }
    }

    let total = results.len();
    let items: Vec<&Game> = results.into_iter().skip(offset).take(limit).collect();

    SearchResults { items, total }
}

pub struct SearchResults<'a> {
    pub items: Vec<&'a Game>,
    pub total: usize,
}

fn matches_query(game: &Game, query: &SearchQuery) -> bool {
    if let Some(ref text) = query.text {
        let lower = text.to_lowercase();
        if !game.name.to_lowercase().contains(&lower) {
            return false;
        }
    }

    if let Some(ref mfr) = query.manufacturer {
        let lower = mfr.to_lowercase();
        match &game.manufacturer {
            Some(m) if m.to_lowercase().contains(&lower) => {}
            _ => return false,
        }
    }

    if let Some(year) = query.year
        && game.year != Some(year)
    {
        return false;
    }

    if let Some(ref gt) = query.game_type {
        let lower = gt.to_lowercase();
        match &game.game_type {
            Some(t) if t.to_lowercase() == lower => {}
            _ => return false,
        }
    }

    if let Some(ref fmt) = query.table_format {
        let lower = fmt.to_lowercase();
        let has_format = game.table_files.iter().any(|t| {
            t.table_format
                .as_ref()
                .is_some_and(|f| f.to_lowercase() == lower)
        });
        if !has_format {
            return false;
        }
    }

    if let Some(ref author) = query.author {
        let lower = author.to_lowercase();
        if !game_has_author(game, &lower) {
            return false;
        }
    }

    true
}

fn game_has_author(game: &Game, author_lower: &str) -> bool {
    let check = |authors: &[String]| {
        authors
            .iter()
            .any(|a| a.to_lowercase().contains(author_lower))
    };

    game.table_files.iter().any(|r| check(&r.authors))
        || game.b2s_files.iter().any(|r| check(&r.authors))
        || game.rom_files.iter().any(|r| check(&r.authors))
        || game.wheel_art_files.iter().any(|r| check(&r.authors))
        || game.topper_files.iter().any(|r| check(&r.authors))
        || game.pup_pack_files.iter().any(|r| check(&r.authors))
        || game.alt_sound_files.iter().any(|r| check(&r.authors))
        || game.alt_color_files.iter().any(|r| check(&r.authors))
        || game.tutorial_files.iter().any(|r| check(&r.authors))
        || game.pov_files.iter().any(|r| check(&r.authors))
        || game.media_pack_files.iter().any(|r| check(&r.authors))
        || game.rule_files.iter().any(|r| check(&r.authors))
        || game.sound_files.iter().any(|r| check(&r.authors))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vpsdb::models::*;

    fn make_game(name: &str, manufacturer: &str, year: u16) -> Game {
        Game {
            id: name.to_lowercase().replace(' ', "-"),
            name: name.to_string(),
            manufacturer: Some(manufacturer.to_string()),
            year: Some(year),
            game_type: Some("SS".to_string()),
            players: Some(4),
            theme: vec![],
            designers: vec![],
            features: vec![],
            ipdb_url: None,
            mpu: None,
            img_url: None,
            broken: None,
            updated_at: Some(1000),
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

    fn make_table_file(format: &str, author: &str) -> TableFile {
        TableFile {
            id: "t1".to_string(),
            version: None,
            authors: vec![author.to_string()],
            features: vec![],
            urls: vec![],
            comment: None,
            img_url: None,
            table_format: Some(format.to_string()),
            edition: None,
            theme: vec![],
            game_file_name: None,
            parent_id: None,
            created_at: None,
            updated_at: None,
            game: None,
        }
    }

    fn sample_games() -> Vec<Game> {
        let mut g1 = make_game("Medieval Madness", "Williams", 1997);
        g1.table_files = vec![make_table_file("VPX", "Flupper")];
        g1.updated_at = Some(3000);

        let mut g2 = make_game("Hook", "Data East", 1992);
        g2.table_files = vec![make_table_file("VPX", "Bigus1")];
        g2.updated_at = Some(2000);

        let mut g3 = make_game("Elvis", "Stern", 2004);
        g3.table_files = vec![make_table_file("FP", "SomeAuthor")];
        g3.updated_at = Some(1000);

        vec![g1, g2, g3]
    }

    #[test]
    fn search_by_name() {
        let games = sample_games();
        let query = SearchQuery {
            text: Some("medieval".to_string()),
            ..Default::default()
        };
        let results = search(&games, &query, SortOrder::Name, 0, 50);
        assert_eq!(results.total, 1);
        assert_eq!(results.items[0].name, "Medieval Madness");
    }

    #[test]
    fn search_by_manufacturer() {
        let games = sample_games();
        let query = SearchQuery {
            manufacturer: Some("data east".to_string()),
            ..Default::default()
        };
        let results = search(&games, &query, SortOrder::Name, 0, 50);
        assert_eq!(results.total, 1);
        assert_eq!(results.items[0].name, "Hook");
    }

    #[test]
    fn search_by_year() {
        let games = sample_games();
        let query = SearchQuery {
            year: Some(1992),
            ..Default::default()
        };
        let results = search(&games, &query, SortOrder::Name, 0, 50);
        assert_eq!(results.total, 1);
        assert_eq!(results.items[0].name, "Hook");
    }

    #[test]
    fn search_by_table_format() {
        let games = sample_games();
        let query = SearchQuery {
            table_format: Some("FP".to_string()),
            ..Default::default()
        };
        let results = search(&games, &query, SortOrder::Name, 0, 50);
        assert_eq!(results.total, 1);
        assert_eq!(results.items[0].name, "Elvis");
    }

    #[test]
    fn search_by_author() {
        let games = sample_games();
        let query = SearchQuery {
            author: Some("bigus".to_string()),
            ..Default::default()
        };
        let results = search(&games, &query, SortOrder::Name, 0, 50);
        assert_eq!(results.total, 1);
        assert_eq!(results.items[0].name, "Hook");
    }

    #[test]
    fn search_combined_filters() {
        let games = sample_games();
        let query = SearchQuery {
            manufacturer: Some("williams".to_string()),
            table_format: Some("VPX".to_string()),
            ..Default::default()
        };
        let results = search(&games, &query, SortOrder::Name, 0, 50);
        assert_eq!(results.total, 1);
        assert_eq!(results.items[0].name, "Medieval Madness");
    }

    #[test]
    fn search_no_results() {
        let games = sample_games();
        let query = SearchQuery {
            text: Some("nonexistent".to_string()),
            ..Default::default()
        };
        let results = search(&games, &query, SortOrder::Name, 0, 50);
        assert_eq!(results.total, 0);
    }

    #[test]
    fn sort_by_last_updated() {
        let games = sample_games();
        let query = SearchQuery::default();
        let results = search(&games, &query, SortOrder::LastUpdated, 0, 50);
        assert_eq!(results.items[0].name, "Medieval Madness");
        assert_eq!(results.items[1].name, "Hook");
        assert_eq!(results.items[2].name, "Elvis");
    }

    #[test]
    fn pagination() {
        let games = sample_games();
        let query = SearchQuery::default();
        let page1 = search(&games, &query, SortOrder::Name, 0, 2);
        assert_eq!(page1.total, 3);
        assert_eq!(page1.items.len(), 2);

        let page2 = search(&games, &query, SortOrder::Name, 2, 2);
        assert_eq!(page2.total, 3);
        assert_eq!(page2.items.len(), 1);
    }

    #[test]
    fn empty_query_returns_all() {
        let games = sample_games();
        let query = SearchQuery::default();
        let results = search(&games, &query, SortOrder::Name, 0, 50);
        assert_eq!(results.total, 3);
    }
}
