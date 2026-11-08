use std::path::PathBuf;

use winisland_core::assistant::ModelState;
use winisland_core::config::AppConfigField;
use winisland_core::i18n::{tr, tr_args};
use winisland_lingosoul::ApiFormat;

use crate::core::assistant::{assistant_status, history_path};
use crate::core::lingosoul_config::{LingoSoulConfig, paths};
use crate::utils::settings_ui::ClickResult;
use crate::utils::settings_ui::items::{CONTENT_PADDING, GROUP_INNER_PAD, SettingsItem};

use super::super::{PluginSettingsRequest, PopupState, SettingsApp};
use super::{PageInput, SettingsPage};

#[derive(Clone, Copy)]
pub(crate) enum LingoSoulField {
    Name,
    WakeWords,
    ApiKey,
    Model,
    BaseUrl,
    Proxy,
}

#[derive(Clone, Copy)]
pub(crate) enum Permission {
    Read,
    Write,
    Execute,
}

#[derive(Clone, Copy)]
pub(crate) enum LingoSoulAction {
    Text(LingoSoulField),
    Format,
    Models,
    OpenChat,
    OpenHistory,
    Clear,
    Agent,
    Permission(Permission),
    Workspace,
    AddReadDirectory,
    ReadDirectory(usize),
    EditPrompt,
}

impl LingoSoulField {
    fn value(self, config: &LingoSoulConfig) -> String {
        match self {
            Self::Name => config.settings.assistant.name.clone(),
            Self::WakeWords => config.wake_words(),
            Self::ApiKey => config.settings.llm.api_key.clone(),
            Self::Model => config.settings.llm.model.clone(),
            Self::BaseUrl => config.settings.llm.base_url.clone(),
            Self::Proxy => config.settings.llm.proxy.clone(),
        }
    }
}

fn save_config(app: &mut SettingsApp, config: &LingoSoulConfig) {
    match config.save() {
        Ok(()) => {
            app.lingosoul_error = None;
            app.plugin_request = Some(PluginSettingsRequest::ReloadLingoSoul);
        }
        Err(error) => app.lingosoul_error = Some(error),
    }
    app.mark_items_dirty();
    app.request_redraw();
}

fn commit_text(app: &mut SettingsApp, value: &str) {
    let Some(field) = app.lingosoul_field.take() else {
        return;
    };
    let mut config = LingoSoulConfig::load();
    let value = value.trim();
    if field.value(&config) == value {
        return;
    }
    match field {
        LingoSoulField::Name => config.settings.assistant.name = value.into(),
        LingoSoulField::WakeWords => config.set_wake_words(value),
        LingoSoulField::ApiKey => config.settings.llm.api_key = value.into(),
        LingoSoulField::Model => config.settings.llm.model = value.into(),
        LingoSoulField::BaseUrl => config.settings.llm.base_url = value.into(),
        LingoSoulField::Proxy => config.settings.llm.proxy = value.into(),
    }
    save_config(app, &config);
}

fn select_format(app: &mut SettingsApp, value: &str) {
    let mut config = LingoSoulConfig::load();
    config.settings.llm.format = match value {
        "openai" => ApiFormat::Openai,
        "anthropic" => ApiFormat::Anthropic,
        _ => return,
    };
    save_config(app, &config);
}

fn field(
    page: &mut SettingsPage<LingoSoulAction>,
    config: &LingoSoulConfig,
    label: &str,
    placeholder: &str,
    field: LingoSoulField,
) {
    page.row_text(
        tr(label),
        field.value(config),
        tr(placeholder),
        matches!(field, LingoSoulField::ApiKey),
        LingoSoulAction::Text(field),
    );
}

fn model_status(state: &ModelState) -> String {
    match state {
        ModelState::Missing => tr("lingosoul_models_missing"),
        ModelState::Ready => tr("lingosoul_models_ready"),
        ModelState::Installing => tr("lingosoul_models_installing"),
        ModelState::Failed(error) => tr_args("lingosoul_models_failed", &[error]),
        ModelState::Downloading {
            file,
            received,
            total,
            attempt,
        } => {
            let name = tr(match file.as_str() {
                "silero-vad" => "lingosoul_model_vad",
                "wake-word" => "lingosoul_model_wake",
                _ => "lingosoul_model_asr",
            });
            let percent = if *total == 0 {
                0.0
            } else {
                (*received).min(*total) as f64 / *total as f64 * 100.0
            };
            tr_args(
                "lingosoul_models_progress",
                &[&name, &format!("{percent:.0}"), &attempt.to_string()],
            )
        }
    }
}

fn note(page: &mut SettingsPage<LingoSoulAction>, text: String, width: f32) {
    for paragraph in text.lines() {
        for line in winisland_render::text::FontManager::global().wrap_text(
            paragraph,
            13.0,
            winisland_render::FontWeight::REGULAR,
            width,
        ) {
            page.row_label(line);
        }
    }
}

impl SettingsApp {
    fn build_lingosoul_page(&self) -> SettingsPage<LingoSoulAction> {
        let config = LingoSoulConfig::load();
        let note_width =
            (self.content_width() - CONTENT_PADDING * 2.0 - GROUP_INNER_PAD * 2.0).max(1.0);
        let mut status = assistant_status();
        if status.models == ModelState::Missing && winisland_lingosoul::models_ready(&paths()) {
            status.models = ModelState::Ready;
        }
        let working = matches!(
            status.models,
            ModelState::Downloading { .. } | ModelState::Installing
        );
        let mut page = SettingsPage::new();
        page.section(tr("tab_lingosoul"));
        page.group_start();
        page.setting(&self.config, AppConfigField::LingosoulEnabled, true);
        page.row_button(
            tr("lingosoul_speech_models"),
            tr(if working {
                "lingosoul_models_cancel"
            } else if status.models == ModelState::Ready {
                "lingosoul_models_repair"
            } else {
                "lingosoul_models_download"
            }),
            true,
            LingoSoulAction::Models,
        );
        note(&mut page, model_status(&status.models), note_width);
        if let Some(error) = self.lingosoul_error.as_ref().or(status.error.as_ref()) {
            note(&mut page, error.clone(), note_width);
        }
        page.row_button(
            tr("lingosoul_chat_open"),
            tr("lingosoul_chat_open_button"),
            status.enabled,
            LingoSoulAction::OpenChat,
        );
        page.group_end();
        if let Some(error) = &config.error {
            page.group_start();
            note(
                &mut page,
                tr_args("lingosoul_config_error", &[error]),
                note_width,
            );
            page.row_label(
                crate::core::lingosoul_config::config_path()
                    .display()
                    .to_string(),
            );
            page.group_end();
            return page;
        }

        page.section(tr("lingosoul_assistant_title"));
        page.group_start();
        field(
            &mut page,
            &config,
            "lingosoul_name",
            "lingosoul_name_hint",
            LingoSoulField::Name,
        );
        field(
            &mut page,
            &config,
            "lingosoul_wake_words",
            "lingosoul_wake_words_hint",
            LingoSoulField::WakeWords,
        );
        page.setting(&self.config, AppConfigField::LingosoulHoldToTalk, true);
        note(&mut page, tr("lingosoul_hold_to_talk_note"), note_width);
        page.group_end();

        page.section(tr("lingosoul_llm_title"));
        page.group_start();
        page.row_source(
            tr("lingosoul_api_format"),
            vec![
                (
                    tr("lingosoul_format_openai"),
                    config.settings.llm.format == ApiFormat::Openai,
                ),
                (
                    tr("lingosoul_format_anthropic"),
                    config.settings.llm.format == ApiFormat::Anthropic,
                ),
            ],
            true,
            LingoSoulAction::Format,
        );
        field(
            &mut page,
            &config,
            "lingosoul_llm_url",
            "lingosoul_llm_url_hint",
            LingoSoulField::BaseUrl,
        );
        field(
            &mut page,
            &config,
            "lingosoul_llm_model",
            "lingosoul_llm_model_hint",
            LingoSoulField::Model,
        );
        field(
            &mut page,
            &config,
            "lingosoul_llm_key",
            "lingosoul_llm_key_hint",
            LingoSoulField::ApiKey,
        );
        field(
            &mut page,
            &config,
            "lingosoul_llm_proxy",
            "lingosoul_llm_proxy_hint",
            LingoSoulField::Proxy,
        );
        note(&mut page, tr("lingosoul_network_note"), note_width);
        page.group_end();

        page.section(tr("lingosoul_prompts_title"));
        page.group_start();
        page.row_button(
            tr("lingosoul_prompt_file"),
            tr("lingosoul_prompt_edit"),
            true,
            LingoSoulAction::EditPrompt,
        );
        page.group_end();

        page.section(tr("lingosoul_local_agent_title"));
        page.group_start();
        let agent = &config.settings.local_agent;
        page.row_switch(
            tr("lingosoul_local_agent_enabled"),
            agent.enabled,
            true,
            LingoSoulAction::Agent,
        );
        if agent.enabled {
            let workspace = (!agent.workspace.as_os_str().is_empty())
                .then(|| agent.workspace.display().to_string());
            page.row_folder(
                tr("lingosoul_workspace"),
                tr("folder_select"),
                workspace.as_ref().map(|_| tr("folder_clear")),
                workspace,
                true,
                LingoSoulAction::Workspace,
            );
            for (label, allowed, permission) in [
                (
                    "lingosoul_permission_read",
                    agent.allow_read,
                    Permission::Read,
                ),
                (
                    "lingosoul_permission_write",
                    agent.allow_write,
                    Permission::Write,
                ),
                (
                    "lingosoul_permission_execute",
                    agent.allow_execute,
                    Permission::Execute,
                ),
            ] {
                page.row_switch(
                    tr(label),
                    allowed,
                    true,
                    LingoSoulAction::Permission(permission),
                );
            }
            note(&mut page, tr("lingosoul_agent_scope_note"), note_width);
            if agent.allow_execute {
                note(&mut page, tr("lingosoul_execute_note"), note_width);
            }
            for (index, directory) in agent.read_directories.iter().enumerate() {
                page.row_folder(
                    tr("lingosoul_read_directory"),
                    tr("folder_select"),
                    Some(tr("folder_clear")),
                    Some(directory.display().to_string()),
                    true,
                    LingoSoulAction::ReadDirectory(index),
                );
            }
            page.row_button(
                tr("lingosoul_add_read_directory"),
                tr("folder_select"),
                true,
                LingoSoulAction::AddReadDirectory,
            );
        }
        page.group_end();

        page.section(tr("lingosoul_history_title"));
        page.group_start();
        page.row_button(
            tr("lingosoul_history_folder"),
            tr("lingosoul_history_open"),
            true,
            LingoSoulAction::OpenHistory,
        );
        page.setting(
            &self.config,
            AppConfigField::LingosoulHistoryRetention,
            true,
        );
        page.row_button(
            tr("lingosoul_history_clear"),
            tr("lingosoul_chat_clear"),
            true,
            LingoSoulAction::Clear,
        );
        page.group_end();
        page
    }

    pub(crate) fn build_lingosoul_items(&self) -> Vec<SettingsItem> {
        self.build_lingosoul_page().into_items()
    }

    pub(crate) fn handle_lingosoul_click(&mut self, input: PageInput) {
        let page = self.build_lingosoul_page();
        let result = input.hit_test(&page);
        if self.handle_setting_click(&page, &result, input) {
            return;
        }
        let Some(action) = page.action(&result).copied() else {
            return;
        };
        match (action, &result) {
            (LingoSoulAction::Models, ClickResult::RowButton(_)) => {
                self.plugin_request = Some(
                    if matches!(
                        assistant_status().models,
                        ModelState::Downloading { .. } | ModelState::Installing
                    ) {
                        PluginSettingsRequest::CancelLingoSoulDownload
                    } else {
                        PluginSettingsRequest::DownloadLingoSoulModels
                    },
                );
            }
            (LingoSoulAction::OpenChat, ClickResult::RowButton(_)) => {
                self.plugin_request = Some(PluginSettingsRequest::OpenLingoSoulChat)
            }
            (LingoSoulAction::OpenHistory, ClickResult::RowButton(_)) => {
                let history = history_path();
                let directory = history
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or(std::path::Path::new("."));
                let result = std::fs::create_dir_all(directory)
                    .map_err(|error| error.to_string())
                    .and_then(|()| {
                        crate::platform::shell()
                            .reveal_path(directory)
                            .map_err(|error| error.to_string())
                    });
                self.lingosoul_error = result.err();
                self.mark_items_dirty();
                self.request_redraw();
            }
            (LingoSoulAction::Clear, ClickResult::RowButton(_)) => {
                self.plugin_request = Some(PluginSettingsRequest::ClearLingoSoulHistory)
            }
            (LingoSoulAction::Text(field), ClickResult::TextField(index)) => {
                self.lingosoul_field = Some(field);
                let rect = input.text_field_rect(&page, *index, self.scroll_y);
                self.begin_text_input(rect, field.value(&LingoSoulConfig::load()), commit_text);
            }
            (LingoSoulAction::Format, ClickResult::SourceButton(index)) => {
                let config = LingoSoulConfig::load();
                let rect = input.popup_button_rect(&page, *index, self.scroll_y);
                let (width, height) = self.logical_window_size();
                self.show_popup(PopupState::new(
                    select_format,
                    rect,
                    vec![
                        tr("lingosoul_format_openai"),
                        tr("lingosoul_format_anthropic"),
                    ],
                    vec!["openai".into(), "anthropic".into()],
                    usize::from(config.settings.llm.format == ApiFormat::Anthropic),
                    width,
                    height,
                ));
            }
            (LingoSoulAction::Agent, ClickResult::Switch(_)) => {
                let mut config = LingoSoulConfig::load();
                config.settings.local_agent.enabled = !config.settings.local_agent.enabled;
                save_config(self, &config);
            }
            (LingoSoulAction::Permission(permission), ClickResult::Switch(_)) => {
                let mut config = LingoSoulConfig::load();
                let enabled = match permission {
                    Permission::Read => &mut config.settings.local_agent.allow_read,
                    Permission::Write => &mut config.settings.local_agent.allow_write,
                    Permission::Execute => &mut config.settings.local_agent.allow_execute,
                };
                *enabled = !*enabled;
                save_config(self, &config);
            }
            (
                LingoSoulAction::Workspace
                | LingoSoulAction::ReadDirectory(_)
                | LingoSoulAction::AddReadDirectory,
                ClickResult::FolderSelect(_) | ClickResult::RowButton(_),
            ) => {
                let Some(directory) = rfd::FileDialog::new().pick_folder() else {
                    return;
                };
                let mut config = LingoSoulConfig::load();
                match action {
                    LingoSoulAction::Workspace => config.settings.local_agent.workspace = directory,
                    LingoSoulAction::ReadDirectory(index) => {
                        if let Some(current) =
                            config.settings.local_agent.read_directories.get_mut(index)
                        {
                            *current = directory;
                        }
                    }
                    LingoSoulAction::AddReadDirectory => {
                        if !config
                            .settings
                            .local_agent
                            .read_directories
                            .contains(&directory)
                        {
                            config.settings.local_agent.read_directories.push(directory);
                        }
                    }
                    _ => return,
                }
                save_config(self, &config);
            }
            (
                LingoSoulAction::Workspace | LingoSoulAction::ReadDirectory(_),
                ClickResult::FolderClear(_),
            ) => {
                let mut config = LingoSoulConfig::load();
                match action {
                    LingoSoulAction::Workspace => {
                        config.settings.local_agent.workspace = PathBuf::new()
                    }
                    LingoSoulAction::ReadDirectory(index)
                        if index < config.settings.local_agent.read_directories.len() =>
                    {
                        config.settings.local_agent.read_directories.remove(index);
                    }
                    _ => return,
                }
                save_config(self, &config);
            }
            (LingoSoulAction::EditPrompt, ClickResult::RowButton(_)) => {
                let result = winisland_lingosoul::initialize_prompts(&paths()).and_then(|()| {
                    crate::platform::shell()
                        .reveal_path(&LingoSoulConfig::load().prompt_path())
                        .map_err(|error| error.to_string())
                });
                self.lingosoul_error = result.err();
                self.mark_items_dirty();
                self.request_redraw();
            }
            _ => {}
        }
    }
}
