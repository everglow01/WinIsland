use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
#[cfg(feature = "lingosoul")]
use winisland_core::assistant::ModelState;
pub use winisland_core::assistant::{
    AssistantCommand, AssistantSnapshot, AssistantState, ChatMessage,
};
use winisland_core::assistant::{AssistantInput, AssistantStatus};

#[derive(Default)]
struct Chat {
    revision: u64,
    messages: Vec<ChatMessage>,
}

pub struct AssistantRequest {
    pub id: u64,
    pub body: Value,
}

#[derive(Default)]
struct Shared {
    snapshot: Mutex<AssistantSnapshot>,
    commands: Mutex<Vec<AssistantCommand>>,
    requests: Mutex<Vec<AssistantRequest>>,
    replies: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
    chat: Mutex<Chat>,
    input: Mutex<Option<mpsc::UnboundedSender<AssistantInput>>>,
    status: Mutex<AssistantStatus>,
    status_revision: AtomicU64,
    #[cfg(feature = "lingosoul")]
    generation: AtomicU64,
    #[cfg(feature = "lingosoul")]
    next_request: AtomicU64,
}

static SHARED: LazyLock<Arc<Shared>> = LazyLock::new(|| Arc::new(Shared::default()));

#[cfg(feature = "lingosoul")]
pub fn assistant_status() -> AssistantStatus {
    SHARED
        .status
        .lock()
        .map(|status| status.clone())
        .unwrap_or_default()
}

pub fn status_revision() -> u64 {
    SHARED.status_revision.load(Ordering::Acquire)
}

impl Shared {
    fn update_status(&self, edit: impl FnOnce(&mut AssistantStatus)) {
        let changed = self.status.lock().is_ok_and(|mut status| {
            let previous = status.clone();
            edit(&mut status);
            *status != previous
        });
        if changed {
            self.status_revision.fetch_add(1, Ordering::AcqRel);
            crate::platform::wake();
        }
    }

    fn send(&self, input: AssistantInput) -> bool {
        self.input.lock().is_ok_and(|sender| {
            sender
                .as_ref()
                .is_some_and(|sender| sender.send(input).is_ok())
        })
    }
}

#[cfg(feature = "lingosoul")]
#[derive(Default)]
struct Runtime {
    enabled: bool,
    assistant: Option<winisland_lingosoul::Assistant>,
    download: Option<(
        tokio_util::sync::CancellationToken,
        tokio::task::JoinHandle<()>,
    )>,
}

pub struct AssistantLink {
    shared: Arc<Shared>,
    #[cfg(feature = "lingosoul")]
    runtime: Runtime,
}

impl AssistantLink {
    pub fn start() -> Self {
        Self {
            shared: Arc::clone(&SHARED),
            #[cfg(feature = "lingosoul")]
            runtime: Runtime::default(),
        }
    }

    pub fn snapshot(&self) -> AssistantSnapshot {
        self.shared
            .snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_default()
    }

    pub fn take_commands(&self) -> Vec<AssistantCommand> {
        self.shared
            .commands
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }

    pub fn take_requests(&self) -> Vec<AssistantRequest> {
        self.shared
            .requests
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }

    pub fn request_pending(&self, id: u64) -> bool {
        self.shared
            .replies
            .lock()
            .is_ok_and(|replies| replies.get(&id).is_some_and(|reply| !reply.is_closed()))
    }

    pub fn is_connected(&self) -> bool {
        self.shared
            .input
            .lock()
            .is_ok_and(|sender| sender.as_ref().is_some_and(|sender| !sender.is_closed()))
    }

    pub fn chat_revision(&self) -> u64 {
        self.shared.chat.lock().map_or(0, |chat| chat.revision)
    }

    pub fn has_messages(&self) -> bool {
        self.shared
            .chat
            .lock()
            .is_ok_and(|chat| !chat.messages.is_empty())
    }

    pub fn chat(&self) -> Vec<ChatMessage> {
        self.shared
            .chat
            .lock()
            .map(|chat| chat.messages.clone())
            .unwrap_or_default()
    }

    pub fn ask(&self, text: &str) -> bool {
        self.shared.send(AssistantInput::Ask(text.to_owned()))
    }
    pub fn clear(&self) -> bool {
        self.shared.send(AssistantInput::Clear)
    }
    pub fn listen(&self) -> bool {
        self.shared.send(AssistantInput::Listen)
    }
    pub fn cancel(&self) -> bool {
        self.shared.send(AssistantInput::Cancel)
    }
    pub fn set_panel_open(&self, open: bool) -> bool {
        self.shared.send(AssistantInput::Panel(open))
    }
    pub fn set_retention(&self, days: u64) -> bool {
        self.shared.send(AssistantInput::Retention(days))
    }

    pub fn voice_available(&self) -> bool {
        self.shared
            .status
            .lock()
            .is_ok_and(|status| status.voice_available)
    }

    pub fn reply(&self, id: u64, body: Value) {
        if let Ok(mut replies) = self.shared.replies.lock()
            && let Some(reply) = replies.remove(&id)
        {
            let _ = reply.send(body);
        }
    }

    #[cfg(feature = "lingosoul")]
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.runtime.enabled == enabled {
            return;
        }
        self.runtime.enabled = enabled;
        if enabled {
            self.reload();
        } else {
            self.stop();
        }
    }

    #[cfg(feature = "lingosoul")]
    pub fn reload(&mut self) {
        if !self.runtime.enabled {
            return;
        }
        let settings = match winisland_lingosoul::Settings::load(
            &crate::core::lingosoul_config::config_path(),
        ) {
            Ok(settings) => settings,
            Err(error) => {
                self.shared
                    .update_status(|status| status.error = Some(error));
                return;
            }
        };
        let client = match http_client(&settings.llm.proxy) {
            Ok(client) => client,
            Err(error) => {
                self.shared
                    .update_status(|status| status.error = Some(error));
                return;
            }
        };
        self.shared.update_status(|status| {
            status.enabled = true;
            status.error = None;
        });
        if let Some(assistant) = self
            .runtime
            .assistant
            .as_ref()
            .filter(|assistant| assistant.is_running())
        {
            if let Err(error) = assistant.configure(settings, client) {
                self.shared
                    .update_status(|status| status.error = Some(error));
            }
            return;
        }
        let generation = self
            .shared
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let host: winisland_lingosoul::SharedHost = Arc::new(Bridge {
            shared: Arc::clone(&self.shared),
            generation: Some(generation),
        });
        let assistant = winisland_lingosoul::Assistant::start(
            settings,
            crate::core::lingosoul_config::paths(),
            client,
            host,
            self.runtime
                .download
                .as_ref()
                .is_some_and(|(_, task)| !task.is_finished()),
        );
        if let Ok(mut input) = self.shared.input.lock() {
            *input = Some(assistant.input());
        }
        self.runtime.assistant = Some(assistant);
    }

    #[cfg(feature = "lingosoul")]
    pub fn download_models(&mut self) {
        if self
            .runtime
            .download
            .as_ref()
            .is_some_and(|(_, task)| !task.is_finished())
        {
            return;
        }
        if matches!(
            assistant_status().models,
            ModelState::Downloading { .. } | ModelState::Installing
        ) {
            return;
        }
        if self.is_connected() {
            self.shared.send(AssistantInput::DownloadModels);
            return;
        }
        let settings = match winisland_lingosoul::Settings::load(
            &crate::core::lingosoul_config::config_path(),
        ) {
            Ok(settings) => settings,
            Err(error) => {
                self.shared
                    .update_status(|status| status.error = Some(error));
                return;
            }
        };
        let client = match http_client(&settings.llm.proxy) {
            Ok(client) => client,
            Err(error) => {
                self.shared
                    .update_status(|status| status.error = Some(error));
                return;
            }
        };
        let paths = crate::core::lingosoul_config::paths();
        let previous = assistant_status().models;
        let repair = winisland_lingosoul::models_ready(&paths);
        let host: winisland_lingosoul::SharedHost = Arc::new(Bridge {
            shared: Arc::clone(&self.shared),
            generation: None,
        });
        let cancel = tokio_util::sync::CancellationToken::new();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            let result = winisland_lingosoul::download_models(
                &paths,
                client,
                Arc::clone(&host),
                token.clone(),
                repair,
            )
            .await;
            if token.is_cancelled() {
                let restored = if matches!(previous, ModelState::Failed(_)) {
                    previous
                } else if winisland_lingosoul::models_ready(&paths) {
                    ModelState::Ready
                } else {
                    ModelState::Missing
                };
                host.emit(winisland_lingosoul::Event::Models(restored));
            } else if let Err(error) = result {
                host.emit(winisland_lingosoul::Event::Models(ModelState::Failed(
                    error,
                )));
            }
        });
        self.runtime.download = Some((cancel, task));
    }

    #[cfg(feature = "lingosoul")]
    pub fn cancel_download(&self) {
        if let Some((cancel, _)) = &self.runtime.download {
            cancel.cancel();
        }
        self.shared.send(AssistantInput::CancelDownload);
    }

    pub fn stop(&mut self) {
        #[cfg(feature = "lingosoul")]
        {
            self.shared.generation.fetch_add(1, Ordering::AcqRel);
            let active = self.runtime.assistant.is_some();
            self.runtime.assistant = None;
            let downloading = self
                .runtime
                .download
                .as_ref()
                .is_some_and(|(_, task)| !task.is_finished());
            if active && !downloading {
                let models =
                    if winisland_lingosoul::models_ready(&crate::core::lingosoul_config::paths()) {
                        ModelState::Ready
                    } else {
                        ModelState::Missing
                    };
                self.shared.update_status(|status| {
                    if matches!(
                        status.models,
                        ModelState::Downloading { .. } | ModelState::Installing
                    ) {
                        status.models = models;
                    }
                });
            }
        }
        if let Ok(mut input) = self.shared.input.lock() {
            *input = None;
        }
        if let Ok(mut replies) = self.shared.replies.lock() {
            replies.clear();
        }
        if let Ok(mut requests) = self.shared.requests.lock() {
            requests.clear();
        }
        if let Ok(mut commands) = self.shared.commands.lock() {
            commands.clear();
        }
        if let Ok(mut snapshot) = self.shared.snapshot.lock() {
            *snapshot = AssistantSnapshot::default();
        }
        self.shared.update_status(|status| {
            status.enabled = false;
            status.voice_available = false;
            status.error = None;
        });
    }

    pub fn shutdown(&mut self) {
        self.stop();
        #[cfg(feature = "lingosoul")]
        if let Some((cancel, _)) = self.runtime.download.take() {
            cancel.cancel();
        }
    }
}

impl Drop for AssistantLink {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(feature = "lingosoul")]
fn http_client(proxy: &str) -> Result<reqwest::Client, String> {
    crate::utils::proxy::apply(reqwest::Client::builder(), proxy)
        .connect_timeout(std::time::Duration::from_secs(15))
        .user_agent("WinIsland-LingoSoul")
        .build()
        .map_err(|error| error.without_url().to_string())
}

#[cfg(feature = "lingosoul")]
struct Bridge {
    shared: Arc<Shared>,
    generation: Option<u64>,
}

#[cfg(feature = "lingosoul")]
impl Bridge {
    fn live(&self) -> bool {
        self.generation
            .is_none_or(|generation| generation == self.shared.generation.load(Ordering::Acquire))
    }
}

#[cfg(feature = "lingosoul")]
impl winisland_lingosoul::Host for Bridge {
    fn emit(&self, event: winisland_lingosoul::Event) {
        use winisland_lingosoul::Event;
        if !self.live() {
            return;
        }
        match event {
            Event::Snapshot(value) => {
                if let Ok(mut snapshot) = self.shared.snapshot.lock() {
                    *snapshot = value;
                }
            }
            Event::Command(command) => {
                if let Ok(mut commands) = self.shared.commands.lock() {
                    commands.push(command);
                }
            }
            Event::History(messages) => {
                if let Ok(mut chat) = self.shared.chat.lock() {
                    chat.messages = messages;
                    chat.revision = chat.revision.wrapping_add(1);
                }
            }
            Event::VoiceAvailable(available) => {
                self.shared.update_status(|status| {
                    status.voice_available = available;
                    if available {
                        status.error = None;
                    }
                });
            }
            Event::Models(models) => {
                let ready = models == ModelState::Ready;
                self.shared.update_status(|status| {
                    if models == ModelState::Missing
                        && self.generation.is_some()
                        && matches!(
                            status.models,
                            ModelState::Downloading { .. } | ModelState::Installing
                        )
                    {
                        return;
                    }
                    if status.models != models {
                        match &models {
                            ModelState::Installing => {
                                log::info!("LingoSoul: installing speech models")
                            }
                            ModelState::Ready => {
                                log::info!("LingoSoul: speech models are installed")
                            }
                            ModelState::Failed(error) => {
                                log::error!("LingoSoul speech models failed: {error}")
                            }
                            ModelState::Missing | ModelState::Downloading { .. } => {}
                        }
                    }
                    status.models = models;
                    if ready {
                        status.error = None;
                    }
                });
                if ready && self.generation.is_none() {
                    self.shared.send(AssistantInput::RefreshModels);
                }
            }
            Event::Error(error) => self
                .shared
                .update_status(|status| status.error = Some(error)),
            Event::Stopped => {
                if let Ok(mut input) = self.shared.input.lock() {
                    *input = None;
                }
                self.shared.update_status(|status| {
                    status.enabled = false;
                    status.voice_available = false;
                });
            }
        }
        crate::platform::wake();
    }

    fn request(&self, body: Value) -> oneshot::Receiver<Value> {
        let (reply, receiver) = oneshot::channel();
        if !self.live() {
            return receiver;
        }
        let id = self.shared.next_request.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut replies) = self.shared.replies.lock() {
            replies.retain(|_, reply| !reply.is_closed());
            if replies.len() >= 128 {
                return receiver;
            }
            replies.insert(id, reply);
        } else {
            return receiver;
        }
        if let Ok(mut requests) = self.shared.requests.lock() {
            requests.push(AssistantRequest { id, body });
        }
        crate::platform::wake();
        receiver
    }

    fn language(&self) -> String {
        winisland_core::i18n::current_lang()
    }

    fn local_time(&self) -> String {
        let time = crate::platform::shell().local_datetime();
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            time.year, time.month, time.day, time.hour, time.minute, time.second
        )
    }

    fn open_app(&self, name: &str) -> Result<String, String> {
        crate::platform::shell()
            .launch_application(name)
            .map_err(|error| error.to_string())
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        crate::platform::shell()
            .open_url(url)
            .map_err(|error| error.to_string())
    }

    fn media_control(&self, action: &str, percent: Option<u64>) -> Result<(), String> {
        use winisland_platform::MediaCommand;
        if matches!(action, "mute" | "volume_up" | "volume_down" | "set_volume") {
            let audio = crate::platform::audio();
            if action == "mute" {
                return audio.toggle_mute().map_err(|error| error.to_string());
            }
            let percent = if action == "set_volume" {
                percent.ok_or("set_volume requires percent")?
            } else {
                percent.unwrap_or(10)
            };
            if percent > 100 {
                return Err("Volume percent must be from 0 to 100".into());
            }
            let amount = percent as f32 / 100.0;
            let level = match action {
                "set_volume" => amount,
                "volume_up" => audio.volume().map_err(|error| error.to_string())?.level + amount,
                "volume_down" => audio.volume().map_err(|error| error.to_string())?.level - amount,
                _ => return Err("Unsupported volume operation".into()),
            };
            return audio
                .set_volume(level.clamp(0.0, 1.0))
                .map_err(|error| error.to_string());
        }
        let context = crate::platform::media()
            .open_context()
            .map_err(|error| error.to_string())?;
        let session = context.current().ok_or("No media session is available")?;
        let command = match action {
            "play_pause" => MediaCommand::Toggle,
            "next" => MediaCommand::Next,
            "previous" => MediaCommand::Previous,
            _ => return Err("Unsupported media operation".into()),
        };
        match session.send(command).map_err(|error| error.to_string())? {
            true => Ok(()),
            false => Err("The media session rejected the command".into()),
        }
    }
}

#[cfg(feature = "lingosoul")]
pub fn history_path() -> std::path::PathBuf {
    crate::core::lingosoul_config::paths().history()
}

#[cfg(feature = "lingosoul")]
pub fn clear_history_file() {
    let path = history_path();
    if path.exists()
        && let Err(error) = std::fs::write(path, "")
    {
        SHARED.update_status(|status| status.error = Some(error.to_string()));
        return;
    }
    if let Ok(mut chat) = SHARED.chat.lock() {
        chat.messages.clear();
        chat.revision = chat.revision.wrapping_add(1);
    }
    crate::platform::wake();
}
