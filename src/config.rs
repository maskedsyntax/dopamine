use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use std::fs;

#[derive(Serialize, Deserialize, Clone)]
pub struct Theme {
    pub fg: (u8, u8, u8),
    pub bg: (u8, u8, u8),
    pub primary: (u8, u8, u8),
    pub secondary: (u8, u8, u8),
    pub accent: (u8, u8, u8),
    pub inactive: (u8, u8, u8),
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            fg: (205, 214, 244),
            bg: (30, 30, 46),
            primary: (137, 180, 250),
            secondary: (166, 227, 161),
            accent: (203, 166, 247),
            inactive: (88, 91, 112),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub music_dirs: Vec<String>,
    pub theme: Theme,
}

impl Default for Config {
    fn default() -> Self {
        let mut music_dirs = Vec::new();
        if let Some(audio_dir) = dirs::audio_dir() {
            music_dirs.push(audio_dir.to_string_lossy().to_string());
        }
        if let Some(home_dir) = dirs::home_dir() {
            let m = home_dir.join("Music");
            if m.exists() {
                music_dirs.push(m.to_string_lossy().to_string());
            }
        }
        
        Self {
            music_dirs,
            theme: Theme::default(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_path = dirs::config_dir()
            .unwrap_or_default()
            .join("dopamine")
            .join("config.toml");
        
        if let Ok(content) = fs::read_to_string(&config_path) {
            if let Ok(config) = toml::from_str(&content) {
                return config;
            }
        }
        
        let default_config = Self::default();
        let _ = fs::create_dir_all(config_path.parent().unwrap());
        let _ = fs::write(&config_path, toml::to_string(&default_config).unwrap());
        default_config
    }
}
