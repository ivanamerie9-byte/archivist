use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

/// Write a Windows `desktop.ini` in UTF-16 LE with BOM, CRLF line endings,
/// pointing at `ico_path`. The file's content matches the existing handcrafted
/// libraries on disk byte-for-byte:
///
/// ```ini
/// [.ShellClassInfo]
/// IconResource=<ico>,0
/// [ViewState]
/// Mode=
/// Vid=
/// FolderType=Generic
/// ```
pub fn write_desktop_ini(dst: &Path, ico_path: &Path) -> Result<()> {
    if dst.exists() {
        crate::pipeline::winattr::clear_hidden_system(dst).ok();
    }
    let body = format!(
        "[.ShellClassInfo]\r\nIconResource={},0\r\n[ViewState]\r\nMode=\r\nVid=\r\nFolderType=Generic\r\n",
        ico_path.display()
    );
    // encoding_rs::UTF_16LE.encode() falls back to UTF-8 (WHATWG quirk), so do
    // the UTF-16 LE encoding manually.
    let mut bytes = Vec::with_capacity(2 + body.len() * 2);
    bytes.extend_from_slice(&[0xFF, 0xFE]);
    for unit in body.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let mut file =
        std::fs::File::create(dst).with_context(|| format!("create {}", dst.display()))?;
    file.write_all(&bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn bom_and_crlf() {
        let dir = tempdir().unwrap();
        let dst = dir.path().join("desktop.ini");
        let ico = PathBuf::from(r"V:\library\Cinema Theatre\_covers\Foo.ico");
        write_desktop_ini(&dst, &ico).unwrap();

        let bytes = std::fs::read(&dst).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);

        let (decoded, _, had_errors) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        assert!(!had_errors);
        assert!(decoded.contains("\r\n"));
        assert!(decoded.contains("[.ShellClassInfo]"));
        assert!(decoded.contains("IconResource=V:\\library"));
        assert!(decoded.ends_with("FolderType=Generic\r\n"));
    }
}
