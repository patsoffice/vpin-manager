use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use vpin_manager_core::config::AppConfig;
use vpin_manager_core::vpsdb::fetch::{self, SyncResult, VpsDb};
use vpin_manager_core::vpsdb::search::{self, SearchQuery, SortOrder};

#[derive(Parser)]
#[command(name = "vpin-manager", about = "Virtual pinball resource library manager")]
struct Cli {
    /// Override the data/cache directory
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fetch or refresh the VPS database
    Sync {
        /// Force a full download even if the cache is current
        #[arg(long, short)]
        force: bool,
    },

    /// Search for games in the VPS database
    Search {
        /// Search text (matches game name)
        query: Option<String>,

        /// Filter by manufacturer
        #[arg(long, short)]
        manufacturer: Option<String>,

        /// Filter by year
        #[arg(long, short)]
        year: Option<u16>,

        /// Filter by table format (VPX, VP9, FP, FX, FX3)
        #[arg(long, short)]
        format: Option<String>,

        /// Filter by game type (SS, EM)
        #[arg(long, short = 't')]
        game_type: Option<String>,

        /// Filter by resource author
        #[arg(long, short)]
        author: Option<String>,

        /// Sort order: name, year, manufacturer, updated
        #[arg(long, short, default_value = "name")]
        sort: String,

        /// Maximum number of results
        #[arg(long, short = 'n', default_value = "25")]
        limit: usize,
    },

    /// Show details for a specific game
    Show {
        /// Game ID or name (exact match on ID, substring match on name)
        game: String,
    },

    /// Show or update configuration
    Config {
        /// Show the config file path
        #[arg(long)]
        path: bool,

        /// Initialize a default config file
        #[arg(long)]
        init: bool,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let cache_dir = fetch::resolve_cache_dir(cli.data_dir.as_deref());

    match cli.command {
        Command::Sync { force } => cmd_sync(&cache_dir, force).await,
        Command::Search {
            query,
            manufacturer,
            year,
            format,
            game_type,
            author,
            sort,
            limit,
        } => {
            let sort_order = parse_sort_order(&sort)?;
            let search_query = SearchQuery {
                text: query,
                manufacturer,
                year,
                game_type,
                table_format: format,
                author,
            };
            cmd_search(&cache_dir, &search_query, sort_order, limit)
        }
        Command::Show { game } => cmd_show(&cache_dir, &game),
        Command::Config { path, init } => cmd_config(path, init),
    }
}

async fn cmd_sync(
    cache_dir: &PathBuf,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let vpsdb = VpsDb::new(cache_dir.clone());

    println!("Checking VPS database...");
    let result = if force {
        vpsdb.force_sync().await?
    } else {
        vpsdb.sync().await?
    };

    match result {
        SyncResult::Updated { game_count } => {
            println!("Database updated: {game_count} games");
        }
        SyncResult::AlreadyCurrent { game_count } => {
            println!("Database is already current: {game_count} games");
        }
    }

    Ok(())
}

fn cmd_search(
    cache_dir: &PathBuf,
    query: &SearchQuery,
    sort: SortOrder,
    limit: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let vpsdb = VpsDb::new(cache_dir.clone());
    let games = vpsdb.load_cached()?;

    let results = search::search(&games, query, sort, 0, limit);

    if results.total == 0 {
        println!("No games found.");
        return Ok(());
    }

    println!(
        "Found {} game{}{}:\n",
        results.total,
        if results.total == 1 { "" } else { "s" },
        if results.total > limit {
            format!(" (showing first {limit})")
        } else {
            String::new()
        }
    );

    // Column widths
    let id_w = 12;
    let name_w = 35;
    let mfr_w = 15;
    let year_w = 4;

    println!(
        "{:<id_w$}  {:<name_w$}  {:<mfr_w$}  {:>year_w$}  Resources",
        "ID", "Name", "Manufacturer", "Year",
    );
    println!(
        "{:<id_w$}  {:<name_w$}  {:<mfr_w$}  {:>year_w$}  ---------",
        "---", "----", "------------", "----",
    );

    for game in &results.items {
        let id = truncate(&game.id, id_w);
        let name = truncate(&game.name, name_w);
        let mfr = truncate(game.manufacturer.as_deref().unwrap_or("-"), mfr_w);
        let year = game
            .year
            .map(|y| y.to_string())
            .unwrap_or_else(|| "-".to_string());
        let resources = game.resource_count();

        println!("{id:<id_w$}  {name:<name_w$}  {mfr:<mfr_w$}  {year:>year_w$}  {resources}");
    }

    Ok(())
}

fn cmd_show(
    cache_dir: &PathBuf,
    game_query: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let vpsdb = VpsDb::new(cache_dir.clone());
    let games = vpsdb.load_cached()?;

    // Try exact ID match first, then substring name match
    let game = games
        .iter()
        .find(|g| g.id == game_query)
        .or_else(|| {
            let lower = game_query.to_lowercase();
            let matches: Vec<_> = games
                .iter()
                .filter(|g| g.name.to_lowercase().contains(&lower))
                .collect();
            if matches.len() == 1 {
                Some(matches[0])
            } else {
                None
            }
        });

    let Some(game) = game else {
        // Check if multiple name matches
        let lower = game_query.to_lowercase();
        let matches: Vec<_> = games
            .iter()
            .filter(|g| g.name.to_lowercase().contains(&lower))
            .collect();
        if matches.len() > 1 {
            println!("Multiple games match '{game_query}':");
            for g in &matches {
                println!(
                    "  {} - {} ({})",
                    g.id,
                    g.name,
                    g.manufacturer.as_deref().unwrap_or("?")
                );
            }
            println!("\nUse the game ID to select a specific one.");
        } else {
            println!("No game found matching '{game_query}'.");
        }
        return Ok(());
    };

    // Header
    println!("{}", game.name);
    println!("{}", "=".repeat(game.name.len()));
    if let Some(ref mfr) = game.manufacturer {
        print!("Manufacturer: {mfr}");
    }
    if let Some(year) = game.year {
        print!("  Year: {year}");
    }
    if let Some(ref gt) = game.game_type {
        print!("  Type: {gt}");
    }
    if let Some(players) = game.players {
        print!("  Players: {players}");
    }
    println!();
    println!("ID: {}", game.id);

    if !game.theme.is_empty() {
        println!("Themes: {}", game.theme.join(", "));
    }
    if !game.designers.is_empty() {
        println!("Designers: {}", game.designers.join(", "));
    }
    if let Some(ref mpu) = game.mpu {
        println!("MPU: {mpu}");
    }
    if let Some(ref url) = game.ipdb_url {
        println!("IPDB: {url}");
    }
    println!();

    // Resources
    print_table_files(&game.table_files);
    print_resource_section("Backglasses", &game.b2s_files, |r| {
        format_features(&r.features)
    });
    print_resource_section("ROMs", &game.rom_files, |r| {
        r.name.clone().unwrap_or_default()
    });
    print_simple_section("Wheel Art", &game.wheel_art_files);
    print_simple_section("Toppers", &game.topper_files);
    print_simple_section("PuP Packs", &game.pup_pack_files);
    print_simple_section("Alt Sound", &game.alt_sound_files);
    print_resource_section("Alt Color", &game.alt_color_files, |r| {
        r.color_type.clone().unwrap_or_default()
    });
    print_tutorial_section(&game.tutorial_files);
    print_simple_section("POV", &game.pov_files);
    print_simple_section("Media Packs", &game.media_pack_files);
    print_simple_section("Rules", &game.rule_files);
    print_simple_section("Sound", &game.sound_files);

    Ok(())
}

fn print_table_files(tables: &[vpin_manager_core::vpsdb::models::TableFile]) {
    if tables.is_empty() {
        return;
    }
    println!("Tables ({}):", tables.len());
    for t in tables {
        let fmt = t.table_format.as_deref().unwrap_or("?");
        let ver = t.version.as_deref().unwrap_or("-");
        let authors = t.authors.join(", ");
        let features = format_features(&t.features);
        println!("  [{fmt}] v{ver} by {authors}");
        if !features.is_empty() {
            println!("    Features: {features}");
        }
        if let Some(ref comment) = t.comment {
            println!("    {comment}");
        }
        for url in &t.urls {
            let broken = if url.broken == Some(true) { " [BROKEN]" } else { "" };
            println!("    -> {}{broken}", url.url);
        }
    }
    println!();
}

fn print_resource_section<T, F>(label: &str, items: &[T], extra: F)
where
    T: HasResourceFields,
    F: Fn(&T) -> String,
{
    if items.is_empty() {
        return;
    }
    println!("{label} ({}):", items.len());
    for item in items {
        let ver = item.version().unwrap_or("-");
        let authors = item.authors().join(", ");
        let extra_str = extra(item);
        if extra_str.is_empty() {
            println!("  v{ver} by {authors}");
        } else {
            println!("  v{ver} by {authors} ({extra_str})");
        }
        for url in item.urls() {
            let broken = if url.broken == Some(true) { " [BROKEN]" } else { "" };
            println!("    -> {}{broken}", url.url);
        }
    }
    println!();
}

fn print_simple_section(label: &str, items: &[vpin_manager_core::vpsdb::models::ResourceFile]) {
    print_resource_section(label, items, |_| String::new());
}

fn print_tutorial_section(tutorials: &[vpin_manager_core::vpsdb::models::TutorialFile]) {
    if tutorials.is_empty() {
        return;
    }
    println!("Tutorials ({}):", tutorials.len());
    for t in tutorials {
        let title = t.title.as_deref().unwrap_or("Untitled");
        let authors = t.authors.join(", ");
        println!("  {title} by {authors}");
        if let Some(ref yt) = t.youtube_id {
            println!("    -> https://www.youtube.com/watch?v={yt}");
        }
        if let Some(ref url) = t.url {
            println!("    -> {url}");
        }
        for url in &t.urls {
            let broken = if url.broken == Some(true) { " [BROKEN]" } else { "" };
            println!("    -> {}{broken}", url.url);
        }
    }
    println!();
}

fn cmd_config(show_path: bool, init: bool) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = AppConfig::default_path()
        .ok_or("could not determine config directory")?;

    if show_path {
        println!("{}", config_path.display());
        return Ok(());
    }

    if init {
        if config_path.exists() {
            println!("Config file already exists at {}", config_path.display());
        } else {
            let config = AppConfig::default();
            config.save(&config_path)?;
            println!("Created default config at {}", config_path.display());
        }
        return Ok(());
    }

    // Default: show current config
    let config = AppConfig::load(&config_path)?;
    println!("Config file: {}", config_path.display());
    println!("Active profile: {}", config.active_profile);
    println!("Web port: {}", config.web_port);
    println!();

    for profile in &config.profiles {
        println!("Profile: {}", profile.name);
        println!("  Base directory: {}", profile.base_dir.display());
        for rt in vpin_manager_core::config::ResourceType::ALL {
            if let Some(rel) = profile.mappings.get(rt) {
                println!("  {rt}: {}", rel.display());
            }
        }
        println!();
    }

    Ok(())
}

// --- Helpers ---

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

fn format_features(features: &[String]) -> String {
    features.join(", ")
}

fn parse_sort_order(s: &str) -> Result<SortOrder, Box<dyn std::error::Error>> {
    match s.to_lowercase().as_str() {
        "name" => Ok(SortOrder::Name),
        "year" => Ok(SortOrder::Year),
        "manufacturer" | "mfr" => Ok(SortOrder::Manufacturer),
        "updated" | "last_updated" => Ok(SortOrder::LastUpdated),
        _ => Err(format!("unknown sort order '{s}', expected: name, year, manufacturer, updated").into()),
    }
}

/// Trait to abstract over resource file types for display.
trait HasResourceFields {
    fn version(&self) -> Option<&str>;
    fn authors(&self) -> &[String];
    fn urls(&self) -> &[vpin_manager_core::vpsdb::models::ResourceUrl];
}

impl HasResourceFields for vpin_manager_core::vpsdb::models::ResourceFile {
    fn version(&self) -> Option<&str> { self.version.as_deref() }
    fn authors(&self) -> &[String] { &self.authors }
    fn urls(&self) -> &[vpin_manager_core::vpsdb::models::ResourceUrl] { &self.urls }
}

impl HasResourceFields for vpin_manager_core::vpsdb::models::B2sFile {
    fn version(&self) -> Option<&str> { self.version.as_deref() }
    fn authors(&self) -> &[String] { &self.authors }
    fn urls(&self) -> &[vpin_manager_core::vpsdb::models::ResourceUrl] { &self.urls }
}

impl HasResourceFields for vpin_manager_core::vpsdb::models::RomFile {
    fn version(&self) -> Option<&str> { self.version.as_deref() }
    fn authors(&self) -> &[String] { &self.authors }
    fn urls(&self) -> &[vpin_manager_core::vpsdb::models::ResourceUrl] { &self.urls }
}

impl HasResourceFields for vpin_manager_core::vpsdb::models::AltColorFile {
    fn version(&self) -> Option<&str> { self.version.as_deref() }
    fn authors(&self) -> &[String] { &self.authors }
    fn urls(&self) -> &[vpin_manager_core::vpsdb::models::ResourceUrl] { &self.urls }
}
