use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ApiFormat {
    #[default]
    Openai,
    #[serde(alias = "claude")]
    Anthropic,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct LlmSettings {
    #[serde(alias = "provider")]
    pub format: ApiFormat,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub proxy: String,
    pub timeout_secs: u64,
    pub max_tokens: u32,
    pub prompt_file: PathBuf,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            format: ApiFormat::Openai,
            base_url: String::new(),
            model: String::new(),
            api_key: String::new(),
            proxy: String::new(),
            timeout_secs: 60,
            max_tokens: 4096,
            prompt_file: PathBuf::new(),
        }
    }
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AssistantSettings {
    pub name: String,
    pub wake_words: Vec<String>,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            name: "LingoSoul".into(),
            wake_words: vec!["你好灵言".into(), "Hey Lingo Soul".into()],
        }
    }
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AgentSettings {
    pub enabled: bool,
    pub workspace: PathBuf,
    pub read_directories: Vec<PathBuf>,
    pub allow_read: bool,
    pub allow_write: bool,
    pub allow_execute: bool,
    pub command_timeout_secs: u64,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            workspace: PathBuf::new(),
            read_directories: Vec::new(),
            allow_read: true,
            allow_write: false,
            allow_execute: false,
            command_timeout_secs: 60,
        }
    }
}

#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub assistant: AssistantSettings,
    pub llm: LlmSettings,
    pub local_agent: AgentSettings,
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str(&text).map_err(|error: toml::de::Error| error.message().to_owned())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = toml::to_string_pretty(self).map_err(|error| error.to_string())?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        temporary
            .write_all(text.as_bytes())
            .map_err(|error| error.to_string())?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        temporary
            .persist(path)
            .map(|_| ())
            .map_err(|error| error.error.to_string())
    }
}
