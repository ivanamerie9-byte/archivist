use std::path::Path;

#[cfg(windows)]
use anyhow::{anyhow, Context, Result};

#[cfg(windows)]
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
#[cfg(windows)]
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
#[cfg(windows)]
const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
#[cfg(windows)]
const INVALID_FILE_ATTRIBUTES: u32 = u32::MAX;

#[cfg(windows)]
fn to_pcwstr(path: &Path) -> Result<widestring::U16CString> {
    widestring::U16CString::from_os_str(path.as_os_str())
        .with_context(|| format!("encode path: {}", path.display()))
}

#[cfg(windows)]
fn get_attrs(path: &Path) -> Result<u32> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetFileAttributesW;

    let s = to_pcwstr(path)?;
    let attrs = unsafe { GetFileAttributesW(PCWSTR(s.as_ptr())) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return Err(anyhow!("GetFileAttributesW failed for {}", path.display()));
    }
    Ok(attrs)
}

#[cfg(windows)]
fn set_attrs(path: &Path, attrs: u32) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_FLAGS_AND_ATTRIBUTES};

    let s = to_pcwstr(path)?;
    unsafe {
        SetFileAttributesW(PCWSTR(s.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(attrs))
            .with_context(|| format!("SetFileAttributesW {}", path.display()))?;
    }
    Ok(())
}

#[cfg(windows)]
pub fn set_hidden_system(path: &Path) -> Result<()> {
    let cur = get_attrs(path).unwrap_or(0);
    let new = (cur | FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) & !FILE_ATTRIBUTE_READONLY;
    set_attrs(path, new)
}

#[cfg(windows)]
pub fn clear_hidden_system(path: &Path) -> Result<()> {
    let cur = get_attrs(path).unwrap_or(0);
    let new = cur & !(FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM | FILE_ATTRIBUTE_READONLY);
    set_attrs(path, new)
}

#[cfg(windows)]
pub fn set_system_on_dir(path: &Path) -> Result<()> {
    let cur = get_attrs(path).unwrap_or(0);
    let new = cur | FILE_ATTRIBUTE_SYSTEM;
    set_attrs(path, new)
}

#[cfg(not(windows))]
pub fn set_hidden_system(_p: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn clear_hidden_system(_p: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn set_system_on_dir(_p: &Path) -> anyhow::Result<()> {
    Ok(())
}
