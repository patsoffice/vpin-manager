use std::path::Path;

use regex_lite::Regex;

/// Metadata extracted from a VPX file.
#[derive(Debug, Clone, Default)]
pub struct VpxMetadata {
    /// Table name from TableInfo.
    pub table_name: Option<String>,
    /// Table version from TableInfo.
    pub table_version: Option<String>,
    /// Author name from TableInfo.
    pub author_name: Option<String>,
    /// ROM name extracted from VBScript code.
    pub rom_name: Option<String>,
    /// Whether the table requires PinMAME.
    pub requires_pinmame: bool,
    /// Table description from TableInfo.
    pub table_description: Option<String>,
    /// Table blurb from TableInfo.
    pub table_blurb: Option<String>,
    /// Release date from TableInfo.
    pub release_date: Option<String>,
}

/// Read metadata from a VPX file.
pub fn read_vpx_metadata(path: &Path) -> Result<VpxMetadata, VpxError> {
    let mut vpx_file =
        vpin::vpx::open(path).map_err(|e| VpxError::Io(format!("{}: {e}", path.display())))?;

    let table_info = vpx_file
        .read_tableinfo()
        .map_err(|e| VpxError::Parse(format!("tableinfo: {e}")))?;

    let gamedata = vpx_file
        .read_gamedata()
        .map_err(|e| VpxError::Parse(format!("gamedata: {e}")))?;

    let code = &gamedata.code.string;
    let requires_pinmame = script_requires_pinmame(code);
    let rom_name = if requires_pinmame {
        extract_rom_name(code)
    } else {
        None
    };

    Ok(VpxMetadata {
        table_name: non_empty(table_info.table_name),
        table_version: non_empty(table_info.table_version),
        author_name: non_empty(table_info.author_name),
        rom_name,
        requires_pinmame,
        table_description: non_empty(table_info.table_description),
        table_blurb: non_empty(table_info.table_blurb),
        release_date: non_empty(table_info.release_date),
    })
}

/// Extract the ROM/game name from VBScript code.
/// Looks for patterns like:
///   cGameName = "xyz"
///   .GameName = "xyz"
fn extract_rom_name(code: &str) -> Option<String> {
    let patterns = [
        Regex::new(r#"(?i)cGameName\s*=\s*"([^"]+)""#).unwrap(),
        Regex::new(r#"(?i)\.GameName\s*=\s*"([^"]+)""#).unwrap(),
    ];

    for pattern in &patterns {
        if let Some(caps) = pattern.captures(code)
            && let Some(name) = caps.get(1)
        {
            let val = name.as_str().trim().to_string();
            if !val.is_empty() {
                return Some(val);
            }
        }
    }

    None
}

/// Check if the VBScript code requires PinMAME.
fn script_requires_pinmame(code: &str) -> bool {
    let lower = code.to_lowercase();
    lower.contains("loadvpm") || lower.contains("loadcore") || lower.contains("vpmmapexits")
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.trim().is_empty())
}

#[derive(Debug)]
pub enum VpxError {
    Io(String),
    Parse(String),
}

impl std::fmt::Display for VpxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VpxError::Io(e) => write!(f, "VPX I/O error: {e}"),
            VpxError::Parse(e) => write!(f, "VPX parse error: {e}"),
        }
    }
}

impl std::error::Error for VpxError {}
