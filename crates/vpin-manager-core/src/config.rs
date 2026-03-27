use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Resource types that can be mapped to directories in an export profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Tables,
    Backglasses,
    Roms,
    WheelArt,
    Toppers,
    PupPacks,
    AltSound,
    AltColor,
    Pov,
    MediaPacks,
    Rules,
    Sound,
    Tutorials,
}

impl ResourceType {
    pub const ALL: &[ResourceType] = &[
        ResourceType::Tables,
        ResourceType::Backglasses,
        ResourceType::Roms,
        ResourceType::WheelArt,
        ResourceType::Toppers,
        ResourceType::PupPacks,
        ResourceType::AltSound,
        ResourceType::AltColor,
        ResourceType::Pov,
        ResourceType::MediaPacks,
        ResourceType::Rules,
        ResourceType::Sound,
        ResourceType::Tutorials,
    ];
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::Tables => write!(f, "Tables"),
            ResourceType::Backglasses => write!(f, "Backglasses"),
            ResourceType::Roms => write!(f, "ROMs"),
            ResourceType::WheelArt => write!(f, "Wheel Art"),
            ResourceType::Toppers => write!(f, "Toppers"),
            ResourceType::PupPacks => write!(f, "PuP Packs"),
            ResourceType::AltSound => write!(f, "Alt Sound"),
            ResourceType::AltColor => write!(f, "Alt Color"),
            ResourceType::Pov => write!(f, "POV"),
            ResourceType::MediaPacks => write!(f, "Media Packs"),
            ResourceType::Rules => write!(f, "Rules"),
            ResourceType::Sound => write!(f, "Sound"),
            ResourceType::Tutorials => write!(f, "Tutorials"),
        }
    }
}

/// Context for resolving path templates in export profiles.
#[derive(Debug, Default)]
pub struct PathContext<'a> {
    /// Game name (substituted for `{game}` in path templates).
    pub game_name: Option<&'a str>,
    /// ROM name (substituted for `{rom}` in path templates).
    pub rom_name: Option<&'a str>,
}

/// An export profile mapping resource types to relative path templates under a base directory.
///
/// Path templates can contain placeholders:
/// - `{game}` -- replaced with the sanitized game name
/// - `{rom}` -- replaced with the ROM identifier
///
/// Flat profiles (like VPX standard) use plain paths without placeholders.
/// Per-game profiles (like VPX-standalone/Batocera) use `{game}` to nest resources under game directories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportProfile {
    pub name: String,
    pub base_dir: PathBuf,
    /// Path templates per resource type. May contain `{game}` and `{rom}` placeholders.
    pub mappings: HashMap<ResourceType, String>,
}

impl ExportProfile {
    /// Built-in VPX (standard Windows install) profile.
    /// Flat structure: all tables in Tables/, all ROMs in VPinMAME/roms/, etc.
    pub fn vpx(base_dir: PathBuf) -> Self {
        let mut mappings = HashMap::new();
        mappings.insert(ResourceType::Tables, "Tables".to_string());
        mappings.insert(ResourceType::Backglasses, "Tables".to_string());
        mappings.insert(ResourceType::Roms, "VPinMAME/roms".to_string());
        mappings.insert(ResourceType::AltColor, "VPinMAME/altcolor".to_string());
        mappings.insert(ResourceType::AltSound, "VPinMAME/altsound".to_string());
        mappings.insert(ResourceType::PupPacks, "PinUPSystem/PUPVideos".to_string());
        mappings.insert(ResourceType::Sound, "Music".to_string());
        mappings.insert(ResourceType::Pov, "POV".to_string());
        mappings.insert(ResourceType::WheelArt, "Tables".to_string());
        mappings.insert(ResourceType::Toppers, "Tables".to_string());
        mappings.insert(ResourceType::MediaPacks, "MediaPacks".to_string());
        mappings.insert(ResourceType::Rules, "Rules".to_string());
        mappings.insert(ResourceType::Tutorials, "Tutorials".to_string());

        Self {
            name: "vpx".to_string(),
            base_dir,
            mappings,
        }
    }

    /// Built-in VPX-standalone profile (Batocera recommended layout).
    /// Per-game structure: each game gets its own directory with all resources nested inside.
    pub fn vpx_standalone(base_dir: PathBuf) -> Self {
        let mut mappings = HashMap::new();
        mappings.insert(ResourceType::Tables, "{game}".to_string());
        mappings.insert(ResourceType::Backglasses, "{game}".to_string());
        mappings.insert(ResourceType::Roms, "{game}/pinmame/roms".to_string());
        mappings.insert(
            ResourceType::AltColor,
            "{game}/pinmame/altcolor".to_string(),
        );
        mappings.insert(
            ResourceType::AltSound,
            "{game}/pinmame/altsound".to_string(),
        );
        mappings.insert(ResourceType::PupPacks, "{game}/pupvideos".to_string());
        mappings.insert(ResourceType::Sound, "{game}/music".to_string());
        mappings.insert(ResourceType::Pov, "{game}".to_string());
        mappings.insert(ResourceType::WheelArt, "{game}".to_string());
        mappings.insert(ResourceType::Toppers, "{game}".to_string());
        mappings.insert(ResourceType::MediaPacks, "{game}".to_string());
        mappings.insert(ResourceType::Rules, "{game}".to_string());
        mappings.insert(ResourceType::Tutorials, "{game}".to_string());

        Self {
            name: "vpx-standalone".to_string(),
            base_dir,
            mappings,
        }
    }

    /// Resolve the full path for a resource type, substituting placeholders.
    pub fn resolve_path(&self, resource_type: ResourceType, ctx: &PathContext) -> PathBuf {
        match self.mappings.get(&resource_type) {
            Some(template) => {
                let resolved = resolve_template(template, ctx);
                self.base_dir.join(resolved)
            }
            None => self.base_dir.clone(),
        }
    }

    /// Check if this profile uses per-game paths (contains `{game}` placeholders).
    pub fn is_per_game(&self) -> bool {
        self.mappings.values().any(|v| v.contains("{game}"))
    }
}

/// Resolve `{game}` and `{rom}` placeholders in a path template.
fn resolve_template(template: &str, ctx: &PathContext) -> String {
    let mut result = template.to_string();
    if let Some(game) = ctx.game_name {
        result = result.replace("{game}", &sanitize_dirname(game));
    }
    if let Some(rom) = ctx.rom_name {
        result = result.replace("{rom}", rom);
    }
    result
}

/// Sanitize a name for use as a directory name.
fn sanitize_dirname(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Active export profile name.
    pub active_profile: String,
    /// All configured export profiles.
    pub profiles: Vec<ExportProfile>,
    /// Port for the web UI server.
    pub web_port: u16,
}

impl Default for AppConfig {
    fn default() -> Self {
        let default_base = dirs_default_base();
        Self {
            active_profile: "vpx-standalone".to_string(),
            profiles: vec![
                ExportProfile::vpx(default_base.clone()),
                ExportProfile::vpx_standalone(default_base),
            ],
            web_port: 3000,
        }
    }
}

impl AppConfig {
    /// Load config from a TOML file, or return defaults if it doesn't exist.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save config to a TOML file.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get the active export profile.
    pub fn active_profile(&self) -> Option<&ExportProfile> {
        self.profiles.iter().find(|p| p.name == self.active_profile)
    }

    /// Return the default config file path.
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "vpin-manager")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }
}

fn dirs_default_base() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().join("VPinball"))
        .unwrap_or_else(|| PathBuf::from("~/VPinball"))
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    TomlRead(toml::de::Error),
    TomlWrite(toml::ser::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config I/O error: {e}"),
            ConfigError::TomlRead(e) => write!(f, "config parse error: {e}"),
            ConfigError::TomlWrite(e) => write!(f, "config serialize error: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::TomlRead(e)
    }
}

impl From<toml::ser::Error> for ConfigError {
    fn from(e: toml::ser::Error) -> Self {
        ConfigError::TomlWrite(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_roundtrips_through_toml() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.active_profile, "vpx-standalone");
        assert_eq!(parsed.profiles.len(), 2);
        assert_eq!(parsed.web_port, 3000);
    }

    #[test]
    fn vpx_profile_flat_paths() {
        let profile = ExportProfile::vpx(PathBuf::from("/vpinball"));
        let ctx = PathContext::default();
        assert_eq!(
            profile.resolve_path(ResourceType::Tables, &ctx),
            PathBuf::from("/vpinball/Tables")
        );
        assert_eq!(
            profile.resolve_path(ResourceType::Roms, &ctx),
            PathBuf::from("/vpinball/VPinMAME/roms")
        );
        assert_eq!(
            profile.resolve_path(ResourceType::PupPacks, &ctx),
            PathBuf::from("/vpinball/PinUPSystem/PUPVideos")
        );
        assert!(!profile.is_per_game());
    }

    #[test]
    fn vpx_standalone_per_game_paths() {
        let profile = ExportProfile::vpx_standalone(PathBuf::from("/vpx"));
        let ctx = PathContext {
            game_name: Some("Medieval Madness"),
            rom_name: Some("mm_109c"),
        };

        assert_eq!(
            profile.resolve_path(ResourceType::Tables, &ctx),
            PathBuf::from("/vpx/Medieval Madness")
        );
        assert_eq!(
            profile.resolve_path(ResourceType::Backglasses, &ctx),
            PathBuf::from("/vpx/Medieval Madness")
        );
        assert_eq!(
            profile.resolve_path(ResourceType::Roms, &ctx),
            PathBuf::from("/vpx/Medieval Madness/pinmame/roms")
        );
        assert_eq!(
            profile.resolve_path(ResourceType::AltColor, &ctx),
            PathBuf::from("/vpx/Medieval Madness/pinmame/altcolor")
        );
        assert_eq!(
            profile.resolve_path(ResourceType::AltSound, &ctx),
            PathBuf::from("/vpx/Medieval Madness/pinmame/altsound")
        );
        assert_eq!(
            profile.resolve_path(ResourceType::PupPacks, &ctx),
            PathBuf::from("/vpx/Medieval Madness/pupvideos")
        );
        assert_eq!(
            profile.resolve_path(ResourceType::Sound, &ctx),
            PathBuf::from("/vpx/Medieval Madness/music")
        );
        assert!(profile.is_per_game());
    }

    #[test]
    fn path_sanitizes_game_name() {
        let profile = ExportProfile::vpx_standalone(PathBuf::from("/vpx"));
        let ctx = PathContext {
            game_name: Some("AC/DC: Let There Be Rock"),
            rom_name: None,
        };
        assert_eq!(
            profile.resolve_path(ResourceType::Tables, &ctx),
            PathBuf::from("/vpx/AC_DC_ Let There Be Rock")
        );
    }

    #[test]
    fn all_resource_types_mapped_in_profiles() {
        let vpx = ExportProfile::vpx(PathBuf::from("/test"));
        let standalone = ExportProfile::vpx_standalone(PathBuf::from("/test"));
        for rt in ResourceType::ALL {
            assert!(
                vpx.mappings.contains_key(rt),
                "VPX profile missing mapping for {rt}"
            );
            assert!(
                standalone.mappings.contains_key(rt),
                "VPX-standalone profile missing mapping for {rt}"
            );
        }
    }
}
