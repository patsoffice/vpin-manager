use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::ResourceType;

/// A file discovered during a directory scan.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub resource_type: ResourceType,
    /// Filename without extension, used for matching against game names.
    pub stem: String,
    /// File size in bytes.
    pub size: u64,
}

/// Results of a directory scan.
#[derive(Debug)]
pub struct ScanResults {
    pub files: Vec<ScannedFile>,
    pub errors: Vec<(PathBuf, std::io::Error)>,
}

impl ScanResults {
    /// Group scanned files by resource type.
    pub fn by_type(&self) -> HashMap<ResourceType, Vec<&ScannedFile>> {
        let mut map: HashMap<ResourceType, Vec<&ScannedFile>> = HashMap::new();
        for file in &self.files {
            map.entry(file.resource_type).or_default().push(file);
        }
        map
    }

    /// Summary counts by resource type.
    pub fn summary(&self) -> Vec<(ResourceType, usize)> {
        let grouped = self.by_type();
        let mut counts: Vec<_> = grouped
            .into_iter()
            .map(|(rt, files)| (rt, files.len()))
            .collect();
        counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        counts
    }
}

/// Known file extensions mapped to resource types.
const EXTENSION_MAP: &[(&str, ResourceType)] = &[
    // Tables
    ("vpx", ResourceType::Tables),
    ("vpt", ResourceType::Tables),
    ("fx", ResourceType::Tables),
    ("fx2", ResourceType::Tables),
    ("fx3", ResourceType::Tables),
    ("fpt", ResourceType::Tables),
    // Backglasses
    ("directb2s", ResourceType::Backglasses),
    // POV
    ("pov", ResourceType::Pov),
    // Alt color
    ("pal", ResourceType::AltColor),
    ("vni", ResourceType::AltColor),
    ("cRZ", ResourceType::AltColor),
    // Rules
    ("pdf", ResourceType::Rules),
];

/// File name patterns that indicate a resource type regardless of extension.
const NAME_PATTERNS: &[(&str, ResourceType)] = &[
    ("pup", ResourceType::PupPacks),
    ("puppack", ResourceType::PupPacks),
    ("altsound", ResourceType::AltSound),
    ("alt_sound", ResourceType::AltSound),
];

/// Scan a directory recursively for known virtual pinball file types.
pub fn scan_directory(dir: &Path) -> ScanResults {
    let mut files = Vec::new();
    let mut errors = Vec::new();

    scan_recursive(dir, &mut files, &mut errors);

    ScanResults { files, errors }
}

fn scan_recursive(
    dir: &Path,
    files: &mut Vec<ScannedFile>,
    errors: &mut Vec<(PathBuf, std::io::Error)>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            errors.push((dir.to_path_buf(), e));
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push((dir.to_path_buf(), e));
                continue;
            }
        };

        let path = entry.path();

        if path.is_dir() {
            scan_recursive(&path, files, errors);
            continue;
        }

        if let Some(scanned) = classify_file(&path) {
            files.push(scanned);
        }
    }
}

fn classify_file(path: &Path) -> Option<ScannedFile> {
    let extension = path.extension()?.to_str()?;
    let stem = path.file_stem()?.to_str()?.to_string();
    let size = path.metadata().map(|m| m.len()).unwrap_or(0);

    // Check extension map first
    if let Some(&(_, rt)) = EXTENSION_MAP.iter().find(|(ext, _)| ext.eq_ignore_ascii_case(extension)) {
        return Some(ScannedFile {
            path: path.to_path_buf(),
            resource_type: rt,
            stem,
            size,
        });
    }

    // Check if ZIP/RAR/7z might be a ROM or other resource based on name patterns
    if matches!(
        extension.to_lowercase().as_str(),
        "zip" | "rar" | "7z"
    ) {
        let lower_stem = stem.to_lowercase();

        // Check name patterns
        for &(pattern, rt) in NAME_PATTERNS {
            if lower_stem.contains(pattern) {
                return Some(ScannedFile {
                    path: path.to_path_buf(),
                    resource_type: rt,
                    stem,
                    size,
                });
            }
        }

        // Small ZIPs in a "roms" parent directory are likely ROMs
        if is_in_directory(path, "roms") || is_in_directory(path, "rom") {
            return Some(ScannedFile {
                path: path.to_path_buf(),
                resource_type: ResourceType::Roms,
                stem,
                size,
            });
        }

        // Archives in an "altsound" directory
        if is_in_directory(path, "altsound") || is_in_directory(path, "alt_sound") {
            return Some(ScannedFile {
                path: path.to_path_buf(),
                resource_type: ResourceType::AltSound,
                stem,
                size,
            });
        }

        // Archives in an "altcolor" directory
        if is_in_directory(path, "altcolor") || is_in_directory(path, "alt_color") {
            return Some(ScannedFile {
                path: path.to_path_buf(),
                resource_type: ResourceType::AltColor,
                stem,
                size,
            });
        }
    }

    // MP3 files are sound
    if extension.eq_ignore_ascii_case("mp3") || extension.eq_ignore_ascii_case("wav") || extension.eq_ignore_ascii_case("ogg") {
        return Some(ScannedFile {
            path: path.to_path_buf(),
            resource_type: ResourceType::Sound,
            stem,
            size,
        });
    }

    None
}

fn is_in_directory(path: &Path, dir_name: &str) -> bool {
    path.ancestors().any(|ancestor| {
        ancestor
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case(dir_name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        // Tables
        fs::write(base.join("Medieval_Madness.vpx"), b"fake vpx").unwrap();
        fs::write(base.join("Hook.vpx"), b"fake vpx").unwrap();
        fs::write(base.join("OldTable.vpt"), b"fake vpt").unwrap();

        // Backglasses
        fs::write(base.join("Medieval_Madness.directb2s"), b"fake b2s").unwrap();

        // POV
        fs::write(base.join("Hook.pov"), b"fake pov").unwrap();

        // ROMs in subdirectory
        let roms_dir = base.join("roms");
        fs::create_dir(&roms_dir).unwrap();
        fs::write(roms_dir.join("mm_109c.zip"), b"fake rom").unwrap();

        // Alt sound
        let altsound_dir = base.join("altsound");
        fs::create_dir(&altsound_dir).unwrap();
        fs::write(altsound_dir.join("hook_altsound.zip"), b"fake altsound").unwrap();

        // Sound files
        fs::write(base.join("theme.mp3"), b"fake mp3").unwrap();

        // Non-pinball files (should be ignored)
        fs::write(base.join("readme.txt"), b"text").unwrap();
        fs::write(base.join("notes.doc"), b"doc").unwrap();

        dir
    }

    #[test]
    fn scan_finds_all_types() {
        let dir = setup_test_dir();
        let results = scan_directory(dir.path());

        assert!(results.errors.is_empty());

        let summary = results.summary();
        let by_type: HashMap<_, _> = summary.into_iter().collect();

        assert_eq!(by_type[&ResourceType::Tables], 3); // 2 vpx + 1 vpt
        assert_eq!(by_type[&ResourceType::Backglasses], 1);
        assert_eq!(by_type[&ResourceType::Pov], 1);
        assert_eq!(by_type[&ResourceType::Roms], 1);
        assert_eq!(by_type[&ResourceType::AltSound], 1);
        assert_eq!(by_type[&ResourceType::Sound], 1);
    }

    #[test]
    fn scan_ignores_unknown_files() {
        let dir = setup_test_dir();
        let results = scan_directory(dir.path());

        let has_txt = results.files.iter().any(|f| {
            f.path.extension().is_some_and(|e| e == "txt")
        });
        assert!(!has_txt);
    }

    #[test]
    fn scan_extracts_stems() {
        let dir = setup_test_dir();
        let results = scan_directory(dir.path());

        let vpx_files: Vec<_> = results
            .files
            .iter()
            .filter(|f| f.resource_type == ResourceType::Tables)
            .collect();

        let stems: Vec<&str> = vpx_files.iter().map(|f| f.stem.as_str()).collect();
        assert!(stems.contains(&"Medieval_Madness"));
        assert!(stems.contains(&"Hook"));
    }

    #[test]
    fn scan_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let results = scan_directory(dir.path());
        assert!(results.files.is_empty());
        assert!(results.errors.is_empty());
    }

    #[test]
    fn scan_nonexistent_directory() {
        let results = scan_directory(Path::new("/nonexistent/path/12345"));
        assert!(results.files.is_empty());
        assert!(!results.errors.is_empty());
    }

    #[test]
    fn classify_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Table.VPX"), b"fake").unwrap();
        fs::write(dir.path().join("Glass.DirectB2S"), b"fake").unwrap();

        let results = scan_directory(dir.path());
        assert_eq!(results.files.len(), 2);
    }
}
