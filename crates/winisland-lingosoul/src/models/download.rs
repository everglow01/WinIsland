use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::header::{ACCEPT_ENCODING, CONTENT_RANGE, RANGE};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::Model;
use crate::{Event, SharedHost};
use winisland_core::assistant::ModelState;

const ATTEMPTS: u32 = 4;
const READ_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) async fn fetch(
    client: &reqwest::Client,
    model: &Model,
    cache: &Path,
    completed: u64,
    total: u64,
    host: &SharedHost,
    cancel: &CancellationToken,
) -> Result<PathBuf, String> {
    tokio::fs::create_dir_all(cache)
        .await
        .map_err(|error| error.to_string())?;
    let finished = cache.join(&model.asset);
    if verified(&finished, model).await? {
        return Ok(finished);
    }
    let partial = cache.join(format!("{}.part", model.asset));
    let base = std::env::var("LINGOSOUL_MODEL_BASE")
        .unwrap_or_else(|_| "https://github.com/k2-fsa/sherpa-onnx/releases/download".into());
    let url = format!(
        "{}/{}/{}",
        base.trim_end_matches('/'),
        model.release,
        model.asset
    );
    let mut last_error = String::new();
    for attempt in 1..=ATTEMPTS {
        if cancel.is_cancelled() {
            return Err("Download cancelled".into());
        }
        let length = tokio::fs::metadata(&partial)
            .await
            .map_or(0, |file| file.len());
        if length > model.size {
            truncate(&partial).await?;
        }
        if length == model.size {
            if verified(&partial, model).await? {
                tokio::fs::rename(&partial, &finished)
                    .await
                    .map_err(|error| error.to_string())?;
                return Ok(finished);
            }
            truncate(&partial).await?;
        }
        let progress = TransferProgress {
            model,
            host,
            completed,
            total,
            attempt,
        };
        let result = transfer(client, &url, &partial, &progress, cancel).await;
        if cancel.is_cancelled() {
            return Err("Download cancelled".into());
        }
        match result {
            Ok(()) if verified(&partial, model).await? => {
                if finished.exists() {
                    tokio::fs::remove_file(&finished)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                tokio::fs::rename(&partial, &finished)
                    .await
                    .map_err(|error| error.to_string())?;
                return Ok(finished);
            }
            Ok(()) => {
                last_error = "The downloaded file failed its SHA-256 check".into();
                truncate(&partial).await?;
            }
            Err(error) => last_error = error,
        }
        if attempt < ATTEMPTS {
            tokio::select! {
                _ = cancel.cancelled() => return Err("Download cancelled".into()),
                _ = tokio::time::sleep(Duration::from_secs(u64::from(attempt) * 2)) => {}
            }
        }
    }
    Err(format!(
        "{}: {last_error}. Retry to resume or redownload this file.",
        model.name
    ))
}

struct TransferProgress<'a> {
    model: &'a Model,
    host: &'a SharedHost,
    completed: u64,
    total: u64,
    attempt: u32,
}

impl TransferProgress<'_> {
    fn report(&self, received: u64) {
        self.host.emit(Event::Models(ModelState::Downloading {
            file: self.model.name.clone(),
            received: self.completed + received,
            total: self.total,
            attempt: self.attempt,
        }));
    }
}

async fn transfer(
    client: &reqwest::Client,
    url: &str,
    partial: &Path,
    progress: &TransferProgress<'_>,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let model = progress.model;
    let mut offset = tokio::fs::metadata(partial)
        .await
        .map_or(0, |file| file.len());
    progress.report(offset);
    let mut request = client.get(url).header(ACCEPT_ENCODING, "identity");
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
    }
    let response = tokio::select! {
        _ = cancel.cancelled() => return Err("Download cancelled".into()),
        response = tokio::time::timeout(READ_TIMEOUT, request.send()) => response,
    };
    let mut response = response
        .map_err(|_| "Timed out while connecting to the model server".to_owned())?
        .map_err(|error| error.without_url().to_string())?;
    if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        truncate(partial).await?;
        return Err("The server rejected the saved download range; retrying from the start".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "The model server returned HTTP {}",
            response.status()
        ));
    }
    if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let range = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("bytes "))
            .and_then(|value| value.split_once('/'))
            .and_then(|(range, total)| {
                let (start, end) = range.split_once('-')?;
                Some((
                    start.parse::<u64>().ok()?,
                    end.parse::<u64>().ok()?,
                    total.parse::<u64>().ok()?,
                ))
            });
        if !range.is_some_and(|(start, end, total)| {
            start == offset && end >= start && end < total && total == model.size
        }) {
            truncate(partial).await?;
            return Err("The model server returned an invalid download range".into());
        }
    } else if response.status() == reqwest::StatusCode::OK {
        offset = 0;
    } else {
        return Err(format!(
            "Unexpected download response: HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length != model.size - offset)
    {
        return Err("The model server returned an unexpected file size".into());
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset > 0)
        .truncate(offset == 0)
        .open(partial)
        .await
        .map_err(|error| error.to_string())?;
    let mut received = offset;
    let mut reported = Instant::now();
    let result = async {
        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => return Err("Download cancelled".into()),
                chunk = tokio::time::timeout(READ_TIMEOUT, response.chunk()) => chunk,
            }
            .map_err(|_| "The download stalled; its saved range can be resumed".to_owned())?
            .map_err(|error| format!("Download response interrupted: {}", error.without_url()))?;
            let Some(chunk) = chunk else { break };
            let count = u64::try_from(chunk.len()).map_err(|error| error.to_string())?;
            if count > model.size.saturating_sub(received) {
                file.set_len(0).await.map_err(|error| error.to_string())?;
                received = 0;
                return Err("The downloaded content exceeds the expected model size".into());
            }
            file.write_all(&chunk)
                .await
                .map_err(|error| error.to_string())?;
            received += count;
            if reported.elapsed() >= Duration::from_millis(250) {
                progress.report(received);
                reported = Instant::now();
            }
        }
        Ok::<(), String>(())
    }
    .await;
    file.flush().await.map_err(|error| error.to_string())?;
    file.sync_all().await.map_err(|error| error.to_string())?;
    progress.report(received);
    result?;
    if received != model.size {
        return Err(format!(
            "Incomplete model file: received {received} of {} bytes",
            model.size
        ));
    }
    Ok(())
}

async fn truncate(path: &Path) -> Result<(), String> {
    tokio::fs::File::create(path)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

async fn verified(path: &Path, model: &Model) -> Result<bool, String> {
    if !tokio::fs::metadata(path)
        .await
        .is_ok_and(|file| file.len() == model.size)
    {
        return Ok(false);
    }
    let path = path.to_owned();
    let expected = model.sha256.clone();
    tokio::task::spawn_blocking(move || checksum(&path).map(|hash| hash == expected))
        .await
        .map_err(|error| error.to_string())?
}

pub(super) fn checksum(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex::encode(hash.finalize()))
}
