use std::time::Duration;

use serde_json::{Value, json};

use crate::tools::ToolSpec;
use crate::{ApiFormat, LlmSettings};

const MAX_RESPONSE: usize = 4 * 1024 * 1024;

pub(super) struct Completion {
    pub message: Value,
    pub text: String,
    pub calls: Vec<ToolCall>,
}

pub(super) struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Result<Value, String>,
}

pub(super) async fn complete(
    client: &reqwest::Client,
    config: &LlmSettings,
    system: &str,
    messages: &[Value],
    specs: &[ToolSpec],
) -> Result<Completion, String> {
    if config.base_url.trim().is_empty() || config.model.trim().is_empty() {
        return Err(winisland_core::i18n::tr("lingosoul_api_not_configured"));
    }
    let url = endpoint(config)?;
    let mut request = client
        .post(url)
        .timeout(Duration::from_secs(config.timeout_secs.clamp(5, 300)));
    let body = match config.format {
        ApiFormat::Openai => {
            if !config.api_key.is_empty() {
                request = request.bearer_auth(&config.api_key);
            }
            let messages: Vec<Value> =
                std::iter::once(json!({"role": "system", "content": system}))
                    .chain(messages.iter().cloned())
                    .collect();
            let tools: Vec<_> = specs.iter().map(|tool| json!({
                "type": "function",
                "function": {"name": tool.name, "description": tool.description, "parameters": tool.schema},
            })).collect();
            json!({"model": config.model, "messages": messages, "tools": tools, "max_tokens": config.max_tokens.clamp(256, 32768)})
        }
        ApiFormat::Anthropic => {
            request = request.header("anthropic-version", "2023-06-01");
            if !config.api_key.is_empty() {
                request = request.header("x-api-key", &config.api_key);
            }
            let tools: Vec<_> = specs.iter().map(|tool| json!({
                "name": tool.name, "description": tool.description, "input_schema": tool.schema,
            })).collect();
            json!({"model": config.model, "system": system, "messages": messages, "tools": tools, "max_tokens": config.max_tokens.clamp(256, 32768)})
        }
    };
    let mut response = request
        .json(&body)
        .send()
        .await
        .map_err(|error| error.without_url().to_string())?;
    let status = response.status();
    let mut data = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| error.without_url().to_string())?
    {
        if chunk.len() > MAX_RESPONSE.saturating_sub(data.len()) {
            return Err("The AI response exceeds the size limit".into());
        }
        data.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&data)
        .map_err(|_| format!("The API returned a non-JSON response (HTTP {status})"))?;
    if !status.is_success() || value.get("error").is_some() {
        let message = value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("The API rejected the request");
        return Err(format!(
            "HTTP {status}: {}",
            message.chars().take(1000).collect::<String>()
        ));
    }
    match config.format {
        ApiFormat::Openai => parse_openai(&value),
        ApiFormat::Anthropic => parse_anthropic(&value),
    }
}

fn endpoint(config: &LlmSettings) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(config.base_url.trim()).map_err(|error| error.to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("The API URL must use HTTP or HTTPS".into());
    }
    let path = url.path().trim_end_matches('/');
    let path = match config.format {
        ApiFormat::Openai if !path.ends_with("/chat/completions") => {
            format!("{path}/chat/completions")
        }
        ApiFormat::Anthropic if !path.ends_with("/messages") => {
            if path.ends_with("/v1") {
                format!("{path}/messages")
            } else {
                format!("{path}/v1/messages")
            }
        }
        _ => path.to_owned(),
    };
    url.set_path(&path);
    Ok(url)
}

fn required_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("The AI response is missing {field}"))
}

fn parse_openai(value: &Value) -> Result<Completion, String> {
    let message = value
        .pointer("/choices/0/message")
        .ok_or("The AI response has no message")?;
    let mut calls = Vec::new();
    for call in message
        .get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let function = call
            .get("function")
            .ok_or("The AI returned an invalid tool call")?;
        let arguments = match function.get("arguments") {
            Some(Value::String(raw)) => {
                serde_json::from_str(raw).map_err(|error| error.to_string())
            }
            Some(value @ Value::Object(_)) => Ok(value.clone()),
            _ => Err("Tool arguments must be a JSON object".into()),
        };
        calls.push(ToolCall {
            id: required_string(call, "id")?,
            name: required_string(function, "name")?,
            arguments,
        });
    }
    let text = message
        .get("content")
        .and_then(Value::as_str)
        .or_else(|| message.get("refusal").and_then(Value::as_str))
        .unwrap_or_default()
        .trim()
        .to_owned();
    if calls.is_empty() && text.is_empty() {
        return Err("The AI returned an empty reply".into());
    }
    let mut message = message.clone();
    message["role"] = "assistant".into();
    Ok(Completion {
        message,
        text,
        calls,
    })
}

fn parse_anthropic(value: &Value) -> Result<Completion, String> {
    let content = value
        .get("content")
        .and_then(Value::as_array)
        .ok_or("The AI response has no content")?;
    let mut calls = Vec::new();
    let mut text = String::new();
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => text.push_str(
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            Some("tool_use") => calls.push(ToolCall {
                id: required_string(block, "id")?,
                name: required_string(block, "name")?,
                arguments: block
                    .get("input")
                    .filter(|value| value.is_object())
                    .cloned()
                    .ok_or_else(|| "Tool arguments must be a JSON object".into()),
            }),
            _ => {}
        }
    }
    if calls.is_empty() && text.trim().is_empty() {
        return Err("The AI returned an empty reply".into());
    }
    Ok(Completion {
        message: json!({"role": "assistant", "content": content}),
        text: text.trim().into(),
        calls,
    })
}
