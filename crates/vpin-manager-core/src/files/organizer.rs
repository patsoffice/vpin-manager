use std::path::{Path, PathBuf};

use crate::config::{ExportProfile, PathContext, ResourceType};

/// How to handle the source file when organizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAction {
    /// Move the file (rename/move, falling back to copy+delete).
    Move,
    /// Copy the file, leaving the original in place.
    Copy,
}

/// Result of organizing a single file.
#[derive(Debug)]
pub struct OrganizeResult {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub action: FileAction,
}

/// Errors that can occur during file organization.
#[derive(Debug)]
pub enum OrganizeError {
    Io(std::io::Error),
    SourceNotFound(PathBuf),
    DestinationExists(PathBuf),
}

impl std::fmt::Display for OrganizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrganizeError::Io(e) => write!(f, "I/O error: {e}"),
            OrganizeError::SourceNotFound(p) => {
                write!(f, "source file not found: {}", p.display())
            }
            OrganizeError::DestinationExists(p) => {
                write!(f, "destination already exists: {}", p.display())
            }
        }
    }
}

impl std::error::Error for OrganizeError {}

impl From<std::io::Error> for OrganizeError {
    fn from(e: std::io::Error) -> Self {
        OrganizeError::Io(e)
    }
}

/// Compute the destination path for a file given a profile and resource type.
/// For per-game profiles, `game_name` is required to resolve `{game}` placeholders.
/// For flat profiles, `game_name` creates an optional subdirectory.
pub fn destination_path(
    profile: &ExportProfile,
    resource_type: ResourceType,
    file_name: &str,
    game_name: Option<&str>,
    rom_name: Option<&str>,
) -> PathBuf {
    let ctx = PathContext {
        game_name,
        rom_name,
    };
    let dest = profile.resolve_path(resource_type, &ctx);
    dest.join(file_name)
}

/// Organize a single file: move or copy it to the appropriate directory
/// based on the export profile.
pub fn organize_file(
    source: &Path,
    profile: &ExportProfile,
    resource_type: ResourceType,
    game_name: Option<&str>,
    rom_name: Option<&str>,
    action: FileAction,
    overwrite: bool,
) -> Result<OrganizeResult, OrganizeError> {
    if !source.exists() {
        return Err(OrganizeError::SourceNotFound(source.to_path_buf()));
    }

    let file_name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let dest = destination_path(profile, resource_type, file_name, game_name, rom_name);

    if dest.exists() && !overwrite {
        return Err(OrganizeError::DestinationExists(dest));
    }

    // Ensure destination directory exists
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match action {
        FileAction::Move => {
            // Try rename first (fast, same filesystem)
            if std::fs::rename(source, &dest).is_err() {
                // Fall back to copy + delete (cross-filesystem)
                std::fs::copy(source, &dest)?;
                std::fs::remove_file(source)?;
            }
        }
        FileAction::Copy => {
            std::fs::copy(source, &dest)?;
        }
    }

    Ok(OrganizeResult {
        source: source.to_path_buf(),
        destination: dest,
        action,
    })
}

/// Organize multiple files, collecting results and errors.
/// Each entry is (source_path, resource_type, game_name, rom_name).
pub fn organize_files(
    files: &[(PathBuf, ResourceType, Option<String>, Option<String>)],
    profile: &ExportProfile,
    action: FileAction,
    overwrite: bool,
) -> (Vec<OrganizeResult>, Vec<(PathBuf, OrganizeError)>) {
    let mut results = Vec::new();
    let mut errors = Vec::new();

    for (source, resource_type, game_name, rom_name) in files {
        match organize_file(
            source,
            profile,
            *resource_type,
            game_name.as_deref(),
            rom_name.as_deref(),
            action,
            overwrite,
        ) {
            Ok(result) => results.push(result),
            Err(e) => errors.push((source.clone(), e)),
        }
    }

    (results, errors)
}

/// Sanitize a game name for use as a directory name.
/// Removes characters that are invalid in filesystem paths.
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn flat_profile(base: &Path) -> ExportProfile {
        ExportProfile::vpx(base.to_path_buf())
    }

    fn per_game_profile(base: &Path) -> ExportProfile {
        ExportProfile::vpx_standalone(base.to_path_buf())
    }

    // --- Flat profile (VPX) tests ---

    #[test]
    fn flat_destination_path() {
        let profile = flat_profile(Path::new("/vpinball"));
        let dest = destination_path(&profile, ResourceType::Tables, "hook.vpx", None, None);
        assert_eq!(dest, PathBuf::from("/vpinball/Tables/hook.vpx"));
    }

    #[test]
    fn flat_roms_path() {
        let profile = flat_profile(Path::new("/vpinball"));
        let dest = destination_path(&profile, ResourceType::Roms, "mm_109c.zip", None, None);
        assert_eq!(dest, PathBuf::from("/vpinball/VPinMAME/roms/mm_109c.zip"));
    }

    // --- Per-game profile (VPX-standalone/Batocera) tests ---

    #[test]
    fn per_game_table_path() {
        let profile = per_game_profile(Path::new("/vpx"));
        let dest = destination_path(
            &profile,
            ResourceType::Tables,
            "hook.vpx",
            Some("Hook"),
            None,
        );
        assert_eq!(dest, PathBuf::from("/vpx/Hook/hook.vpx"));
    }

    #[test]
    fn per_game_backglass_same_dir() {
        let profile = per_game_profile(Path::new("/vpx"));
        let dest = destination_path(
            &profile,
            ResourceType::Backglasses,
            "hook.directb2s",
            Some("Hook"),
            None,
        );
        assert_eq!(dest, PathBuf::from("/vpx/Hook/hook.directb2s"));
    }

    #[test]
    fn per_game_roms_nested() {
        let profile = per_game_profile(Path::new("/vpx"));
        let dest = destination_path(
            &profile,
            ResourceType::Roms,
            "hook_501.zip",
            Some("Hook"),
            None,
        );
        assert_eq!(dest, PathBuf::from("/vpx/Hook/pinmame/roms/hook_501.zip"));
    }

    #[test]
    fn per_game_altcolor_nested() {
        let profile = per_game_profile(Path::new("/vpx"));
        let dest = destination_path(
            &profile,
            ResourceType::AltColor,
            "hook.pal",
            Some("Hook"),
            None,
        );
        assert_eq!(dest, PathBuf::from("/vpx/Hook/pinmame/altcolor/hook.pal"));
    }

    #[test]
    fn per_game_music_nested() {
        let profile = per_game_profile(Path::new("/vpx"));
        let dest = destination_path(
            &profile,
            ResourceType::Sound,
            "theme.mp3",
            Some("Hook"),
            None,
        );
        assert_eq!(dest, PathBuf::from("/vpx/Hook/music/theme.mp3"));
    }

    #[test]
    fn per_game_sanitizes_name() {
        let profile = per_game_profile(Path::new("/vpx"));
        let dest = destination_path(
            &profile,
            ResourceType::Tables,
            "table.vpx",
            Some("AC/DC: Let There Be Rock"),
            None,
        );
        assert_eq!(
            dest,
            PathBuf::from("/vpx/AC_DC_ Let There Be Rock/table.vpx")
        );
    }

    // --- File operation tests ---

    #[test]
    fn copy_file_flat() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let source = base.join("source/hook.vpx");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"fake vpx data").unwrap();

        let profile = flat_profile(&base.join("output"));

        let result = organize_file(
            &source,
            &profile,
            ResourceType::Tables,
            None,
            None,
            FileAction::Copy,
            false,
        )
        .unwrap();

        assert_eq!(result.destination, base.join("output/Tables/hook.vpx"));
        assert!(result.destination.exists());
        assert!(source.exists());
    }

    #[test]
    fn copy_file_per_game() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let source = base.join("hook.vpx");
        fs::write(&source, b"fake").unwrap();

        let profile = per_game_profile(&base.join("output"));

        let result = organize_file(
            &source,
            &profile,
            ResourceType::Tables,
            Some("Hook"),
            None,
            FileAction::Copy,
            false,
        )
        .unwrap();

        assert_eq!(result.destination, base.join("output/Hook/hook.vpx"));
        assert!(result.destination.exists());
    }

    #[test]
    fn move_file() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let source = base.join("source/hook.vpx");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"fake vpx data").unwrap();

        let profile = flat_profile(&base.join("output"));

        let result = organize_file(
            &source,
            &profile,
            ResourceType::Tables,
            None,
            None,
            FileAction::Move,
            false,
        )
        .unwrap();

        assert!(result.destination.exists());
        assert!(!source.exists());
    }

    #[test]
    fn error_on_source_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let profile = flat_profile(dir.path());

        let result = organize_file(
            Path::new("/nonexistent/file.vpx"),
            &profile,
            ResourceType::Tables,
            None,
            None,
            FileAction::Copy,
            false,
        );

        assert!(matches!(result, Err(OrganizeError::SourceNotFound(_))));
    }

    #[test]
    fn error_on_destination_exists_without_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let source = base.join("hook.vpx");
        fs::write(&source, b"source").unwrap();

        let profile = flat_profile(&base.join("output"));
        let dest = base.join("output/Tables/hook.vpx");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, b"existing").unwrap();

        let result = organize_file(
            &source,
            &profile,
            ResourceType::Tables,
            None,
            None,
            FileAction::Copy,
            false,
        );

        assert!(matches!(result, Err(OrganizeError::DestinationExists(_))));
    }

    #[test]
    fn overwrite_replaces_existing() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let source = base.join("hook.vpx");
        fs::write(&source, b"new content").unwrap();

        let profile = flat_profile(&base.join("output"));
        let dest = base.join("output/Tables/hook.vpx");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, b"old content").unwrap();

        let result = organize_file(
            &source,
            &profile,
            ResourceType::Tables,
            None,
            None,
            FileAction::Copy,
            true,
        )
        .unwrap();

        assert_eq!(fs::read(&result.destination).unwrap(), b"new content");
    }

    #[test]
    fn organize_multiple_per_game() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let f1 = base.join("hook.vpx");
        let f2 = base.join("hook.directb2s");
        let f3 = base.join("missing.vpx");
        fs::write(&f1, b"vpx").unwrap();
        fs::write(&f2, b"b2s").unwrap();

        let profile = per_game_profile(&base.join("output"));

        let files = vec![
            (f1, ResourceType::Tables, Some("Hook".to_string()), None),
            (
                f2,
                ResourceType::Backglasses,
                Some("Hook".to_string()),
                None,
            ),
            (f3, ResourceType::Tables, None, None),
        ];

        let (results, errors) = organize_files(&files, &profile, FileAction::Copy, false);

        assert_eq!(results.len(), 2);
        assert_eq!(errors.len(), 1);
        // Both go into the Hook game directory
        assert!(results[0].destination.to_string_lossy().contains("/Hook/"));
        assert!(results[1].destination.to_string_lossy().contains("/Hook/"));
    }
}
