use std::path::PathBuf;

use winisland_lingosoul::{Paths, Settings};

pub struct LingoSoulConfig {
    pub settings: Settings,
    pub error: Option<String>,
}

pub fn config_path() -> PathBuf {
    std::env::var_os("LINGOSOUL_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|home| home.join(".winisland-lingosoul"))
                .unwrap_or_else(|| crate::platform::shell().config_dir().join("lingosoul"))
                .join("config.toml")
        })
}

pub fn paths() -> Paths {
    let config = config_path();
    let models = std::env::var_os("LINGOSOUL_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let local =
                dirs::data_local_dir().unwrap_or_else(|| crate::platform::shell().data_dir());
            let current = local.join("WinIsland").join("lingosoul").join("models");
            let previous = local.join("LingoSoul").join("models");
            if !current.exists() && previous.is_dir() {
                previous
            } else {
                current
            }
        });
    Paths { config, models }
}

impl LingoSoulConfig {
    pub fn load() -> Self {
        match Settings::load(&config_path()) {
            Ok(settings) => Self {
                settings,
                error: None,
            },
            Err(error) => Self {
                settings: Settings::default(),
                error: Some(error),
            },
        }
    }

    pub fn save(&self) -> Result<(), String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        self.settings.save(&config_path())
    }

    pub fn wake_words(&self) -> String {
        self.settings.assistant.wake_words.join(", ")
    }

    pub fn set_wake_words(&mut self, value: &str) {
        self.settings.assistant.wake_words = value
            .split([',', '，', '、', ';', '；'])
            .map(str::trim)
            .filter(|word| !word.is_empty())
            .map(str::to_owned)
            .collect();
    }

    pub fn prompt_path(&self) -> PathBuf {
        if self.settings.llm.prompt_file.as_os_str().is_empty() {
            paths().prompts().join("system.txt")
        } else {
            self.settings.llm.prompt_file.clone()
        }
    }
}
