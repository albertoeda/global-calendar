//! `team.toml`: who is on the team and where they are.

use crate::cities::CityDb;
use chrono_tz::Tz;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub name: String,
    /// City as the user wrote it, without any ", country" qualifier.
    pub city: String,
    pub lat: f64,
    pub lon: f64,
    pub tz: Tz,
}

#[derive(Clone, Debug, Default)]
pub struct TeamConfig {
    /// The file this was loaded from, if any.
    pub path: Option<PathBuf>,
    pub theme: Option<String>,
    pub people: Vec<Person>,
    /// Human-readable problems. People that did resolve are still in `people`.
    pub errors: Vec<String>,
}

/// Where we look for `team.toml`, in order.
pub fn search_paths(cli: Option<&Path>) -> Vec<PathBuf> {
    let _ = cli;
    todo!()
}

/// Loads the first existing file from [`search_paths`].
pub fn load(cli: Option<&Path>) -> TeamConfig {
    let _ = cli;
    todo!()
}

/// Parses config text. The returned `path` is `None`.
pub fn parse(text: &str, db: &CityDb) -> TeamConfig {
    let _ = (text, db);
    todo!()
}

/// Sets the top-level `theme` key in the file, preserving comments and formatting.
pub fn save_theme(path: &Path, theme_id: &str) -> anyhow::Result<()> {
    let _ = (path, theme_id);
    todo!()
}
