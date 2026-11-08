use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::sync::CancellationToken;

use crate::AgentSettings;

const FILE_LIMIT: u64 = 1024 * 1024;
const OUTPUT_LIMIT: usize = 64 * 1024;
const PENDING_LIMIT: usize = 16;

pub(crate) struct AgentTools {
    settings: AgentSettings,
    files: Arc<Mutex<FileTools>>,
}

struct FileTools {
    settings: AgentSettings,
    next_change: u64,
    pending: HashMap<String, Change>,
}

struct Change {
    sequence: u64,
    path: PathBuf,
    original_hash: String,
    content: String,
    diff: String,
}

impl AgentTools {
    pub fn new(settings: AgentSettings) -> Self {
        Self {
            files: Arc::new(Mutex::new(FileTools {
                settings: settings.clone(),
                next_change: 1,
                pending: HashMap::new(),
            })),
            settings,
        }
    }

    pub fn settings(&self) -> &AgentSettings {
        &self.settings
    }

    pub fn reset(&self) -> Result<(), String> {
        self.files
            .lock()
            .map_err(|_| "File tools are unavailable".to_owned())?
            .pending
            .clear();
        Ok(())
    }

    pub fn available(&self, name: &str) -> bool {
        self.settings.enabled
            && match name {
                "read" => self.settings.allow_read,
                "write" => self.settings.allow_write,
                "update" => self.settings.allow_read,
                "execute" => self.settings.allow_execute,
                _ => false,
            }
    }

    pub async fn call(
        &self,
        name: &str,
        args: &Value,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        if !self.available(name) {
            return Err(format!("Permission denied for tool {name}"));
        }
        if name == "execute" {
            return execute(&self.settings, args, cancel).await;
        }
        let files = Arc::clone(&self.files);
        let name = name.to_owned();
        let args = args.clone();
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            if cancel.is_cancelled() {
                return Err("Operation cancelled".into());
            }
            let mut files = files.lock().map_err(|_| "File tools are unavailable")?;
            match name.as_str() {
                "read" => files.read(&args),
                "write" => files.write(&args, &cancel),
                "update" => files.update(&args, &cancel),
                _ => Err("Unknown file tool".into()),
            }
        })
        .await
        .map_err(|error| error.to_string())?
    }
}

fn text<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing text field: {name}"))
}

fn workspace(settings: &AgentSettings) -> Result<PathBuf, String> {
    if settings.workspace.as_os_str().is_empty() {
        return Err("Choose an agent working directory in LingoSoul settings first".into());
    }
    let path = std::fs::canonicalize(&settings.workspace).map_err(|error| error.to_string())?;
    if !path.is_dir() {
        return Err("The agent working directory is not a directory".into());
    }
    Ok(path)
}

fn requested_path(settings: &AgentSettings, value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.components().any(
            |part| matches!(part, Component::Normal(name) if name.to_string_lossy().contains(':')),
        )
    {
        return Err("Invalid file path".into());
    }
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        if path
            .components()
            .any(|part| matches!(part, Component::Prefix(_) | Component::RootDir))
        {
            return Err("Drive-relative paths are not supported".into());
        }
        Ok(workspace(settings)?.join(path))
    }
}

fn readable(settings: &AgentSettings, value: &str) -> Result<PathBuf, String> {
    if !settings.allow_read {
        return Err("Read permission is disabled".into());
    }
    let path = std::fs::canonicalize(requested_path(settings, value)?)
        .map_err(|error| error.to_string())?;
    let allowed = std::iter::once(&settings.workspace)
        .chain(&settings.read_directories)
        .filter(|root| !root.as_os_str().is_empty())
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .any(|root| root.is_dir() && path.starts_with(root));
    if !allowed {
        return Err("The path is outside the permitted read directories".into());
    }
    Ok(path)
}

fn writable(settings: &AgentSettings, value: &str) -> Result<PathBuf, String> {
    if !settings.allow_write {
        return Err("Write permission is disabled".into());
    }
    let root = workspace(settings)?;
    let requested = requested_path(settings, value)?;
    let mut existing = requested.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        let name = existing.file_name().ok_or("Invalid destination path")?;
        if name == "." || name == ".." {
            return Err("Invalid destination component".into());
        }
        missing.push(name.to_owned());
        existing = existing.parent().ok_or("Invalid destination parent")?;
    }
    let mut path = std::fs::canonicalize(existing).map_err(|error| error.to_string())?;
    for component in missing.into_iter().rev() {
        path.push(component);
    }
    if !path.starts_with(&root) || path == root {
        return Err("Writes are restricted to files inside the working directory".into());
    }
    if path.exists() && !path.is_file() {
        return Err("The destination is not a regular file".into());
    }
    Ok(path)
}

fn read_file(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("The path is not a regular file".into());
    }
    let mut data = Vec::new();
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut data)
        .map_err(|error| error.to_string())?;
    if data.len() as u64 > FILE_LIMIT {
        return Err("The file exceeds the 1 MiB text limit".into());
    }
    String::from_utf8(data).map_err(|_| "The file is not UTF-8 text".into())
}

fn hash(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

impl FileTools {
    fn read(&self, args: &Value) -> Result<Value, String> {
        let path = readable(&self.settings, text(args, "path")?)?;
        if path.is_dir() {
            let mut entries = Vec::new();
            for entry in std::fs::read_dir(&path)
                .map_err(|error| error.to_string())?
                .take(201)
            {
                let entry = entry.map_err(|error| error.to_string())?;
                let kind = entry.file_type().map_err(|error| error.to_string())?;
                entries.push(json!({
                    "name": entry.file_name().to_string_lossy(),
                    "directory": kind.is_dir(),
                    "symlink": kind.is_symlink(),
                }));
            }
            let truncated = entries.len() > 200;
            entries.truncate(200);
            return Ok(json!({"path": path, "entries": entries, "truncated": truncated}));
        }
        let content = read_file(&path)?;
        let start = args
            .get("start_line")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1);
        let count = args
            .get("line_count")
            .and_then(Value::as_u64)
            .unwrap_or(200)
            .clamp(1, 1000);
        let start_index = usize::try_from(start - 1).map_err(|error| error.to_string())?;
        let count = usize::try_from(count).map_err(|error| error.to_string())?;
        let lines: Vec<_> = content
            .lines()
            .skip(start_index)
            .take(count)
            .enumerate()
            .map(|(index, line)| format!("{}: {line}", start_index + index + 1))
            .collect();
        let mut output = lines.join("\n");
        let truncated = output.len() > OUTPUT_LIMIT;
        if truncated {
            let mut boundary = OUTPUT_LIMIT;
            while !output.is_char_boundary(boundary) {
                boundary -= 1;
            }
            output.truncate(boundary);
        }
        Ok(
            json!({"path": path, "sha256": hash(&content), "content": output, "truncated": truncated}),
        )
    }

    fn write(&self, args: &Value, cancel: &CancellationToken) -> Result<Value, String> {
        let path = writable(&self.settings, text(args, "path")?)?;
        let content = text(args, "content")?;
        self.store(
            &path,
            content,
            args.get("expected_sha256").and_then(Value::as_str),
            cancel,
        )?;
        Ok(json!({"path": path, "written_bytes": content.len(), "sha256": hash(content)}))
    }

    fn update(&mut self, args: &Value, cancel: &CancellationToken) -> Result<Value, String> {
        match args
            .get("operation")
            .and_then(Value::as_str)
            .unwrap_or("preview")
        {
            "preview" => self.preview(args),
            "apply" => {
                let id = text(args, "change_id")?;
                let change = self
                    .pending
                    .get(id)
                    .ok_or("The update preview expired; generate a new diff")?;
                let path = writable(&self.settings, &change.path.to_string_lossy())?;
                self.store(&path, &change.content, Some(&change.original_hash), cancel)?;
                let result = json!({"path": path, "applied": true, "diff": change.diff, "sha256": hash(&change.content)});
                self.pending.remove(id);
                Ok(result)
            }
            _ => Err("Update operation must be preview or apply".into()),
        }
    }

    fn preview(&mut self, args: &Value) -> Result<Value, String> {
        let path = readable(&self.settings, text(args, "path")?)?;
        let original = read_file(&path)?;
        let lines: Vec<_> = original.split_inclusive('\n').collect();
        let start = args
            .get("start_line")
            .and_then(Value::as_u64)
            .ok_or("Missing start_line")?;
        let end = args
            .get("end_line")
            .and_then(Value::as_u64)
            .ok_or("Missing end_line")?;
        let start = usize::try_from(start).map_err(|error| error.to_string())?;
        let end = usize::try_from(end).map_err(|error| error.to_string())?;
        if start == 0 || start > lines.len() + 1 || end < start - 1 || end > lines.len() {
            return Err("Invalid line range; use end_line = start_line - 1 to insert".into());
        }
        let newline = if original.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut replacement = text(args, "content")?.replace("\r\n", "\n");
        if newline == "\r\n" {
            replacement = replacement.replace('\n', newline);
        }
        if !replacement.is_empty()
            && !replacement.ends_with('\n')
            && (end < lines.len() || original.ends_with('\n'))
        {
            replacement.push_str(newline);
        }
        let content = format!(
            "{}{}{}",
            lines[..start - 1].concat(),
            replacement,
            lines[end..].concat()
        );
        if content.len() as u64 > FILE_LIMIT {
            return Err("The updated file exceeds the text limit".into());
        }
        let label = path.to_string_lossy();
        let diff = similar::TextDiff::from_lines(&original, &content)
            .unified_diff()
            .context_radius(3)
            .header(&label, &label)
            .to_string();
        if diff.len() > OUTPUT_LIMIT {
            return Err("The diff is too large; split the change into smaller line ranges".into());
        }
        if self.pending.len() >= PENDING_LIMIT
            && let Some(oldest) = self
                .pending
                .iter()
                .min_by_key(|(_, change)| change.sequence)
                .map(|(id, _)| id.clone())
        {
            self.pending.remove(&oldest);
        }
        let sequence = self.next_change;
        self.next_change = self
            .next_change
            .checked_add(1)
            .ok_or("Update identifier exhausted")?;
        let original_hash = hash(&original);
        let mut identity = Sha256::new();
        identity.update(path.to_string_lossy().as_bytes());
        identity.update([0]);
        identity.update(original_hash.as_bytes());
        identity.update([0]);
        identity.update(content.as_bytes());
        let id = hex::encode(&identity.finalize()[..16]);
        let result = json!({"change_id": id, "path": path, "sha256": original_hash, "diff": diff, "applied": false});
        self.pending.insert(
            id,
            Change {
                sequence,
                path,
                original_hash,
                content,
                diff,
            },
        );
        Ok(result)
    }

    fn store(
        &self,
        path: &Path,
        content: &str,
        expected: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        if content.len() as u64 > FILE_LIMIT {
            return Err("The content exceeds the 1 MiB text limit".into());
        }
        if cancel.is_cancelled() {
            return Err("Operation cancelled".into());
        }
        if let Some(expected) = expected
            && hash(&read_file(path)?) != expected
        {
            return Err(
                "The file changed after it was read; generate a new diff before writing".into(),
            );
        }
        let parent = path.parent().ok_or("Invalid destination parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let path = writable(&self.settings, &path.to_string_lossy())?;
        let parent = path.parent().ok_or("Invalid destination parent")?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        temporary
            .write_all(content.as_bytes())
            .map_err(|error| error.to_string())?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        if cancel.is_cancelled() {
            return Err("Operation cancelled".into());
        }
        if let Some(expected) = expected
            && hash(&read_file(&path)?) != expected
        {
            return Err("The file changed before the update could be applied".into());
        }
        temporary
            .persist(&path)
            .map_err(|error| error.error.to_string())?;
        Ok(())
    }
}

async fn collect_output(mut stream: impl AsyncRead + Unpin) -> Result<(String, bool), String> {
    let mut result = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let count = stream
            .read(&mut buffer)
            .await
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        let keep = count.min(OUTPUT_LIMIT.saturating_sub(result.len()));
        result.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
    Ok((String::from_utf8_lossy(&result).into_owned(), truncated))
}

async fn execute(
    settings: &AgentSettings,
    args: &Value,
    cancel: &CancellationToken,
) -> Result<Value, String> {
    if !settings.allow_execute {
        return Err("Execute permission is disabled".into());
    }
    let directory = workspace(settings)?;
    let script = text(args, "command")?;
    if script.len() > 32 * 1024 {
        return Err("The command is too long".into());
    }
    let mut command = tokio::process::Command::new("powershell.exe");
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &format!("$OutputEncoding = [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); {script}"),
    ]);
    command
        .current_dir(&directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut command = process_wrap::tokio::CommandWrap::from(command);
    command.wrap(process_wrap::tokio::KillOnDrop);
    #[cfg(windows)]
    command
        .wrap(process_wrap::tokio::CreationFlags(
            windows::Win32::System::Threading::CREATE_NO_WINDOW,
        ))
        .wrap(process_wrap::tokio::JobObject);
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let stdout = child
        .stdout()
        .take()
        .ok_or("Command stdout is unavailable")?;
    let stderr = child
        .stderr()
        .take()
        .ok_or("Command stderr is unavailable")?;
    let operation = async {
        let (status, output, errors) = tokio::try_join!(
            async { child.wait().await.map_err(|error| error.to_string()) },
            collect_output(stdout),
            collect_output(stderr),
        )?;
        Ok::<_, String>(json!({
            "directory": directory,
            "exit_code": status.code(),
            "success": status.success(),
            "stdout": output.0,
            "stderr": errors.0,
            "truncated": output.1 || errors.1,
        }))
    };
    tokio::select! {
        _ = cancel.cancelled() => Err("Command cancelled".into()),
        result = tokio::time::timeout(Duration::from_secs(settings.command_timeout_secs.clamp(1, 300)), operation) => {
            result.map_err(|_| "Command timed out".to_owned())?
        }
    }
}
