mod agent;
mod weather;

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use winisland_core::assistant::AssistantCommand;

use crate::{AgentSettings, Event, SharedHost};

#[derive(Clone, Deserialize)]
pub(crate) struct ToolSpec {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

pub(crate) struct Tools {
    host: SharedHost,
    client: reqwest::Client,
    catalog: Vec<ToolSpec>,
    agent: agent::AgentTools,
    plugins: Option<Value>,
    plugin_routes: HashMap<String, String>,
}

impl Tools {
    pub fn reset(&self) -> Result<(), String> {
        self.agent.reset()
    }

    pub fn new(
        host: SharedHost,
        client: reqwest::Client,
        settings: AgentSettings,
    ) -> Result<Self, String> {
        let catalog = serde_json::from_str(include_str!("../../resources/tools.json"))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            host,
            client,
            catalog,
            agent: agent::AgentTools::new(settings),
            plugins: None,
            plugin_routes: HashMap::new(),
        })
    }

    pub async fn refresh_plugins(&mut self) {
        self.plugins = self
            .request(json!({"request": "plugins"}), Duration::from_secs(2))
            .await
            .ok();
        self.plugin_routes.clear();
        if let Some(plugins) = &self.plugins {
            for (index, tool) in plugins["tools"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                if let Some(id) = tool["name"].as_str() {
                    self.plugin_routes
                        .insert(format!("plugin_{index}"), id.to_owned());
                }
            }
        }
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        let mut specs: Vec<_> = self
            .catalog
            .iter()
            .filter(|tool| {
                !matches!(tool.name.as_str(), "read" | "write" | "update" | "execute")
                    || self.agent.available(&tool.name)
            })
            .cloned()
            .collect();
        if !self.agent.settings().allow_write
            && let Some(update) = specs.iter_mut().find(|tool| tool.name == "update")
        {
            update.schema["properties"]["operation"]["enum"] = json!(["preview"]);
        }
        if let Some(plugins) = &self.plugins {
            specs.push(ToolSpec {
                name: "list_plugins".into(), description: "List installed WinIsland plugins, commands, tools, and their visible activities.".into(),
                schema: json!({"type": "object", "properties": {}}),
            });
            let commands: Vec<_> = plugins["commands"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|command| command["id"].as_str())
                .collect();
            if !commands.is_empty() {
                specs.push(ToolSpec {
                    name: "run_plugin_command".into(), description: "Run an available WinIsland plugin command.".into(),
                    schema: json!({"type": "object", "properties": {"command": {"type": "string", "enum": commands}}, "required": ["command"]}),
                });
            }
            for (name, id) in &self.plugin_routes {
                if let Some(tool) = plugins["tools"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|tool| tool["name"].as_str() == Some(id))
                {
                    specs.push(ToolSpec {
                        name: name.clone(),
                        description: format!(
                            "[WinIsland plugin] {}",
                            tool["description"].as_str().unwrap_or_default()
                        ),
                        schema: tool
                            .get("schema")
                            .filter(|value| value.is_object())
                            .cloned()
                            .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
                    });
                }
            }
        }
        specs
    }

    pub async fn call(
        &self,
        name: &str,
        args: &Value,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        if cancel.is_cancelled() {
            return Err("Operation cancelled".into());
        }
        match name {
            "read" | "write" | "update" | "execute" => self.agent.call(name, args, cancel).await,
            "set_timer" => {
                let seconds = args.get("seconds").and_then(Value::as_u64).ok_or("Missing timer duration")?;
                if seconds > 86_400 { return Err("The timer cannot exceed 24 hours".into()); }
                self.host.emit(Event::Command(AssistantCommand::Timer(seconds)));
                Ok(json!({"seconds": seconds, "cancelled": seconds == 0}))
            }
            "get_weather" => weather::get(&self.client, args.get("city").and_then(Value::as_str).unwrap_or_default(), &self.host.language()).await,
            "open_app" => {
                let query = required_text(args, "name")?.to_owned();
                let host = self.host.clone();
                tokio::task::spawn_blocking(move || host.open_app(&query).map(|name| json!({"opened": name})))
                    .await.map_err(|error| error.to_string())?
            }
            "open_url" | "web_search" => {
                let url = if name == "open_url" {
                    reqwest::Url::parse(required_text(args, "url")?).map_err(|error| error.to_string())?
                } else {
                    let mut url = reqwest::Url::parse("https://www.bing.com/search").map_err(|error| error.to_string())?;
                    url.query_pairs_mut().append_pair("q", required_text(args, "query")?);
                    url
                };
                if !matches!(url.scheme(), "http" | "https") { return Err("Only HTTP and HTTPS URLs may be opened".into()); }
                let host = self.host.clone();
                tokio::task::spawn_blocking(move || host.open_url(url.as_str()).map(|()| json!({"opened": url.as_str()})))
                    .await.map_err(|error| error.to_string())?
            }
            "media_control" => {
                let action = required_text(args, "action")?.to_owned();
                let percent = args.get("percent").and_then(Value::as_u64);
                if args.get("percent").is_some() && percent.is_none_or(|value| value > 100) {
                    return Err("Volume percent must be an integer from 0 to 100".into());
                }
                let host = self.host.clone();
                tokio::task::spawn_blocking(move || host.media_control(&action, percent).map(|()| json!({"action": action, "percent": percent})))
                    .await.map_err(|error| error.to_string())?
            }
            "list_plugins" => self.plugins.clone().ok_or_else(|| "The plugin catalog is unavailable".into()),
            "run_plugin_command" => self.request(json!({"request": "plugin_command", "command": required_text(args, "command")?}), Duration::from_secs(10)).await,
            _ => {
                let tool = self.plugin_routes.get(name).ok_or_else(|| format!("Unknown tool: {name}"))?;
                self.request(json!({"request": "plugin_call", "tool": tool, "args": args}), Duration::from_secs(30)).await
            }
        }
    }

    async fn request(&self, body: Value, timeout: Duration) -> Result<Value, String> {
        let response = tokio::time::timeout(timeout, self.host.request(body))
            .await
            .map_err(|_| "WinIsland did not respond in time")?
            .map_err(|_| "The WinIsland request was cancelled")?;
        if response.get("ok").and_then(Value::as_bool) == Some(false) {
            Err(response
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("The plugin request failed")
                .into())
        } else {
            Ok(response)
        }
    }
}

fn required_text<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("Missing text field: {field}"))
}
