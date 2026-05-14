use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::domain::{Library, LibraryKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub tmdb_api_key: String,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_true")]
    pub test_on_startup: bool,
    pub libraries: LibraryPaths,

    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryPaths {
    pub movies: LibraryPath,
    pub series: LibraryPath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryPath {
    pub root: PathBuf,
}

fn default_concurrency() -> usize {
    8
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tmdb_api_key: String::new(),
            concurrency: default_concurrency(),
            test_on_startup: true,
            libraries: LibraryPaths {
                movies: LibraryPath {
                    root: PathBuf::from(r"V:\library\Cinema Theatre"),
                },
                series: LibraryPath {
                    root: PathBuf::from(r"V:\library\Origial Series"),
                },
            },
            source_path: None,
        }
    }
}

impl Config {
    /// Try several conventional locations and return the first hit, or the
    /// in-memory default. `explicit` (from `--config`) wins if provided.
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let candidates = match explicit {
            Some(p) => vec![p.to_path_buf()],
            None => default_candidates(),
        };

        for path in &candidates {
            if path.is_file() {
                let raw = std::fs::read_to_string(path)
                    .with_context(|| format!("read config at {}", path.display()))?;
                let mut cfg: Config = toml::from_str(&raw)
                    .with_context(|| format!("parse config at {}", path.display()))?;
                cfg.source_path = Some(path.clone());
                return Ok(cfg);
            }
        }

        // Default save location is next to the exe.
        let cfg = Config {
            source_path: candidates.into_iter().next(),
            ..Config::default()
        };
        Ok(cfg)
    }

    pub fn save(&self) -> Result<PathBuf> {
        let path = self
            .source_path
            .clone()
            .or_else(|| exe_dir().map(|d| d.join("config.toml")))
            .unwrap_or_else(|| PathBuf::from("config.toml"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let body = toml::to_string_pretty(self).context("serialize config")?;
        std::fs::write(&path, body)
            .with_context(|| format!("write config to {}", path.display()))?;
        Ok(path)
    }

    pub fn libraries(&self) -> [Library; 2] {
        [
            Library::new(LibraryKind::Movies, self.libraries.movies.root.clone()),
            Library::new(LibraryKind::Series, self.libraries.series.root.clone()),
        ]
    }
}

fn default_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    out.push(PathBuf::from("config.toml"));
    if let Some(dir) = exe_dir() {
        out.push(dir.join("config.toml"));
    }
    if let Some(proj) = directories::ProjectDirs::from("dev", "ivanamerie", "archivist") {
        out.push(proj.config_dir().join("config.toml"));
    }
    out
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrip_through_toml() {
        let cfg = Config::default();
        let s = toml::to_string(&cfg).unwrap();
        let parsed: Config = toml::from_str(&s).unwrap();
        assert_eq!(parsed.concurrency, cfg.concurrency);
        assert_eq!(parsed.libraries.movies.root, cfg.libraries.movies.root);
    }
}
