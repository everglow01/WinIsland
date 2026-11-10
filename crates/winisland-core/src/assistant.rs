use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AssistantState {
    #[default]
    Idle,
    Listening,
    Thinking,
    Speaking,
    Replying,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssistantSnapshot {
    pub state: AssistantState,
    pub level: f32,
    pub text: Arc<str>,
    pub listening: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub from_user: bool,
    pub text: Arc<str>,
    pub at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistantCommand {
    Timer(u64),
    OpenChat,
}

#[derive(Clone, Debug)]
pub enum AssistantInput {
    Ask(String),
    Clear,
    Listen,
    Cancel,
    Panel(bool),
    Retention(u64),
    DownloadModels,
    CancelDownload,
    RefreshModels,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ModelState {
    #[default]
    Missing,
    Downloading {
        file: String,
        received: u64,
        total: u64,
        attempt: u32,
    },
    Installing,
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssistantStatus {
    pub enabled: bool,
    pub voice_available: bool,
    pub models: ModelState,
    pub error: Option<String>,
}
