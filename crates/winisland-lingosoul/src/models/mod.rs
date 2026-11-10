mod download;

use std::collections::BTreeMap;
use std::path::{Component, Path};

use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use winisland_core::assistant::ModelState;

use crate::{Event, SharedHost};

static DOWNLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Deserialize)]
struct Model {
    name: String,
    target: String,
    asset: String,
    release: String,
    size: u64,
    sha256: String,
    files: BTreeMap<String, u64>,
}

fn catalog() -> Result<Vec<Model>, String> {
    serde_json::from_str(include_str!("../../resources/models.json"))
        .map_err(|error| error.to_string())
}

fn present(root: &Path, model: &Model) -> bool {
    let base = if model.asset.ends_with(".tar.bz2") {
        root.join(&model.target)
    } else {
        root.to_owned()
    };
    model.files.iter().all(|(file, size)| {
        std::fs::metadata(base.join(file))
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() == *size)
    })
}

pub(crate) fn ready(root: &Path) -> bool {
    catalog().is_ok_and(|models| models.iter().all(|model| present(root, model)))
}

pub(crate) async fn ensure(
    root: std::path::PathBuf,
    client: reqwest::Client,
    host: SharedHost,
    cancel: CancellationToken,
    repair: bool,
) -> Result<(), String> {
    host.emit(Event::Models(ModelState::Installing));
    let _download = tokio::select! {
        _ = cancel.cancelled() => return Err("Download cancelled".into()),
        guard = DOWNLOAD_LOCK.lock() => guard,
    };
    let models = catalog()?;
    let total = models.iter().map(|model| model.size).sum();
    let mut completed = 0;
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|error| error.to_string())?;
    for model in models {
        if cancel.is_cancelled() {
            return Err("Download cancelled".into());
        }
        if !repair && present(&root, &model) {
            completed += model.size;
            continue;
        }
        let archive = download::fetch(
            &client,
            &model,
            &root.join(".downloads"),
            completed,
            total,
            &host,
            &cancel,
        )
        .await?;
        host.emit(Event::Models(ModelState::Installing));
        let destination = root.clone();
        let worker_cancel = cancel.clone();
        let size = model.size;
        tokio::task::spawn_blocking(move || {
            install(&destination, &archive, &model, &worker_cancel)
        })
        .await
        .map_err(|error| error.to_string())??;
        completed += size;
    }
    host.emit(Event::Models(ModelState::Ready));
    Ok(())
}

fn install(
    root: &Path,
    archive: &Path,
    model: &Model,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let staging = tempfile::Builder::new()
        .prefix(".lingosoul-install-")
        .tempdir_in(root)
        .map_err(|error| error.to_string())?;
    if model.asset.ends_with(".tar.bz2") {
        let file = std::fs::File::open(archive).map_err(|error| error.to_string())?;
        let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(file));
        let mut expanded = 0_u64;
        for entry in tar
            .entries()
            .map_err(|error| format!("Cannot decode model archive: {error}"))?
        {
            if cancel.is_cancelled() {
                return Err("Download cancelled".into());
            }
            let mut entry =
                entry.map_err(|error| format!("Cannot decode model archive: {error}"))?;
            let path = entry.path().map_err(|error| error.to_string())?;
            let normalized: std::path::PathBuf = path
                .components()
                .filter_map(|part| match part {
                    Component::Normal(name) => Some(name),
                    _ => None,
                })
                .collect();
            if path
                .components()
                .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
                || !normalized.starts_with(&model.target)
                || !(entry.header().entry_type().is_file() || entry.header().entry_type().is_dir())
            {
                return Err("The model archive contains an unsupported entry".into());
            }
            expanded = expanded
                .checked_add(entry.size())
                .ok_or("Model archive size overflow")?;
            if expanded > 1024 * 1024 * 1024 {
                return Err("The model archive exceeds its extraction limit".into());
            }
            if !entry
                .unpack_in(staging.path())
                .map_err(|error| format!("Cannot extract model: {error}"))?
            {
                return Err("A model archive entry escaped its destination".into());
            }
        }
    } else {
        std::fs::copy(archive, staging.path().join(&model.target))
            .map_err(|error| error.to_string())?;
    }
    if !present(staging.path(), model) {
        return Err("The model archive is missing required files".into());
    }
    if cancel.is_cancelled() {
        return Err("Download cancelled".into());
    }
    let destination = root.join(&model.target);
    let backup = tempfile::Builder::new()
        .prefix(".lingosoul-backup-")
        .tempdir_in(root)
        .map_err(|error| error.to_string())?;
    let previous = backup.path().join("previous");
    let replacing = destination.exists();
    if replacing {
        std::fs::rename(&destination, &previous).map_err(|error| error.to_string())?;
    }
    if let Err(error) = std::fs::rename(staging.path().join(&model.target), &destination) {
        if replacing && let Err(restore) = std::fs::rename(&previous, &destination) {
            let preserved = backup.keep();
            return Err(format!(
                "Cannot install model: {error}; cannot restore previous model: {restore}. The previous model is preserved in {}",
                preserved.display()
            ));
        }
        return Err(format!("Cannot install model: {error}"));
    }
    if let Err(error) = backup.close() {
        log::warn!("Cannot remove the previous speech model: {error}");
    }
    staging.close().map_err(|error| error.to_string())
}
