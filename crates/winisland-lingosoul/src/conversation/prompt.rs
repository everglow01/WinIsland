use std::io::Write;
use std::path::Path;

use crate::{Paths, Settings, SharedHost};

use super::Via;

const DEFAULTS: [(&str, &str); 4] = [
    (
        "system.txt",
        include_str!("../../resources/prompts/system.txt"),
    ),
    (
        "voice.txt",
        include_str!("../../resources/prompts/voice.txt"),
    ),
    ("text.txt", include_str!("../../resources/prompts/text.txt")),
    (
        "panel_voice.txt",
        include_str!("../../resources/prompts/panel_voice.txt"),
    ),
];

pub(super) fn ensure(directory: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    for (name, content) in DEFAULTS {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))
        {
            Ok(mut file) => file
                .write_all(content.as_bytes())
                .map_err(|error| error.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

pub(super) fn render(
    settings: &Settings,
    paths: &Paths,
    host: &SharedHost,
    via: Via,
) -> Result<String, String> {
    let directory = paths.prompts();
    let path = if settings.llm.prompt_file.as_os_str().is_empty() {
        directory.join("system.txt")
    } else {
        settings.llm.prompt_file.clone()
    };
    let template = std::fs::read_to_string(&path)
        .map_err(|error| format!("Cannot read system prompt {}: {error}", path.display()))?;
    if template.trim().is_empty() {
        return Err("The system prompt file is empty".into());
    }
    let style = std::fs::read_to_string(directory.join(match via {
        Via::Voice => "voice.txt",
        Via::Text => "text.txt",
        Via::PanelVoice => "panel_voice.txt",
    }))
    .map_err(|error| error.to_string())?;
    let language = if host.language().starts_with("zh") {
        "Simplified Chinese"
    } else {
        "English"
    };
    let permissions =
        serde_json::to_string_pretty(&settings.local_agent).map_err(|error| error.to_string())?;
    Ok(template
        .replace("{name}", &settings.assistant.name)
        .replace("{language}", language)
        .replace("{time}", &host.local_time())
        .replace("{style}", &style)
        .replace("{permissions}", &permissions))
}
