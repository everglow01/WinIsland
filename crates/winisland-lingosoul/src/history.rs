use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use winisland_core::assistant::ChatMessage;

const MAX_MESSAGES: usize = 500;

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Record {
    pub role: Role,
    pub text: String,
    pub at: u64,
    #[serde(default)]
    pub via: String,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Role {
    User,
    Assistant,
}

pub(crate) struct History {
    path: PathBuf,
    messages: VecDeque<Record>,
    retention_days: u64,
}

impl History {
    pub fn load(path: PathBuf) -> Self {
        let messages = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let mut history = Self {
            path,
            messages,
            retention_days: 0,
        };
        history.trim();
        history
    }

    pub fn records(&self) -> impl DoubleEndedIterator<Item = &Record> {
        self.messages.iter()
    }

    pub fn snapshot(&self) -> Vec<ChatMessage> {
        self.messages
            .iter()
            .map(|message| ChatMessage {
                from_user: matches!(message.role, Role::User),
                text: Arc::from(message.text.as_str()),
                at_ms: message.at,
            })
            .collect()
    }

    pub fn push(&mut self, role: Role, text: String, via: &str) -> Result<(), String> {
        self.messages.push_back(Record {
            role,
            text,
            at: now_ms(),
            via: via.into(),
        });
        self.trim();
        self.save()
    }

    pub fn clear(&mut self) -> Result<(), String> {
        self.messages.clear();
        self.save()
    }

    pub fn set_retention(&mut self, days: u64) -> Result<(), String> {
        self.retention_days = days;
        self.trim();
        self.save()
    }

    fn trim(&mut self) {
        if self.retention_days > 0 {
            let oldest = now_ms().saturating_sub(self.retention_days.saturating_mul(86_400_000));
            self.messages.retain(|message| message.at >= oldest);
        }
        while self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
        }
    }

    fn save(&self) -> Result<(), String> {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let parent = self
                .path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(std::path::Path::new("."));
            std::fs::create_dir_all(parent)?;
            let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
            let mut file = std::io::BufWriter::new(&mut temporary);
            for message in &self.messages {
                serde_json::to_writer(&mut file, message)?;
                file.write_all(b"\n")?;
            }
            file.flush()?;
            drop(file);
            temporary.as_file().sync_all()?;
            temporary.persist(&self.path).map_err(|error| error.error)?;
            Ok(())
        })();
        result.map_err(|error| error.to_string())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| {
            u64::try_from(time.as_millis()).unwrap_or(u64::MAX)
        })
}
