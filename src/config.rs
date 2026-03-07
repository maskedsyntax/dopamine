use serde::{Serialize, Deserialize};
use std::fs;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Theme {
    pub fg: (u8, u8, u8),
    pub bg: (u8, u8, u8),
    pub primary: (u8, u8, u8),
    pub secondary: (u8, u8, u8),
    pub accent: (u8, u8, u8),
    pub inactive: (u8, u8, u8),
}

impl Theme {
    pub fn mocha() -> Self {
        Self {
            fg: (205, 214, 244),
            bg: (30, 30, 46),
            primary: (137, 180, 250),
            secondary: (166, 227, 161),
            accent: (203, 166, 247),
            inactive: (88, 91, 112),
        }
    }

    pub fn dracula() -> Self {
        Self {
            fg: (248, 248, 242),
            bg: (40, 42, 54),
            primary: (139, 233, 253),
            secondary: (80, 250, 123),
            accent: (189, 147, 249),
            inactive: (98, 114, 164),
        }
    }

    pub fn nord() -> Self {
        Self {
            fg: (236, 239, 244),
            bg: (46, 52, 64),
            primary: (136, 192, 208),
            secondary: (163, 190, 140),
            accent: (180, 142, 173),
            inactive: (76, 86, 106),
        }
    }

    pub fn monokai() -> Self {
        Self {
            fg: (248, 248, 242),
            bg: (39, 40, 34),
            primary: (102, 217, 239),
            secondary: (166, 226, 46),
            accent: (174, 129, 255),
            inactive: (117, 113, 94),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::mocha()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LastFmConfig {
    pub api_key: String,
    pub api_secret: String,
    pub session_key: String,
    pub enabled: bool,
}

impl Default for LastFmConfig {
    fn default() -> Self {
        Self {
            api_key: "".to_string(),
            api_secret: "".to_string(),
            session_key: "".to_string(),
            enabled: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub music_dirs: Vec<String>,
    pub theme_name: String,
    pub custom_theme: Option<Theme>,
    pub lastfm: LastFmConfig,
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
            theme_name: "mocha".to_string(),
            custom_theme: None,
            lastfm: LastFmConfig::default(),
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

    pub fn get_theme(&self) -> Theme {
        if let Some(custom) = &self.custom_theme {
            return custom.clone();
        }
        match self.theme_name.to_lowercase().as_str() {
            "dracula" => Theme::dracula(),
            "nord" => Theme::nord(),
            "monokai" => Theme::monokai(),
            _ => Theme::mocha(),
        }
    }
}
