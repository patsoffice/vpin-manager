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

/// An export profile mapping resource types to relative paths under a base directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportProfile {
    pub name: String,
    pub base_dir: PathBuf,
    pub mappings: HashMap<ResourceType, PathBuf>,
}

impl ExportProfile {
    /// Built-in VPX (standard install) profile.
    pub fn vpx(base_dir: PathBuf) -> Self {
        let mut mappings = HashMap::new();
        mappings.insert(ResourceType::Tables, PathBuf::from("Tables"));
        mappings.insert(ResourceType::Backglasses, PathBuf::from("Tables"));
        mappings.insert(ResourceType::Roms, PathBuf::from("VPinMAME/roms"));
        mappings.insert(ResourceType::AltColor, PathBuf::from("VPinMAME/altcolor"));
        mappings.insert(ResourceType::AltSound, PathBuf::from("VPinMAME/altsound"));
        mappings.insert(
            ResourceType::PupPacks,
            PathBuf::from("PinUPSystem/PUPVideos"),
        );
        mappings.insert(ResourceType::Sound, PathBuf::from("Music"));
        mappings.insert(ResourceType::Pov, PathBuf::from("POV"));
        mappings.insert(ResourceType::WheelArt, PathBuf::from("Tables"));
        mappings.insert(ResourceType::Toppers, PathBuf::from("Tables"));
        mappings.insert(ResourceType::MediaPacks, PathBuf::from("MediaPacks"));
        mappings.insert(ResourceType::Rules, PathBuf::from("Rules"));
        mappings.insert(ResourceType::Tutorials, PathBuf::from("Tutorials"));

        Self {
            name: "vpx".to_string(),
            base_dir,
            mappings,
        }
    }

    /// Built-in VPX-standalone profile.
    pub fn vpx_standalone(base_dir: PathBuf) -> Self {
        let mut mappings = HashMap::new();
        mappings.insert(ResourceType::Tables, PathBuf::from("tables"));
        mappings.insert(ResourceType::Backglasses, PathBuf::from("tables"));
        mappings.insert(ResourceType::Roms, PathBuf::from("roms"));
        mappings.insert(ResourceType::AltColor, PathBuf::from("altcolor"));
        mappings.insert(ResourceType::AltSound, PathBuf::from("altsound"));
        mappings.insert(ResourceType::PupPacks, PathBuf::from("pupvideos"));
        mappings.insert(ResourceType::Sound, PathBuf::from("music"));
        mappings.insert(ResourceType::Pov, PathBuf::from("pov"));
        mappings.insert(ResourceType::WheelArt, PathBuf::from("tables"));
        mappings.insert(ResourceType::Toppers, PathBuf::from("tables"));
        mappings.insert(ResourceType::MediaPacks, PathBuf::from("mediapacks"));
        mappings.insert(ResourceType::Rules, PathBuf::from("rules"));
        mappings.insert(ResourceType::Tutorials, PathBuf::from("tutorials"));

        Self {
            name: "vpx-standalone".to_string(),
            base_dir,
            mappings,
        }
    }

    /// Resolve the full path for a resource type.
    pub fn path_for(&self, resource_type: ResourceType) -> PathBuf {
        match self.mappings.get(&resource_type) {
            Some(rel) => self.base_dir.join(rel),
            None => self.base_dir.clone(),
        }
    }
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
    fn vpx_profile_paths() {
        let profile = ExportProfile::vpx(PathBuf::from("/vpinball"));
        assert_eq!(
            profile.path_for(ResourceType::Tables),
            PathBuf::from("/vpinball/Tables")
        );
        assert_eq!(
            profile.path_for(ResourceType::Roms),
            PathBuf::from("/vpinball/VPinMAME/roms")
        );
        assert_eq!(
            profile.path_for(ResourceType::PupPacks),
            PathBuf::from("/vpinball/PinUPSystem/PUPVideos")
        );
    }

    #[test]
    fn vpx_standalone_profile_paths() {
        let profile = ExportProfile::vpx_standalone(PathBuf::from("/vpx"));
        assert_eq!(
            profile.path_for(ResourceType::Tables),
            PathBuf::from("/vpx/tables")
        );
        assert_eq!(
            profile.path_for(ResourceType::Roms),
            PathBuf::from("/vpx/roms")
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
