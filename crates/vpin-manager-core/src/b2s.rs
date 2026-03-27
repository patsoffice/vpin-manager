use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Metadata extracted from a .directb2s backglass file.
#[derive(Debug, Clone, Default)]
pub struct B2sMetadata {
    /// Project name.
    pub name: Option<String>,
    /// Game/ROM name identifier (e.g., "mm_109c").
    pub game_name: Option<String>,
    /// Author of the backglass.
    pub author: Option<String>,
}

/// Read metadata from a .directb2s file.
pub fn read_b2s_metadata(path: &Path) -> Result<B2sMetadata, B2sError> {
    let file = File::open(path).map_err(|e| B2sError::Io(format!("{}: {e}", path.display())))?;
    let reader = BufReader::new(file);

    let data =
        directb2s::read(reader).map_err(|e| B2sError::Parse(format!("{}: {e}", path.display())))?;

    Ok(B2sMetadata {
        name: non_empty(data.name.value),
        game_name: non_empty(data.game_name.value),
        author: non_empty(data.author.value),
    })
}

fn non_empty(s: String) -> Option<String> {
    if s.trim().is_empty() { None } else { Some(s) }
}

#[derive(Debug)]
pub enum B2sError {
    Io(String),
    Parse(String),
}

impl std::fmt::Display for B2sError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            B2sError::Io(e) => write!(f, "B2S I/O error: {e}"),
            B2sError::Parse(e) => write!(f, "B2S parse error: {e}"),
        }
    }
}

impl std::error::Error for B2sError {}
