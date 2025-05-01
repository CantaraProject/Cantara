//! This module contains the logic and structures for managing, loading and saving the program's settings.

use std::{fs, path::PathBuf};
use serde::{Deserialize, Serialize};

/// The struct representing Cantara's settings.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct Settings {
    pub song_repos: Vec<Repository>,
    pub wizard_completed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            song_repos: vec![],
            wizard_completed: false,
        }
    }
}

impl Settings {
    /// Load settings from storage or creates a new default settings if
    /// the program is run for the first time.
    pub fn load() -> Self {
        match get_settings_file() {
            Some(file) => match std::fs::read_to_string(file) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(settings) => settings,
                    Err(_) => Self::default(),
                },
                Err(_) => Self::default(),
            },
            None => Self::default(),
        }
    }
    
    /// Save the current settings to storage.
    pub fn save(&self) {
        match get_settings_file() {
            Some(file) => {
                let _ = fs::create_dir_all(get_settings_folder().unwrap());
                match std::fs::write(file, serde_json::to_string_pretty(self).unwrap()) {
                    Ok(_) => (),
                    Err(_) => (),
                }
            }
            None => (),
        }
    }
    
    /// Add a new repository to the settings if the repository is not already present (avoiding duplicates).
    pub fn add_repository(&mut self, repo: Repository) {
        if !self.song_repos.contains(&repo) {
            self.song_repos.push(repo);
        }       
    }
    
    /// Add a new repository folder given as String to the settings if the repository is not already present (avoiding duplicates).
    pub fn add_repository_folder(&mut self, folder: String) {
        self.song_repos.push(Repository::LocaleFilePath(folder));
    }
}

/// The enum representing the different types of repositories.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub enum Repository {
    /// A repository that is a local folder represented by a file path.
    LocaleFilePath(String),
    
    /// A repository that is a remote URL.
    Remote(String),
}

fn get_settings_file() -> Option<PathBuf> {
    match get_settings_folder() {
        Some(settings_folder) => Some(settings_folder.join("settings.json")),
        None => None,
    }
}

fn get_settings_folder() -> Option<PathBuf> {
    match dirs::config_local_dir() {
        Some(dir) => Some(dir.join("cantara")),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn test_load_settings() {
        let settings = get_settings_folder().unwrap();
        dbg!(&settings);
        println!("Settings folder: {:?}", settings);
    }
}
