use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppearanceMode {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Comfortable,
    Spacious,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemAppearance {
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedAppearance {
    Dark,
    Light,
}

impl AppearanceMode {
    pub fn resolve(self, system: SystemAppearance) -> ResolvedAppearance {
        match (self, system) {
            (Self::Dark, _) | (Self::System, SystemAppearance::Dark) => ResolvedAppearance::Dark,
            (Self::Light, _) | (Self::System, SystemAppearance::Light) => ResolvedAppearance::Light,
        }
    }
}

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
#[serde(default)]
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
#[serde(default)]
pub struct Config {
    pub music_dirs: Vec<String>,
    pub theme_name: String,
    pub custom_theme: Option<Theme>,
    pub lastfm: LastFmConfig,
    pub appearance: AppearanceMode,
    pub dark_theme: String,
    pub light_theme: String,
    pub density: Density,
    pub reduce_motion: bool,
    pub visualizer_enabled: bool,
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
            appearance: AppearanceMode::System,
            dark_theme: "Cursor Dark".to_string(),
            light_theme: "Cursor Light".to_string(),
            density: Density::Comfortable,
            reduce_motion: false,
            visualizer_enabled: true,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_default()
            .join("dopamine")
            .join("config.toml")
    }

    pub fn load() -> Self {
        let config_path = Self::path();

        if let Ok(content) = fs::read_to_string(&config_path)
            && let Ok(config) = toml::from_str(&content)
        {
            return config;
        }

        let default_config = Self::default();
        let _ = default_config.save_to(&config_path);
        default_config
    }

    /// Atomically replaces the application config after fully writing it beside the destination.
    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&Self::path())
    }

    fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("config path has no parent: {}", path.display()))?;
        fs::create_dir_all(parent)?;

        let serialized = toml::to_string_pretty(self)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config.toml");
        let temporary_path =
            parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));

        let result = (|| -> anyhow::Result<()> {
            let mut temporary = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)?;
            temporary.write_all(serialized.as_bytes())?;
            temporary.sync_all()?;
            drop(temporary);
            fs::rename(&temporary_path, path)?;
            Ok(())
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_receives_appearance_defaults() {
        let config: Config = toml::from_str(
            r#"
music_dirs = ["/music"]
theme_name = "nord"

[lastfm]
api_key = ""
api_secret = ""
session_key = ""
enabled = false
"#,
        )
        .unwrap();

        assert_eq!(config.appearance, AppearanceMode::System);
        assert_eq!(config.dark_theme, "Cursor Dark");
        assert_eq!(config.light_theme, "Cursor Light");
        assert_eq!(config.density, Density::Comfortable);
        assert!(!config.reduce_motion);
        assert!(config.visualizer_enabled);
    }

    #[test]
    fn explicit_appearance_overrides_system() {
        assert_eq!(
            AppearanceMode::Dark.resolve(SystemAppearance::Light),
            ResolvedAppearance::Dark
        );
        assert_eq!(
            AppearanceMode::Light.resolve(SystemAppearance::Dark),
            ResolvedAppearance::Light
        );
    }

    #[test]
    fn save_atomically_replaces_existing_config() {
        let directory = std::env::temp_dir().join(format!(
            "dopamine-config-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = directory.join("config.toml");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, "invalid = true").unwrap();

        let config = Config {
            appearance: AppearanceMode::Dark,
            ..Config::default()
        };
        config.save_to(&path).unwrap();

        let saved: Config = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved.appearance, AppearanceMode::Dark);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }
}
