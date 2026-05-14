pub mod desktop_ini;
pub mod ico;
pub mod image_ops;
pub mod winattr;

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use futures::{stream, StreamExt};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

use crate::domain::{ActionPlan, ExecutionReport, PlannedAction, ProgressEvent};
use crate::tmdb::{TmdbClient, IMG_BASE_ORIGINAL};
use crate::util::fs_ext::ensure_dir;

pub async fn execute(
    plan: ActionPlan,
    tmdb: Arc<TmdbClient>,
    concurrency: usize,
    progress: UnboundedSender<ProgressEvent>,
    cancel: CancellationToken,
) -> ExecutionReport {
    // Two phases: first all FS-restructuring actions (renames/moves) sequentially
    // — they must finish before covers land, otherwise we'd write folder.jpg
    // into the old location. Then all RestoreCover in parallel.
    let total = plan.items.len() as u64;
    let report = Arc::new(AsyncMutex::new(ExecutionReport::default()));
    let counter = Arc::new(AsyncMutex::new(0u64));
    let dry_run = plan.dry_run;

    let (structural, covers): (Vec<_>, Vec<_>) = plan.items.into_iter().partition(|a| {
        matches!(
            a,
            PlannedAction::WrapVideoIntoFolder { .. }
                | PlannedAction::WrapDirectoryIntoFolder { .. }
                | PlannedAction::RenameTitleFolder { .. }
                | PlannedAction::AdoptAsSeason { .. }
        )
    });

    let mut failed_structurally: std::collections::HashSet<std::path::PathBuf> =
        std::collections::HashSet::new();

    // Phase 1: structural changes — sequential.
    for action in structural {
        if cancel.is_cancelled() {
            break;
        }
        let label = action.label().to_string();
        let outcome = if dry_run {
            tracing::info!(action = ?action, "dry-run skip");
            Ok(())
        } else {
            apply_action(&action, &tmdb).await
        };
        let current = {
            let mut c = counter.lock().await;
            *c += 1;
            *c
        };
        let _ = progress.send(ProgressEvent {
            current,
            total,
            message: label.clone(),
        });
        let mut r = report.lock().await;
        match outcome {
            Ok(()) => {
                tracing::info!(action = %label, "ok");
                r.successes.push(label);
            }
            Err(e) => {
                tracing::error!(action = %label, error = %format!("{e:#}"), "failed");
                if let Some(dst) = action_destination(&action) {
                    failed_structurally.insert(dst);
                }
                r.failures.push((label, format!("{e:#}")));
            }
        }
    }

    // Phase 2: covers — parallel.
    stream::iter(covers.into_iter())
        .for_each_concurrent(concurrency.max(1), |action| {
            let tmdb = tmdb.clone();
            let progress = progress.clone();
            let report = report.clone();
            let counter = counter.clone();
            let cancel = cancel.clone();
            let failed = failed_structurally.clone();
            async move {
                if cancel.is_cancelled() {
                    return;
                }
                let label = action.label().to_string();
                if let PlannedAction::RestoreCover { dir, .. } = &action {
                    if failed.contains(dir) {
                        let mut r = report.lock().await;
                        r.skipped
                            .push(format!("{label} (target folder was not created)"));
                        return;
                    }
                }
                let outcome = if dry_run {
                    tracing::info!(action = ?action, "dry-run skip");
                    Ok(())
                } else {
                    apply_action(&action, &tmdb).await
                };
                let current = {
                    let mut c = counter.lock().await;
                    *c += 1;
                    *c
                };
                let _ = progress.send(ProgressEvent {
                    current,
                    total,
                    message: label.clone(),
                });
                let mut r = report.lock().await;
                match outcome {
                    Ok(()) => {
                        tracing::info!(action = %label, "ok");
                        r.successes.push(label);
                    }
                    Err(e) => {
                        tracing::error!(action = %label, error = %format!("{e:#}"), "failed");
                        r.failures.push((label, format!("{e:#}")));
                    }
                }
            }
        })
        .await;

    Arc::try_unwrap(report)
        .map(|m| m.into_inner())
        .unwrap_or_default()
}

fn action_destination(a: &PlannedAction) -> Option<std::path::PathBuf> {
    match a {
        PlannedAction::WrapVideoIntoFolder { new_dir, .. } => Some(new_dir.clone()),
        PlannedAction::WrapDirectoryIntoFolder {
            new_parent,
            rename_to,
            src_dir,
            ..
        } => {
            let name = rename_to.clone().or_else(|| {
                src_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
            })?;
            Some(new_parent.join(name))
        }
        PlannedAction::RenameTitleFolder { dst, .. } => Some(dst.clone()),
        PlannedAction::AdoptAsSeason {
            series_root,
            season,
            ..
        } => Some(series_root.join(format!("Season {season}"))),
        PlannedAction::RestoreCover { dir, .. } => Some(dir.clone()),
    }
}

async fn apply_action(action: &PlannedAction, tmdb: &Arc<TmdbClient>) -> Result<()> {
    match action {
        PlannedAction::RestoreCover {
            dir,
            jpg_backup,
            ico_path,
            poster_url,
            ..
        } => restore_cover(tmdb, dir, jpg_backup, ico_path, poster_url).await,
        PlannedAction::WrapVideoIntoFolder {
            video,
            new_dir,
            sidecars,
            ..
        } => wrap_video_into_folder(video, new_dir, sidecars),
        PlannedAction::WrapDirectoryIntoFolder {
            src_dir,
            new_parent,
            rename_to,
            ..
        } => wrap_directory_into_folder(src_dir, new_parent, rename_to.as_deref()),
        PlannedAction::RenameTitleFolder { src, dst, .. } => rename_title_folder(src, dst),
        PlannedAction::AdoptAsSeason {
            src_dir,
            series_root,
            season,
            ..
        } => adopt_as_season(src_dir, series_root, *season),
    }
}

async fn restore_cover(
    tmdb: &Arc<TmdbClient>,
    dir: &Path,
    jpg_backup: &Path,
    ico_path: &Path,
    poster_url: &str,
) -> Result<()> {
    if !dir.is_dir() {
        ensure_dir(dir)?;
    }
    if let Some(parent) = ico_path.parent() {
        ensure_dir(parent)?;
    }
    if let Some(parent) = jpg_backup.parent() {
        ensure_dir(parent)?;
    }

    // Download once.
    let bytes: Bytes = tmdb
        .fetch_poster_bytes(IMG_BASE_ORIGINAL, poster_url)
        .await
        .with_context(|| format!("download poster {poster_url}"))?;

    let img = image_ops::decode(&bytes).context("decode poster")?;
    let resized_512 = image_ops::resize_to_width(&img, 512);

    // Write folder.jpg + _covers backup
    let folder_jpg = dir.join("folder.jpg");
    image_ops::write_jpeg(&resized_512, &folder_jpg, 90)
        .with_context(|| format!("write {}", folder_jpg.display()))?;
    image_ops::write_jpeg(&resized_512, jpg_backup, 90)
        .with_context(|| format!("write {}", jpg_backup.display()))?;

    // Build and write ICO
    let ico_bytes = ico::build_multi_size_ico(&resized_512)?;
    std::fs::write(ico_path, &ico_bytes)
        .with_context(|| format!("write {}", ico_path.display()))?;

    // Write desktop.ini
    let ini_path = dir.join("desktop.ini");
    desktop_ini::write_desktop_ini(&ini_path, ico_path)
        .with_context(|| format!("write {}", ini_path.display()))?;

    // Set attributes
    winattr::set_hidden_system(&ini_path).ok();
    winattr::set_system_on_dir(dir).ok();

    Ok(())
}

fn wrap_video_into_folder(
    video: &Path,
    new_dir: &Path,
    sidecars: &[std::path::PathBuf],
) -> Result<()> {
    if !is_writable(video) {
        anyhow::bail!(
            "source file is locked or in use (active download / open in a player): {}",
            video.display()
        );
    }

    let we_created_dir = !new_dir.exists();
    if new_dir.exists() {
        // Only safe to merge into an empty / cover-only folder. If a video is
        // already there we don't know which is the "real" copy, so we bail and
        // ask the user to resolve manually.
        if dir_contains_any_video(new_dir) {
            anyhow::bail!(
                "target directory already contains another video — resolve manually: {}",
                new_dir.display()
            );
        }
    } else {
        ensure_dir(new_dir)?;
    }

    let dst_video = new_dir.join(video.file_name().context("video has no file_name")?);
    if dst_video.exists() {
        if we_created_dir {
            std::fs::remove_dir(new_dir).ok();
        }
        anyhow::bail!("destination file already exists: {}", dst_video.display());
    }

    if let Err(e) = move_path(video, &dst_video) {
        if we_created_dir {
            std::fs::remove_dir(new_dir).ok();
        }
        return Err(e);
    }

    for s in sidecars {
        if let Some(name) = s.file_name() {
            let dst = new_dir.join(name);
            move_path(s, &dst).ok();
        }
    }
    Ok(())
}

/// True if we can open the file for read+write — same level of access
/// MoveFileExW needs. Catches "file still being downloaded" and "open in
/// player" cases up front so we report a clear message instead of a cryptic
/// Win32 error.
fn is_writable(p: &Path) -> bool {
    use std::fs::OpenOptions;
    OpenOptions::new().read(true).write(true).open(p).is_ok()
}

fn adopt_as_season(src_dir: &Path, series_root: &Path, season: u8) -> Result<()> {
    if !series_root.is_dir() {
        anyhow::bail!(
            "target series folder does not exist: {}",
            series_root.display()
        );
    }
    let target = series_root.join(format!("Season {season}"));
    if target.exists() {
        anyhow::bail!(
            "Season {season} already exists in {} — resolve manually",
            series_root.display()
        );
    }
    move_path(src_dir, &target)?;
    Ok(())
}

fn rename_title_folder(src: &Path, dst: &Path) -> Result<()> {
    if src == dst {
        return Ok(());
    }
    if dst.exists() {
        anyhow::bail!(
            "rename target already exists: {} (source: {})",
            dst.display(),
            src.display()
        );
    }
    move_path(src, dst)?;
    Ok(())
}

fn dir_contains_any_video(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let p = e.path();
        p.is_file() && crate::scanner::classify::is_video_file(&p)
    })
}

fn wrap_directory_into_folder(
    src: &Path,
    new_parent: &Path,
    rename_to: Option<&str>,
) -> Result<()> {
    let we_created_parent = !new_parent.exists();
    if new_parent.exists() {
        if dir_contains_any_video(new_parent) {
            anyhow::bail!(
                "target parent directory already contains a video — resolve manually: {}",
                new_parent.display()
            );
        }
    } else {
        ensure_dir(new_parent)?;
    }
    let dst_name = rename_to
        .map(|s| s.to_string())
        .or_else(|| {
            src.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .context("could not determine destination name")?;
    let dst = new_parent.join(&dst_name);
    if dst.exists() {
        if we_created_parent {
            std::fs::remove_dir(new_parent).ok();
        }
        anyhow::bail!("destination already exists: {}", dst.display());
    }
    if let Err(e) = move_path(src, &dst) {
        if we_created_parent {
            std::fs::remove_dir(new_parent).ok();
        }
        return Err(e);
    }
    Ok(())
}

#[cfg(windows)]
fn move_path(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        anyhow::bail!("destination exists: {}", dst.display());
    }

    // First try the high-level API. It picks the right flags for in-volume
    // renames (where MOVEFILE_WRITE_THROUGH actually breaks moves of
    // directories on some setups). Only fall back to MoveFileExW if it
    // failed for a non-trivial reason.
    match std::fs::rename(src, dst) {
        Ok(()) => return Ok(()),
        Err(e) => {
            tracing::debug!(?e, src=%src.display(), dst=%dst.display(), "std::fs::rename failed, retrying with MoveFileExW(COPY_ALLOWED)");
        }
    }

    use widestring::U16CString;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::GetLastError;
    use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_COPY_ALLOWED};

    let s = U16CString::from_os_str(src.as_os_str())
        .with_context(|| format!("encode src path: {}", src.display()))?;
    let d = U16CString::from_os_str(dst.as_os_str())
        .with_context(|| format!("encode dst path: {}", dst.display()))?;

    unsafe {
        if let Err(e) = MoveFileExW(
            PCWSTR(s.as_ptr()),
            PCWSTR(d.as_ptr()),
            MOVEFILE_COPY_ALLOWED,
        ) {
            let last = GetLastError().0;
            anyhow::bail!(
                "rename failed: {} → {} (Win32 error {} / {:?})",
                src.display(),
                dst.display(),
                last,
                e
            );
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn move_path(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        anyhow::bail!("destination exists: {}", dst.display());
    }
    std::fs::rename(src, dst)
        .with_context(|| format!("rename {} -> {}", src.display(), dst.display()))?;
    Ok(())
}
