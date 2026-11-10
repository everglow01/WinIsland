mod config;
mod conversation;
mod history;
mod models;
mod runtime;
mod speech;
mod tools;
mod voice;
mod wake;

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::oneshot;
use winisland_core::assistant::{AssistantCommand, AssistantSnapshot, ChatMessage, ModelState};

pub use config::{AgentSettings, ApiFormat, AssistantSettings, LlmSettings, Settings};
pub use runtime::Assistant;

#[derive(Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub models: PathBuf,
}

impl Paths {
    pub fn history(&self) -> PathBuf {
        self.config.with_file_name("history.jsonl")
    }

    pub fn prompts(&self) -> PathBuf {
        self.config.with_file_name("prompts")
    }
}

pub enum Event {
    Snapshot(AssistantSnapshot),
    Command(AssistantCommand),
    History(Vec<ChatMessage>),
    VoiceAvailable(bool),
    Models(ModelState),
    Error(String),
    Stopped,
}

pub trait Host: Send + Sync {
    fn emit(&self, event: Event);
    fn request(&self, body: Value) -> oneshot::Receiver<Value>;
    fn language(&self) -> String;
    fn local_time(&self) -> String;
    fn open_app(&self, name: &str) -> Result<String, String>;
    fn open_url(&self, url: &str) -> Result<(), String>;
    fn media_control(&self, action: &str, percent: Option<u64>) -> Result<(), String>;
}

pub type SharedHost = Arc<dyn Host>;

pub fn models_ready(paths: &Paths) -> bool {
    models::ready(&paths.models)
}

pub fn initialize_prompts(paths: &Paths) -> Result<(), String> {
    conversation::initialize_prompts(paths)
}

pub async fn download_models(
    paths: &Paths,
    client: reqwest::Client,
    host: SharedHost,
    cancel: tokio_util::sync::CancellationToken,
    repair: bool,
) -> Result<(), String> {
    models::ensure(paths.models.clone(), client, host, cancel, repair).await
}
