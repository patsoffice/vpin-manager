use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::b2s::B2sMetadata;
use crate::config::ResourceType;
use crate::vpx::VpxMetadata;

/// A file discovered during a directory scan.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub resource_type: ResourceType,
    /// Filename without extension, used for matching against game names.
    pub stem: String,
    /// File size in bytes.
    pub size: u64,
    /// Metadata extracted from VPX files (table name, author, ROM, etc.).
    pub vpx_metadata: Option<VpxMetadata>,
    /// Metadata extracted from .directb2s backglass files.
    pub b2s_metadata: Option<B2sMetadata>,
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
    let file_name = path.file_name()?.to_str()?;
    let stem = path.file_stem()?.to_str()?.to_string();
    let size = path.metadata().map(|m| m.len()).unwrap_or(0);

    let (resource_type, final_stem) = classify_type(path, extension, file_name, &stem)?;

    // Read VPX metadata for .vpx files
    let vpx_metadata = if extension.eq_ignore_ascii_case("vpx") {
        crate::vpx::read_vpx_metadata(path).ok()
    } else {
        None
    };

    // Read B2S metadata for .directb2s files
    let b2s_metadata = if extension.eq_ignore_ascii_case("directb2s") {
        crate::b2s::read_b2s_metadata(path).ok()
    } else {
        None
    };

    Some(ScannedFile {
        path: path.to_path_buf(),
        resource_type,
        stem: final_stem,
        size,
        vpx_metadata,
        b2s_metadata,
    })
}

/// Determine the resource type and effective stem for a file.
/// Returns None if the file is not a recognized virtual pinball resource.
fn classify_type(
    path: &Path,
    extension: &str,
    file_name: &str,
    stem: &str,
) -> Option<(ResourceType, String)> {
    // Check extension map first
    if let Some(&(_, rt)) = EXTENSION_MAP.iter().find(|(ext, _)| ext.eq_ignore_ascii_case(extension)) {
        return Some((rt, stem.to_string()));
    }

    // Check if ZIP/RAR/7z
    if matches!(extension.to_lowercase().as_str(), "zip" | "rar" | "7z") {
        let lower_name = file_name.to_lowercase();
        let lower_stem = stem.to_lowercase();

        // Check for double extensions like .vpx.zip, .directb2s.zip
        if let Some(inner_ext) = Path::new(stem).extension().and_then(|e| e.to_str()) {
            if let Some(&(_, rt)) = EXTENSION_MAP.iter().find(|(ext, _)| ext.eq_ignore_ascii_case(inner_ext)) {
                let inner_stem = Path::new(stem)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(stem)
                    .to_string();
                return Some((rt, inner_stem));
            }
        }

        // Check name patterns
        for &(pattern, rt) in NAME_PATTERNS {
            if lower_stem.contains(pattern) {
                return Some((rt, stem.to_string()));
            }
        }

        // Archives in a "Tables" directory are likely tables
        if is_in_directory(path, "tables") {
            return Some((ResourceType::Tables, stem.to_string()));
        }

        // ZIPs in a "roms" parent directory are likely ROMs
        if is_in_directory(path, "roms") || is_in_directory(path, "rom") {
            return Some((ResourceType::Roms, stem.to_string()));
        }

        // Archives in an "altsound" directory
        if is_in_directory(path, "altsound") || is_in_directory(path, "alt_sound") {
            return Some((ResourceType::AltSound, stem.to_string()));
        }

        // Archives in an "altcolor" directory
        if is_in_directory(path, "altcolor") || is_in_directory(path, "alt_color") {
            return Some((ResourceType::AltColor, stem.to_string()));
        }

        // Check for common table-related terms in the name
        if lower_name.contains("vpx") || lower_name.contains("vpt") || lower_name.contains("vpw") {
            return Some((ResourceType::Tables, stem.to_string()));
        }
    }

    // MP3/WAV/OGG files are sound
    if extension.eq_ignore_ascii_case("mp3")
        || extension.eq_ignore_ascii_case("wav")
        || extension.eq_ignore_ascii_case("ogg")
    {
        return Some((ResourceType::Sound, stem.to_string()));
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
    fn scan_vpx_zip_double_extension() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Fish Tales (Williams 1992) VPW 1.1.vpx.zip"),
            b"fake",
        )
        .unwrap();
        fs::write(
            dir.path().join("Hook.directb2s.zip"),
            b"fake",
        )
        .unwrap();

        let results = scan_directory(dir.path());
        assert_eq!(results.files.len(), 2);

        let table = results.files.iter().find(|f| f.resource_type == ResourceType::Tables).unwrap();
        assert_eq!(table.stem, "Fish Tales (Williams 1992) VPW 1.1");

        let b2s = results.files.iter().find(|f| f.resource_type == ResourceType::Backglasses).unwrap();
        assert_eq!(b2s.stem, "Hook");
    }

    #[test]
    fn scan_archives_in_tables_dir() {
        let dir = tempfile::tempdir().unwrap();
        let tables_dir = dir.path().join("Tables");
        fs::create_dir(&tables_dir).unwrap();
        fs::write(tables_dir.join("Jurassic Park 30th.zip"), b"fake").unwrap();

        let results = scan_directory(dir.path());
        assert_eq!(results.files.len(), 1);
        assert_eq!(results.files[0].resource_type, ResourceType::Tables);
    }

    #[test]
    fn scan_vpw_in_name() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Skateball (Bally 1980) VPW 1.0.vpx.zip"),
            b"fake",
        )
        .unwrap();

        let results = scan_directory(dir.path());
        assert_eq!(results.files.len(), 1);
        assert_eq!(results.files[0].resource_type, ResourceType::Tables);
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
