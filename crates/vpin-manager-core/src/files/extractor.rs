use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Supported archive formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
    Rar,
}

impl ArchiveFormat {
    /// Detect format from file extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        match ext.to_lowercase().as_str() {
            "zip" => Some(ArchiveFormat::Zip),
            "7z" => Some(ArchiveFormat::SevenZ),
            "rar" => Some(ArchiveFormat::Rar),
            _ => None,
        }
    }
}

impl std::fmt::Display for ArchiveFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchiveFormat::Zip => write!(f, "ZIP"),
            ArchiveFormat::SevenZ => write!(f, "7z"),
            ArchiveFormat::Rar => write!(f, "RAR"),
        }
    }
}

/// Result of extracting an archive.
#[derive(Debug)]
pub struct ExtractResult {
    pub format: ArchiveFormat,
    pub dest_dir: PathBuf,
    pub file_count: usize,
}

/// Errors during extraction.
#[derive(Debug)]
pub enum ExtractError {
    Io(io::Error),
    Zip(zip::result::ZipError),
    SevenZ(String),
    Rar(String),
    UnknownFormat(PathBuf),
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractError::Io(e) => write!(f, "I/O error: {e}"),
            ExtractError::Zip(e) => write!(f, "ZIP error: {e}"),
            ExtractError::SevenZ(e) => write!(f, "7z error: {e}"),
            ExtractError::Rar(e) => write!(f, "RAR error: {e}"),
            ExtractError::UnknownFormat(p) => {
                write!(f, "unknown archive format: {}", p.display())
            }
        }
    }
}

impl std::error::Error for ExtractError {}

impl From<io::Error> for ExtractError {
    fn from(e: io::Error) -> Self {
        ExtractError::Io(e)
    }
}

impl From<zip::result::ZipError> for ExtractError {
    fn from(e: zip::result::ZipError) -> Self {
        ExtractError::Zip(e)
    }
}

/// Extract an archive to a destination directory.
/// Auto-detects format from extension.
pub fn extract(archive_path: &Path, dest_dir: &Path) -> Result<ExtractResult, ExtractError> {
    let format = ArchiveFormat::from_path(archive_path)
        .ok_or_else(|| ExtractError::UnknownFormat(archive_path.to_path_buf()))?;

    extract_as(archive_path, dest_dir, format)
}

/// Extract an archive with an explicit format.
pub fn extract_as(
    archive_path: &Path,
    dest_dir: &Path,
    format: ArchiveFormat,
) -> Result<ExtractResult, ExtractError> {
    fs::create_dir_all(dest_dir)?;

    match format {
        ArchiveFormat::Zip => extract_zip(archive_path, dest_dir),
        ArchiveFormat::SevenZ => extract_7z(archive_path, dest_dir),
        ArchiveFormat::Rar => extract_rar(archive_path, dest_dir),
    }
}

/// Extract a ZIP archive.
fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<ExtractResult, ExtractError> {
    let file = fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut file_count = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(enclosed_name) = entry.enclosed_name() else {
            continue; // Skip entries with unsafe paths
        };

        let dest_path = dest_dir.join(enclosed_name);

        if entry.is_dir() {
            fs::create_dir_all(&dest_path)?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&dest_path)?;
            io::copy(&mut entry, &mut outfile)?;
            file_count += 1;
        }
    }

    Ok(ExtractResult {
        format: ArchiveFormat::Zip,
        dest_dir: dest_dir.to_path_buf(),
        file_count,
    })
}

/// Extract a 7z archive.
fn extract_7z(archive_path: &Path, dest_dir: &Path) -> Result<ExtractResult, ExtractError> {
    sevenz_rust2::decompress_file(archive_path, dest_dir)
        .map_err(|e| ExtractError::SevenZ(e.to_string()))?;

    // Count extracted files
    let file_count = count_files_recursive(dest_dir);

    Ok(ExtractResult {
        format: ArchiveFormat::SevenZ,
        dest_dir: dest_dir.to_path_buf(),
        file_count,
    })
}

/// Extract a RAR archive.
fn extract_rar(archive_path: &Path, dest_dir: &Path) -> Result<ExtractResult, ExtractError> {
    let archive = unrar::Archive::new(archive_path)
        .open_for_processing()
        .map_err(|e| ExtractError::Rar(e.to_string()))?;

    let mut file_count = 0;
    let mut cursor = archive;

    loop {
        match cursor.read_header() {
            Ok(Some(header)) => {
                cursor = header
                    .extract_with_base(dest_dir)
                    .map_err(|e| ExtractError::Rar(e.to_string()))?;
                file_count += 1;
            }
            Ok(None) => break,
            Err(e) => return Err(ExtractError::Rar(e.to_string())),
        }
    }

    Ok(ExtractResult {
        format: ArchiveFormat::Rar,
        dest_dir: dest_dir.to_path_buf(),
        file_count,
    })
}

/// Extract an archive to a temporary directory.
/// Returns the temp dir handle (directory is deleted when dropped) and the result.
pub fn extract_to_temp(
    archive_path: &Path,
) -> Result<(tempfile::TempDir, ExtractResult), ExtractError> {
    let temp_dir = tempfile::tempdir().map_err(ExtractError::Io)?;
    let result = extract(archive_path, temp_dir.path())?;
    Ok((temp_dir, result))
}

fn count_files_recursive(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_files_recursive(&path);
            } else {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_format_from_extension() {
        assert_eq!(
            ArchiveFormat::from_path(Path::new("file.zip")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::from_path(Path::new("file.7z")),
            Some(ArchiveFormat::SevenZ)
        );
        assert_eq!(
            ArchiveFormat::from_path(Path::new("file.rar")),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(
            ArchiveFormat::from_path(Path::new("file.ZIP")),
            Some(ArchiveFormat::Zip)
        );
        assert!(ArchiveFormat::from_path(Path::new("file.txt")).is_none());
        assert!(ArchiveFormat::from_path(Path::new("noext")).is_none());
    }

    #[test]
    fn extract_zip_archive() {
        // Create a test ZIP in memory
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("test.zip");

        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();

            writer.start_file("hello.txt", options).unwrap();
            io::Write::write_all(&mut writer, b"Hello, world!").unwrap();

            writer.start_file("subdir/nested.txt", options).unwrap();
            io::Write::write_all(&mut writer, b"Nested file").unwrap();

            writer.finish().unwrap();
        }

        let dest = dir.path().join("extracted");
        let result = extract(&zip_path, &dest).unwrap();

        assert_eq!(result.format, ArchiveFormat::Zip);
        assert_eq!(result.file_count, 2);
        assert!(dest.join("hello.txt").exists());
        assert!(dest.join("subdir/nested.txt").exists());

        assert_eq!(
            fs::read_to_string(dest.join("hello.txt")).unwrap(),
            "Hello, world!"
        );
    }

    #[test]
    fn extract_to_temp_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("test.zip");

        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("file.txt", options).unwrap();
            io::Write::write_all(&mut writer, b"data").unwrap();
            writer.finish().unwrap();
        }

        let temp_path;
        {
            let (temp_dir, result) = extract_to_temp(&zip_path).unwrap();
            temp_path = temp_dir.path().to_path_buf();
            assert_eq!(result.file_count, 1);
            assert!(temp_path.exists());
            // temp_dir drops here
        }
        assert!(!temp_path.exists());
    }

    #[test]
    fn unknown_format_errors() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("file.txt");
        fs::write(&fake, b"not an archive").unwrap();

        let result = extract(&fake, dir.path());
        assert!(matches!(result, Err(ExtractError::UnknownFormat(_))));
    }
}
