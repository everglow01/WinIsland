use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;
use winisland_core::assistant::{AssistantInput, AssistantSnapshot, AssistantState, ModelState};
use winisland_core::i18n::{tr, tr_args};

use crate::conversation::{Conversation, Via, card_text};
use crate::history::{History, Role};
use crate::{Event, Paths, Settings, SharedHost, models, voice};

pub struct Assistant {
    input: mpsc::UnboundedSender<AssistantInput>,
    settings: watch::Sender<(Settings, reqwest::Client)>,
    stop: CancellationToken,
    worker: JoinHandle<()>,
}

impl Assistant {
    pub fn start(
        settings: Settings,
        paths: Paths,
        client: reqwest::Client,
        host: SharedHost,
        defer_voice: bool,
    ) -> Self {
        let (input, receiver) = mpsc::unbounded_channel();
        let (settings, updates) = watch::channel((settings, client));
        let stop = CancellationToken::new();
        let cancelled = stop.clone();
        let worker = tokio::spawn(async move {
            let (settings, client) = updates.borrow().clone();
            match Session::new(settings, paths, client, Arc::clone(&host), defer_voice) {
                Ok(session) => session.run(receiver, updates, cancelled).await,
                Err(error) => host.emit(Event::Error(error)),
            }
            host.emit(Event::Stopped);
        });
        Self {
            input,
            settings,
            stop,
            worker,
        }
    }

    pub fn input(&self) -> mpsc::UnboundedSender<AssistantInput> {
        self.input.clone()
    }

    pub fn configure(&self, settings: Settings, client: reqwest::Client) -> Result<(), String> {
        self.settings
            .send((settings, client))
            .map_err(|_| "The assistant is not running".into())
    }

    pub fn is_running(&self) -> bool {
        !self.worker.is_finished() && !self.input.is_closed()
    }

    pub fn stop(&self) {
        self.stop.cancel();
    }
}

impl Drop for Assistant {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

struct Answer {
    id: u64,
    via: Via,
    text: Result<String, String>,
}

struct Session {
    settings: Settings,
    paths: Paths,
    client: reqwest::Client,
    host: SharedHost,
    history: History,
    conversation: Arc<Mutex<Conversation>>,
    voice: Option<voice::Voice>,
    voice_sender: mpsc::UnboundedSender<voice::Message>,
    voice_events: mpsc::UnboundedReceiver<voice::Message>,
    answers: JoinSet<Answer>,
    active: Option<(u64, CancellationToken)>,
    sequence: u64,
    downloads: JoinSet<Result<(), String>>,
    download_cancel: Option<CancellationToken>,
    panel_open: bool,
    voice_via: Via,
    follow_up: Option<String>,
    reply_until: Option<Instant>,
    model_error: Option<String>,
    defer_voice: bool,
}

impl Session {
    fn new(
        settings: Settings,
        paths: Paths,
        client: reqwest::Client,
        host: SharedHost,
        defer_voice: bool,
    ) -> Result<Self, String> {
        let history = History::load(paths.history());
        host.emit(Event::History(history.snapshot()));
        let conversation = Conversation::new(
            settings.clone(),
            paths.clone(),
            Arc::clone(&host),
            client.clone(),
            &history,
        )?;
        let (voice_sender, voice_events) = mpsc::unbounded_channel();
        let mut session = Self {
            settings,
            paths,
            client,
            host,
            history,
            conversation: Arc::new(Mutex::new(conversation)),
            voice: None,
            voice_sender,
            voice_events,
            answers: JoinSet::new(),
            active: None,
            sequence: 0,
            downloads: JoinSet::new(),
            download_cancel: None,
            panel_open: false,
            voice_via: Via::Voice,
            follow_up: None,
            reply_until: None,
            model_error: None,
            defer_voice,
        };
        session.refresh_models();
        Ok(session)
    }

    async fn run(
        mut self,
        mut input: mpsc::UnboundedReceiver<AssistantInput>,
        mut updates: watch::Receiver<(Settings, reqwest::Client)>,
        stop: CancellationToken,
    ) {
        let mut clock = tokio::time::interval(Duration::from_millis(100));
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                next = input.recv() => match next {
                    Some(command) => self.input(command).await,
                    None => break,
                },
                changed = updates.changed() => {
                    if changed.is_err() { break; }
                    let (settings, client) = updates.borrow_and_update().clone();
                    if let Err(error) = self.reconfigure(settings, client).await {
                        self.error(error);
                        break;
                    }
                },
                Some(event) = self.voice_events.recv() => self.voice_event(event),
                Some(answer) = self.answers.join_next(), if !self.answers.is_empty() => {
                    match answer {
                        Ok(answer) => self.finish_answer(answer),
                        Err(error) if error.is_cancelled() => {},
                        Err(error) => { self.active = None; self.error(error.to_string()); self.idle(); },
                    }
                },
                Some(result) = self.downloads.join_next(), if !self.downloads.is_empty() => {
                    let cancelled = self.download_cancel.take().is_some_and(|token| token.is_cancelled());
                    match result {
                        Ok(Ok(())) => { self.model_error = None; self.defer_voice = false; self.refresh_models(); },
                        _ if cancelled => {
                            if let Some(error) = &self.model_error { self.host.emit(Event::Models(ModelState::Failed(error.clone()))); }
                            else { self.refresh_models(); }
                        },
                        Ok(Err(error)) => self.host.emit(Event::Models(ModelState::Failed(error))),
                        Err(error) => self.host.emit(Event::Models(ModelState::Failed(error.to_string()))),
                    }
                },
                _ = clock.tick() => {
                    if self.reply_until.is_some_and(|until| Instant::now() >= until) {
                        self.reply_until = None;
                        self.idle();
                    }
                },
            }
        }
        input.close();
        self.cancel_answer();
        self.idle();
        if let Some(cancel) = self.download_cancel.take() {
            cancel.cancel();
        }
        if let Some(voice) = self.voice.take() {
            voice.shutdown().await;
        }
        while let Some(result) = self.downloads.join_next().await {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                log::warn!("Model download worker did not stop cleanly: {error}");
            }
        }
        self.host.emit(Event::VoiceAvailable(false));
        self.idle();
    }

    async fn reconfigure(
        &mut self,
        settings: Settings,
        client: reqwest::Client,
    ) -> Result<(), String> {
        self.cancel_answer();
        let restart_voice = settings.assistant.wake_words != self.settings.assistant.wake_words;
        if restart_voice {
            if let Some(voice) = self.voice.take() {
                voice.shutdown().await;
            }
            while self.voice_events.try_recv().is_ok() {}
            self.host.emit(Event::VoiceAvailable(false));
            self.model_error = None;
        } else if let Some(voice) = &self.voice {
            voice.stop_listening();
        }
        self.settings = settings;
        self.client = client;
        match Conversation::new(
            self.settings.clone(),
            self.paths.clone(),
            Arc::clone(&self.host),
            self.client.clone(),
            &self.history,
        ) {
            Ok(conversation) => self.conversation = Arc::new(Mutex::new(conversation)),
            Err(error) => return Err(error),
        }
        self.follow_up = None;
        self.reply_until = None;
        self.idle();
        self.refresh_models();
        Ok(())
    }

    async fn input(&mut self, input: AssistantInput) {
        match input {
            AssistantInput::Ask(text) if !text.trim().is_empty() => self.ask(text, Via::Text),
            AssistantInput::Ask(_) => {}
            AssistantInput::Listen => {
                self.cancel_answer();
                self.follow_up = None;
                self.reply_until = None;
                self.voice_via = if self.panel_open {
                    Via::PanelVoice
                } else {
                    Via::Voice
                };
                if let Some(voice) = &self.voice {
                    voice.listen(Duration::from_secs(8));
                    self.snapshot(AssistantState::Listening, "", 0.0, true);
                } else {
                    let text = tr("lingosoul_models_required");
                    self.snapshot(AssistantState::Replying, &text, 0.0, false);
                    self.reply_until = Some(Instant::now() + Duration::from_secs(5));
                }
            }
            AssistantInput::Cancel => {
                self.cancel_answer();
                if let Some(voice) = &self.voice {
                    voice.stop_listening();
                }
                self.follow_up = None;
                self.reply_until = None;
                self.idle();
            }
            AssistantInput::Panel(open) => {
                self.panel_open = open;
                if open {
                    self.voice_via = Via::PanelVoice;
                }
                self.follow_up = None;
                self.reply_until = None;
                if open && self.active.is_none() {
                    self.idle();
                }
            }
            AssistantInput::Clear => {
                self.cancel_answer();
                if let Some(voice) = &self.voice {
                    voice.stop_listening();
                }
                self.follow_up = None;
                self.reply_until = None;
                if let Err(error) = self.conversation.lock().await.reset() {
                    self.error(error);
                }
                if let Err(error) = self.history.clear() {
                    self.error(error);
                }
                self.host.emit(Event::History(self.history.snapshot()));
                self.idle();
            }
            AssistantInput::Retention(days) => {
                if let Err(error) = self.history.set_retention(days) {
                    self.error(error);
                }
                self.host.emit(Event::History(self.history.snapshot()));
            }
            AssistantInput::DownloadModels => self.start_download(),
            AssistantInput::CancelDownload => {
                if let Some(cancel) = &self.download_cancel {
                    cancel.cancel();
                }
            }
            AssistantInput::RefreshModels => {
                self.defer_voice = false;
                self.refresh_models();
            }
        }
    }

    fn ask(&mut self, text: String, via: Via) {
        self.cancel_answer();
        if let Some(voice) = &self.voice {
            voice.stop_listening();
        }
        self.follow_up = None;
        self.reply_until = None;
        if let Err(error) = self.history.push(Role::User, text.clone(), via.name()) {
            self.error(error);
        }
        self.host.emit(Event::History(self.history.snapshot()));
        self.sequence = self.sequence.wrapping_add(1);
        let id = self.sequence;
        let cancel = CancellationToken::new();
        self.active = Some((id, cancel.clone()));
        let conversation = Arc::clone(&self.conversation);
        self.snapshot(AssistantState::Thinking, "", 0.0, false);
        self.answers.spawn(async move {
            let text = conversation.lock().await.answer(&text, via, &cancel).await;
            Answer { id, via, text }
        });
    }

    fn cancel_answer(&mut self) {
        if let Some((_, cancel)) = self.active.take() {
            cancel.cancel();
        }
        self.answers.abort_all();
    }

    fn finish_answer(&mut self, answer: Answer) {
        if self.active.as_ref().map(|(id, _)| *id) != Some(answer.id) {
            return;
        }
        self.active = None;
        let text = match answer.text {
            Ok(text) => text,
            Err(error) => tr_args("lingosoul_request_failed", &[&error]),
        };
        if let Err(error) = self
            .history
            .push(Role::Assistant, text.clone(), answer.via.name())
        {
            self.error(error);
        }
        self.host.emit(Event::History(self.history.snapshot()));
        if answer.via == Via::Voice && !self.panel_open {
            let text = card_text(&text);
            self.snapshot(AssistantState::Replying, &text, 0.0, false);
            let hold =
                Duration::from_secs_f32((3.5 + 0.12 * text.chars().count() as f32).min(12.0));
            if let Some(voice) = &self.voice {
                voice.listen(hold + Duration::from_secs(5));
                self.follow_up = Some(text);
            } else {
                self.reply_until = Some(Instant::now() + hold);
            }
        } else {
            self.idle();
        }
    }

    fn voice_event(&mut self, message: voice::Message) {
        let Some(voice) = &self.voice else {
            return;
        };
        if message
            .generation
            .is_some_and(|generation| generation != voice.generation())
        {
            return;
        }
        match message.event {
            voice::Event::Ready => self.host.emit(Event::VoiceAvailable(true)),
            voice::Event::Reconnecting(error) => {
                self.host.emit(Event::VoiceAvailable(false));
                self.error(tr_args("lingosoul_microphone_reconnecting", &[&error]));
                self.follow_up = None;
                self.reply_until = None;
                if self.active.is_none() {
                    self.idle();
                }
            }
            voice::Event::Warning(warning) => {
                self.error(tr_args("lingosoul_wake_warning", &[&warning]))
            }
            voice::Event::Wake => {
                self.cancel_answer();
                self.follow_up = None;
                self.reply_until = None;
                self.voice_via = if self.panel_open {
                    Via::PanelVoice
                } else {
                    Via::Voice
                };
                self.snapshot(AssistantState::Listening, "", 0.0, true);
            }
            voice::Event::Level(level) if self.active.is_none() => {
                if let Some(text) = &self.follow_up {
                    self.snapshot(AssistantState::Replying, text, level, true);
                } else {
                    self.snapshot(AssistantState::Listening, "", level, true);
                }
            }
            voice::Event::Transcribing => self.snapshot(AssistantState::Thinking, "", 0.0, false),
            voice::Event::Text(text) if !text.is_empty() => self.ask(text, self.voice_via),
            voice::Event::Text(_) | voice::Event::Silence => {
                self.follow_up = None;
                if self.active.is_none() {
                    self.idle();
                }
            }
            voice::Event::Failed { error, model } => {
                self.voice = None;
                self.host.emit(Event::VoiceAvailable(false));
                self.follow_up = None;
                self.reply_until = None;
                if model {
                    self.model_error = Some(error.clone());
                    self.host.emit(Event::Models(ModelState::Failed(error)));
                } else {
                    self.error(tr_args("lingosoul_microphone_failed", &[&error]));
                }
                if self.active.is_none() {
                    self.idle();
                }
            }
            voice::Event::Level(_) => {}
        }
    }

    fn refresh_models(&mut self) {
        if !self.downloads.is_empty() || self.model_error.is_some() || self.defer_voice {
            return;
        }
        if models::ready(&self.paths.models) {
            self.host.emit(Event::Models(ModelState::Ready));
            if self.voice.is_none() {
                while self.voice_events.try_recv().is_ok() {}
                match voice::Voice::start(
                    self.paths.models.clone(),
                    self.settings.assistant.wake_words.clone(),
                    self.voice_sender.clone(),
                ) {
                    Ok(voice) => self.voice = Some(voice),
                    Err(error) => self.error(error),
                }
            }
        } else {
            self.host.emit(Event::Models(ModelState::Missing));
        }
    }

    fn start_download(&mut self) {
        if !self.downloads.is_empty() {
            return;
        }
        self.defer_voice = false;
        let voice = self.voice.take();
        self.host.emit(Event::VoiceAvailable(false));
        let cancel = CancellationToken::new();
        self.download_cancel = Some(cancel.clone());
        let paths = self.paths.clone();
        let client = self.client.clone();
        let host = Arc::clone(&self.host);
        let repair = self.model_error.is_some() || models::ready(&paths.models);
        self.downloads.spawn(async move {
            if let Some(voice) = voice {
                voice.shutdown().await;
            }
            crate::download_models(&paths, client, host, cancel, repair).await
        });
    }

    fn idle(&self) {
        self.host
            .emit(Event::Snapshot(AssistantSnapshot::default()));
    }
    fn error(&self, error: String) {
        self.host.emit(Event::Error(error));
    }
    fn snapshot(&self, state: AssistantState, text: &str, level: f32, listening: bool) {
        self.host.emit(Event::Snapshot(AssistantSnapshot {
            state,
            text: Arc::from(text),
            level,
            listening,
        }));
    }
}
