mod client;
mod prompt;

use std::collections::VecDeque;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::history::{History, Role};
use crate::tools::Tools;
use crate::{ApiFormat, Paths, Settings, SharedHost};

const HISTORY_TURNS: usize = 8;
const TOOL_ROUNDS: usize = 12;

pub(crate) fn initialize_prompts(paths: &Paths) -> Result<(), String> {
    prompt::ensure(&paths.prompts())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Via {
    Voice,
    Text,
    PanelVoice,
}

impl Via {
    pub fn name(self) -> &'static str {
        match self {
            Self::Voice | Self::PanelVoice => "voice",
            Self::Text => "text",
        }
    }
}

pub(crate) struct Conversation {
    settings: Settings,
    paths: Paths,
    host: SharedHost,
    client: reqwest::Client,
    tools: Tools,
    turns: VecDeque<Vec<Value>>,
}

impl Conversation {
    pub fn new(
        settings: Settings,
        paths: Paths,
        host: SharedHost,
        client: reqwest::Client,
        history: &History,
    ) -> Result<Self, String> {
        prompt::ensure(&paths.prompts())?;
        let mut records: Vec<_> = history.records().rev().take(HISTORY_TURNS * 2).map(|record| {
            json!({"role": match record.role { Role::User => "user", Role::Assistant => "assistant" }, "content": record.text})
        }).collect();
        records.reverse();
        let turns = records.chunks(2).map(<[Value]>::to_vec).collect();
        let tools = Tools::new(
            Arc::clone(&host),
            client.clone(),
            settings.local_agent.clone(),
        )?;
        Ok(Self {
            settings,
            paths,
            host,
            client,
            tools,
            turns,
        })
    }

    pub fn reset(&mut self) -> Result<(), String> {
        self.turns.clear();
        self.tools.reset()
    }

    pub async fn answer(
        &mut self,
        text: &str,
        via: Via,
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        let operation = self.answer_inner(text, via, cancel);
        tokio::select! {
            _ = cancel.cancelled() => Err("Request cancelled".into()),
            result = operation => result,
        }
    }

    async fn answer_inner(
        &mut self,
        text: &str,
        via: Via,
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        self.tools.refresh_plugins().await;
        let system = prompt::render(&self.settings, &self.paths, &self.host, via)?;
        let specs = self.tools.specs();
        let mut messages: Vec<Value> = self.turns.iter().flatten().cloned().collect();
        let start = messages.len();
        messages.push(json!({"role": "user", "content": text}));
        for _ in 0..TOOL_ROUNDS {
            let response =
                client::complete(&self.client, &self.settings.llm, &system, &messages, &specs)
                    .await?;
            messages.push(response.message);
            if response.calls.is_empty() {
                self.turns.push_back(messages.split_off(start));
                while self.turns.len() > HISTORY_TURNS {
                    self.turns.pop_front();
                }
                return Ok(response.text);
            }
            let mut results = Vec::new();
            for call in response.calls {
                if cancel.is_cancelled() {
                    return Err("Request cancelled".into());
                }
                let result = match call.arguments {
                    Ok(arguments) => self.tools.call(&call.name, &arguments, cancel).await,
                    Err(error) => Err(error),
                };
                let failed = result.is_err();
                let content = match result {
                    Ok(value) => json!({"ok": true, "result": value}).to_string(),
                    Err(error) => json!({"ok": false, "error": error}).to_string(),
                };
                match self.settings.llm.format {
                    ApiFormat::Openai => messages.push(json!({"role": "tool", "tool_call_id": call.id, "content": content})),
                    ApiFormat::Anthropic => results.push(json!({"type": "tool_result", "tool_use_id": call.id, "content": content, "is_error": failed})),
                }
            }
            if !results.is_empty() {
                messages.push(json!({"role": "user", "content": results}));
            }
        }
        Err("The assistant reached its tool-call limit; narrow the request and continue".into())
    }
}

pub(crate) fn card_text(text: &str) -> String {
    let flat = text
        .lines()
        .map(|line| line.trim().trim_start_matches(['#', '-', '*', '>', ' ']))
        .filter(|line| !line.is_empty() && !line.starts_with("```"))
        .collect::<Vec<_>>()
        .join(" ")
        .replace(['*', '`'], "");
    if flat.chars().count() > 200 {
        flat.chars().take(200).collect::<String>() + "…"
    } else {
        flat
    }
}
