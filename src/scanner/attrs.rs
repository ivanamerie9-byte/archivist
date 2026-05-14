use std::io;
use std::path::{Path, PathBuf};

#[cfg(windows)]
pub fn has_system_attr(p: &Path) -> io::Result<bool> {
    use std::os::windows::fs::MetadataExt;

    let meta = std::fs::metadata(p)?;
    // FILE_ATTRIBUTE_SYSTEM = 0x4
    Ok(meta.file_attributes() & 0x4 != 0)
}

#[cfg(not(windows))]
pub fn has_system_attr(_p: &Path) -> io::Result<bool> {
    Ok(true)
}

/// Read `desktop.ini` and extract the path it points at via `IconResource=`.
/// Returns `Some(path)` when the line is present (the path may not actually
/// exist on disk — caller decides). Returns `None` if the ini file is missing
/// or doesn't contain an IconResource line.
pub fn read_icon_resource(desktop_ini: &Path) -> Option<PathBuf> {
    let bytes = std::fs::read(desktop_ini).ok()?;
    // Strip BOM if present, decode as UTF-16 LE.
    let body = if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        // Fallback: assume ANSI/UTF-8 plain text.
        String::from_utf8_lossy(&bytes).into_owned()
    };
    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("IconResource=") {
            // Format: "C:\path\to\icon.ico,0" — strip the trailing ",N"
            let path_str = match rest.rfind(',') {
                Some(idx) => &rest[..idx],
                None => rest,
            };
            return Some(PathBuf::from(path_str.trim()));
        }
    }
    None
}
