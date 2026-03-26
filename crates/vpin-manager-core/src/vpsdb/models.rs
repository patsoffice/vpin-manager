use serde::Deserialize;

/// A game entry in the VPS database.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub name: String,
    pub manufacturer: Option<String>,
    pub year: Option<u16>,
    #[serde(rename = "type")]
    pub game_type: Option<String>,
    pub players: Option<i32>,
    #[serde(default)]
    pub theme: Vec<String>,
    #[serde(default)]
    pub designers: Vec<String>,
    #[serde(default)]
    pub features: Vec<String>,
    pub ipdb_url: Option<String>,
    #[serde(rename = "MPU")]
    pub mpu: Option<String>,
    pub img_url: Option<String>,
    pub broken: Option<bool>,
    pub updated_at: Option<i64>,
    pub last_created_at: Option<i64>,

    // Resource arrays
    #[serde(default)]
    pub table_files: Vec<TableFile>,
    #[serde(default)]
    pub b2s_files: Vec<B2sFile>,
    #[serde(default)]
    pub rom_files: Vec<RomFile>,
    #[serde(default)]
    pub wheel_art_files: Vec<ResourceFile>,
    #[serde(default)]
    pub topper_files: Vec<ResourceFile>,
    #[serde(default)]
    pub pup_pack_files: Vec<ResourceFile>,
    #[serde(default)]
    pub alt_sound_files: Vec<ResourceFile>,
    #[serde(default)]
    pub alt_color_files: Vec<AltColorFile>,
    #[serde(default)]
    pub tutorial_files: Vec<TutorialFile>,
    #[serde(default)]
    pub pov_files: Vec<ResourceFile>,
    #[serde(default)]
    pub media_pack_files: Vec<ResourceFile>,
    #[serde(default)]
    pub rule_files: Vec<ResourceFile>,
    #[serde(default)]
    pub sound_files: Vec<ResourceFile>,
}

/// Common resource file used by most resource types (toppers, pov, media packs,
/// rule files, sound files, alt sound, pup packs, wheel art).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceFile {
    pub id: String,
    pub version: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub urls: Vec<ResourceUrl>,
    pub comment: Option<String>,
    pub name: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub game: Option<GameRef>,
}

/// Table file with format and table-specific fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableFile {
    pub id: String,
    pub version: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub urls: Vec<ResourceUrl>,
    pub comment: Option<String>,
    pub img_url: Option<String>,
    pub table_format: Option<String>,
    pub edition: Option<String>,
    #[serde(default)]
    pub theme: Vec<String>,
    pub game_file_name: Option<String>,
    pub parent_id: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub game: Option<GameRef>,
}

/// Backglass file with feature tags and images.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct B2sFile {
    pub id: String,
    pub version: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub urls: Vec<ResourceUrl>,
    pub comment: Option<String>,
    pub img_url: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub game: Option<GameRef>,
}

/// ROM file with name field for ROM identifier (e.g., "hook_501").
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RomFile {
    pub id: String,
    pub version: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub urls: Vec<ResourceUrl>,
    pub comment: Option<String>,
    pub name: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub game: Option<GameRef>,
}

/// Alt color file with DMD-specific fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AltColorFile {
    pub id: String,
    pub version: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub urls: Vec<ResourceUrl>,
    pub comment: Option<String>,
    #[serde(rename = "type")]
    pub color_type: Option<String>,
    pub file_name: Option<String>,
    pub folder: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub game: Option<GameRef>,
}

/// Tutorial file with title and optional YouTube ID.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TutorialFile {
    pub id: String,
    pub title: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub urls: Vec<ResourceUrl>,
    pub youtube_id: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub game: Option<GameRef>,
}

/// A download URL with optional broken flag.
#[derive(Debug, Clone, Deserialize)]
pub struct ResourceUrl {
    pub url: String,
    pub broken: Option<bool>,
}

/// Back-reference to the parent game.
#[derive(Debug, Clone, Deserialize)]
pub struct GameRef {
    pub id: String,
    pub name: String,
}

impl Game {
    /// Total number of resource files across all types.
    pub fn resource_count(&self) -> usize {
        self.table_files.len()
            + self.b2s_files.len()
            + self.rom_files.len()
            + self.wheel_art_files.len()
            + self.topper_files.len()
            + self.pup_pack_files.len()
            + self.alt_sound_files.len()
            + self.alt_color_files.len()
            + self.tutorial_files.len()
            + self.pov_files.len()
            + self.media_pack_files.len()
            + self.rule_files.len()
            + self.sound_files.len()
    }
}
