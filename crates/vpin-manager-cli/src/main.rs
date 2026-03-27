use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, Subcommand};

use vpin_manager_core::config::{AppConfig, ResourceType};
use vpin_manager_core::files::organizer::{self, FileAction};
use vpin_manager_core::library::db::{InstalledResource, LibraryDb};
use vpin_manager_core::library::importer::{self, Confidence};
use vpin_manager_core::library::scanner;
use vpin_manager_core::vpsdb::fetch::{self, SyncResult, VpsDb};
use vpin_manager_core::vpsdb::models::Game;
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

    /// Scan a directory, match files to VPS games, and register in library
    Import {
        /// Directory to scan
        dir: PathBuf,

        /// Only show high-confidence matches
        #[arg(long)]
        high_only: bool,

        /// Automatically confirm all matches (skip review)
        #[arg(long, short = 'y')]
        yes: bool,

        /// Library name
        #[arg(long, default_value = "default")]
        library: String,
    },

    /// List installed resources or check for updates
    Library {
        #[command(subcommand)]
        action: Option<LibraryAction>,

        /// Library name
        #[arg(long, default_value = "default")]
        library: String,
    },

    /// Move files into the configured folder structure
    Organize {
        /// Directory to scan for files to organize
        dir: PathBuf,

        /// Copy instead of move
        #[arg(long)]
        copy: bool,

        /// Create game-name subdirectories
        #[arg(long)]
        game_dirs: bool,

        /// Library name (used to look up game names for matched files)
        #[arg(long, default_value = "default")]
        library: String,
    },
}

#[derive(Subcommand)]
enum LibraryAction {
    /// Check for updates against the VPS database
    Status,
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
        Command::Import {
            dir,
            high_only,
            yes,
            library,
        } => cmd_import(&cache_dir, &dir, high_only, yes, &library),
        Command::Library { action, library } => cmd_library(&cache_dir, action, &library),
        Command::Organize {
            dir,
            copy,
            game_dirs,
            library,
        } => cmd_organize(&dir, copy, game_dirs, &library),
    }
}

// --- Sync ---

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

// --- Search ---

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

// --- Show ---

fn cmd_show(
    cache_dir: &PathBuf,
    game_query: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let vpsdb = VpsDb::new(cache_dir.clone());
    let games = vpsdb.load_cached()?;

    let game = find_game(&games, game_query);

    let Some(game) = game else {
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

// --- Config ---

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

    let config = AppConfig::load(&config_path)?;
    println!("Config file: {}", config_path.display());
    println!("Active profile: {}", config.active_profile);
    println!("Web port: {}", config.web_port);
    println!();

    for profile in &config.profiles {
        println!("Profile: {}", profile.name);
        println!("  Base directory: {}", profile.base_dir.display());
        for rt in ResourceType::ALL {
            if let Some(rel) = profile.mappings.get(rt) {
                println!("  {rt}: {}", rel.display());
            }
        }
        println!();
    }

    Ok(())
}

// --- Import ---

fn cmd_import(
    cache_dir: &PathBuf,
    dir: &Path,
    high_only: bool,
    auto_confirm: bool,
    library_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {

    if !dir.exists() {
        return Err(format!("directory not found: {}", dir.display()).into());
    }

    // Load VPS database
    let vpsdb = VpsDb::new(cache_dir.clone());
    let games = vpsdb.load_cached()?;

    // Scan directory
    println!("Scanning {}...", dir.display());
    let scan = scanner::scan_directory(dir);

    if scan.files.is_empty() {
        println!("No virtual pinball files found.");
        return Ok(());
    }

    println!("Found {} files:", scan.files.len());
    for (rt, count) in scan.summary() {
        println!("  {rt}: {count}");
    }
    println!();

    if !scan.errors.is_empty() {
        println!("{} scan errors (skipped):", scan.errors.len());
        for (path, err) in &scan.errors {
            println!("  {}: {err}", path.display());
        }
        println!();
    }

    // Match against VPS database
    println!("Matching against VPS database ({} games)...", games.len());
    let results = importer::match_files(&scan.files, &games);

    let matches: Vec<_> = if high_only {
        results
            .matches
            .into_iter()
            .filter(|m| m.confidence == Confidence::High)
            .collect()
    } else {
        results.matches
    };

    if matches.is_empty() {
        println!("No matches found.");
        return Ok(());
    }

    // Display matches
    println!(
        "\n{} match{}:\n",
        matches.len(),
        if matches.len() == 1 { "" } else { "es" }
    );

    let name_w = 30;
    let game_w = 30;
    let conf_w = 6;

    println!(
        "  {:<name_w$}  {:<game_w$}  {:<conf_w$}  Type",
        "File", "Game", "Conf.",
    );
    println!(
        "  {:<name_w$}  {:<game_w$}  {:<conf_w$}  ----",
        "----", "----", "-----",
    );

    for m in &matches {
        let file_name = truncate(
            m.file.path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
            name_w,
        );
        let game_name = truncate(&m.game.name, game_w);
        println!(
            "  {file_name:<name_w$}  {game_name:<game_w$}  {:<conf_w$}  {}",
            m.confidence.to_string(),
            m.file.resource_type,
        );
    }

    if !results.unmatched.is_empty() {
        println!("\n{} unmatched files:", results.unmatched.len());
        for u in &results.unmatched {
            println!(
                "  {} ({})",
                u.file.path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                u.file.resource_type,
            );
        }
    }

    // Confirm
    if !auto_confirm {
        println!("\nRegister {} matches in library '{library_name}'? [y/N] ", matches.len());
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Register in library
    let db_path = LibraryDb::default_path(library_name)
        .ok_or("could not determine library path")?;
    let db = LibraryDb::open(&db_path)?;

    let mut registered = 0;
    for m in &matches {
        // Use VPS resource ID if we identified a specific resource, otherwise generate one
        let id = match &m.matched_resource {
            Some(r) => r.id().to_string(),
            None => format!(
                "import-{}-{}",
                m.game.id,
                m.file
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
            ),
        };

        // Version: prefer file metadata, fall back to matched VPS resource
        let version = m
            .file
            .vpx_metadata
            .as_ref()
            .and_then(|meta| meta.table_version.clone())
            .or_else(|| {
                m.matched_resource
                    .as_ref()
                    .and_then(|r| r.version().map(|v| v.to_string()))
            });

        // Authors: prefer file metadata, fall back to matched VPS resource
        let authors = {
            let from_file = extract_authors(m.file);
            if from_file.is_empty() {
                m.matched_resource
                    .as_ref()
                    .map(|r| r.authors().to_vec())
                    .unwrap_or_default()
            } else {
                from_file
            }
        };

        let resource = InstalledResource {
            id,
            game_id: m.game.id.clone(),
            game_name: m.game.name.clone(),
            resource_type: m.file.resource_type.to_string().to_lowercase(),
            version,
            file_path: m.file.path.to_string_lossy().to_string(),
            installed_at: None,
            vps_updated_at: m.game.updated_at,
            metadata: Some(format!("{{\"confidence\":\"{}\",\"score\":{:.2}}}", m.confidence, m.score)),
            authors,
        };
        db.upsert_installed(&resource)?;
        registered += 1;
    }

    println!("Registered {registered} resources in library '{library_name}'.");
    Ok(())
}

// --- Library ---

fn cmd_library(
    cache_dir: &PathBuf,
    action: Option<LibraryAction>,
    library_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = LibraryDb::default_path(library_name)
        .ok_or("could not determine library path")?;

    if !db_path.exists() {
        println!("Library '{library_name}' is empty. Use 'vpin-manager import' to add resources.");
        return Ok(());
    }

    let db = LibraryDb::open(&db_path)?;

    match action {
        Some(LibraryAction::Status) => cmd_library_status(cache_dir, &db, library_name),
        None => cmd_library_list(&db, library_name),
    }
}

fn cmd_library_list(
    db: &LibraryDb,
    library_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let resources = db.list_installed(None)?;

    if resources.is_empty() {
        println!("Library '{library_name}' is empty.");
        return Ok(());
    }

    let count = resources.len();
    let game_ids = db.installed_game_ids()?;

    println!(
        "Library '{library_name}': {count} resources across {} games\n",
        game_ids.len()
    );

    let name_w = 30;
    let type_w = 12;
    let ver_w = 8;
    let author_w = 25;

    println!(
        "{:<name_w$}  {:<type_w$}  {:<ver_w$}  {:<author_w$}  Path",
        "Game", "Type", "Version", "Authors",
    );
    println!(
        "{:<name_w$}  {:<type_w$}  {:<ver_w$}  {:<author_w$}  ----",
        "----", "----", "-------", "-------",
    );

    for r in &resources {
        let name = truncate(&r.game_name, name_w);
        let rtype = truncate(&r.resource_type, type_w);
        let ver = truncate(r.version.as_deref().unwrap_or("-"), ver_w);
        let authors = if r.authors.is_empty() {
            "-".to_string()
        } else {
            truncate(&r.authors.join(", "), author_w)
        };
        let path = &r.file_path;
        println!("{name:<name_w$}  {rtype:<type_w$}  {ver:<ver_w$}  {authors:<author_w$}  {path}");
    }

    Ok(())
}

fn cmd_library_status(
    cache_dir: &PathBuf,
    db: &LibraryDb,
    library_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let vpsdb = VpsDb::new(cache_dir.clone());
    let games = vpsdb.load_cached()?;

    let resources = db.list_installed(None)?;

    if resources.is_empty() {
        println!("Library '{library_name}' is empty.");
        return Ok(());
    }

    // Build a lookup of game_id -> game
    let game_map: std::collections::HashMap<&str, &Game> =
        games.iter().map(|g| (g.id.as_str(), g)).collect();

    let mut updates_available = 0;
    let mut up_to_date = 0;
    let mut unknown = 0;

    println!("Checking {} resources for updates...\n", resources.len());

    for r in &resources {
        if let Some(game) = game_map.get(r.game_id.as_str()) {
            let installed_ts = r.vps_updated_at.unwrap_or(0);
            let current_ts = game.updated_at.unwrap_or(0);

            if current_ts > installed_ts {
                updates_available += 1;
                println!(
                    "  UPDATE: {} ({}) - installed: {}, latest: {}",
                    r.game_name,
                    r.resource_type,
                    format_timestamp(installed_ts),
                    format_timestamp(current_ts),
                );
            } else {
                up_to_date += 1;
            }
        } else {
            unknown += 1;
        }
    }

    println!();
    println!("{up_to_date} up to date, {updates_available} updates available, {unknown} not found in VPS DB");

    Ok(())
}

// --- Organize ---

fn cmd_organize(
    dir: &PathBuf,
    copy: bool,
    game_dirs: bool,
    library_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {

    if !dir.exists() {
        return Err(format!("directory not found: {}", dir.display()).into());
    }

    let config_path = AppConfig::default_path()
        .ok_or("could not determine config directory")?;
    let config = AppConfig::load(&config_path)?;
    let profile = config
        .active_profile()
        .ok_or_else(|| format!("active profile '{}' not found in config", config.active_profile))?;

    // Scan directory
    println!("Scanning {}...", dir.display());
    let scan = scanner::scan_directory(dir);

    if scan.files.is_empty() {
        println!("No virtual pinball files found.");
        return Ok(());
    }

    println!("Found {} files.", scan.files.len());

    // Try to load library for game name lookup
    let db = LibraryDb::default_path(library_name)
        .filter(|p| p.exists())
        .and_then(|p| LibraryDb::open(&p).ok());

    let action = if copy {
        FileAction::Copy
    } else {
        FileAction::Move
    };

    let action_verb = if copy { "Copying" } else { "Moving" };

    let mut success = 0;
    let mut errors = 0;

    for file in &scan.files {
        // Look up game name from library if available and game_dirs requested
        let game_name = if game_dirs {
            lookup_game_name(&db, &file.path)
        } else {
            None
        };

        match organizer::organize_file(
            &file.path,
            profile,
            file.resource_type,
            game_name.as_deref(),
            action,
            false,
        ) {
            Ok(result) => {
                println!(
                    "  {action_verb} {} -> {}",
                    result.source.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                    result.destination.display(),
                );
                success += 1;
            }
            Err(e) => {
                eprintln!(
                    "  ERROR {}: {e}",
                    file.path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                );
                errors += 1;
            }
        }
    }

    println!("\n{success} files organized, {errors} errors.");
    Ok(())
}

fn lookup_game_name(db: &Option<LibraryDb>, file_path: &Path) -> Option<String> {

    let db = db.as_ref()?;
    let file_str = file_path.to_string_lossy();

    // Search installed resources for a matching file path
    let resources = db.list_installed(None).ok()?;
    resources
        .iter()
        .find(|r| r.file_path == file_str.as_ref())
        .map(|r| r.game_name.clone())
}

// --- Display helpers ---

fn find_game<'a>(games: &'a [Game], query: &str) -> Option<&'a Game> {
    games
        .iter()
        .find(|g| g.id == query)
        .or_else(|| {
            let lower = query.to_lowercase();
            let matches: Vec<_> = games
                .iter()
                .filter(|g| g.name.to_lowercase().contains(&lower))
                .collect();
            if matches.len() == 1 {
                Some(matches[0])
            } else {
                None
            }
        })
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

/// Extract authors from file metadata (VPX or B2S).
fn extract_authors(file: &vpin_manager_core::library::scanner::ScannedFile) -> Vec<String> {
    if let Some(ref meta) = file.vpx_metadata {
        if let Some(ref author) = meta.author_name {
            return author
                .split([',', '&', '/'])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    if let Some(ref meta) = file.b2s_metadata {
        if let Some(ref author) = meta.author {
            return author
                .split([',', '&', '/'])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    vec![]
}

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

fn format_timestamp(ts: i64) -> String {
    if ts == 0 {
        return "unknown".to_string();
    }
    let secs = ts / 1000;
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "invalid".to_string())
}

fn parse_sort_order(s: &str) -> Result<SortOrder, Box<dyn std::error::Error>> {
    match s.to_lowercase().as_str() {
        "name" => Ok(SortOrder::Name),
        "year" => Ok(SortOrder::Year),
        "manufacturer" | "mfr" => Ok(SortOrder::Manufacturer),
        "updated" | "last_updated" => Ok(SortOrder::LastUpdated),
        _ => Err(
            format!("unknown sort order '{s}', expected: name, year, manufacturer, updated").into(),
        ),
    }
}

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
