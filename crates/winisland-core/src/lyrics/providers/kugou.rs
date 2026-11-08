use std::sync::Arc;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;

use super::{MOZILLA_UA, SongQuery, get_json, ranked, url_encode};
use crate::lyrics::{LyricLine, parse_lyrics};

pub(super) async fn fetch(
    title: &str,
    artist: &str,
    duration_secs: u64,
    fuzzy: bool,
) -> Option<Arc<Vec<LyricLine>>> {
    let query = SongQuery::new(title, artist, duration_secs, fuzzy);
    for term in query.search_terms() {
        let search_url = format!(
            "https://lyrics.kugou.com/search?ver=1&man=yes&client=pc&keyword={}&duration={}",
            url_encode(&term),
            duration_secs.saturating_mul(1000)
        );
        if let Some(json) = get_json(&search_url, MOZILLA_UA).await
            && let Some(lyrics) = fetch_candidates(&query, &json).await
        {
            return Some(lyrics);
        }
    }
    fetch_by_song_hash(&query, title).await
}

async fn fetch_candidates(query: &SongQuery, json: &Value) -> Option<Arc<Vec<LyricLine>>> {
    let candidates = json.get("candidates")?.as_array()?;
    for candidate in ranked(candidates, |item| {
        query.score(
            item.get("song")?.as_str()?,
            item.get("singer").and_then(Value::as_str),
            None,
        )
    }) {
        if let Some(lyrics) = download(candidate).await {
            return Some(lyrics);
        }
    }
    None
}

async fn fetch_by_song_hash(query: &SongQuery, title: &str) -> Option<Arc<Vec<LyricLine>>> {
    let url = format!(
        "https://songsearch.kugou.com/song_search_v2?keyword={}&page=1&pagesize=20&platform=WebFilter&filter=2&iscorrection=1&privilege_filter=0",
        url_encode(title)
    );
    let json = get_json(&url, MOZILLA_UA).await?;
    let songs = json.get("data")?.get("lists")?.as_array()?;
    for song in ranked(songs, |song| {
        query.score(
            song.get("SongName")?.as_str()?,
            song.get("SingerName").and_then(Value::as_str),
            None,
        )
    }) {
        let Some(hash) = song.get("FileHash").and_then(Value::as_str) else {
            continue;
        };
        let url = format!("https://lyrics.kugou.com/search?ver=1&man=yes&client=pc&hash={hash}");
        if let Some(json) = get_json(&url, MOZILLA_UA).await
            && let Some(lyrics) = fetch_candidates(query, &json).await
        {
            return Some(lyrics);
        }
    }
    None
}

async fn download(candidate: &Value) -> Option<Arc<Vec<LyricLine>>> {
    let id = candidate.get("id")?.as_str()?;
    let access_key = candidate.get("accesskey")?.as_str()?;
    let download_url = format!(
        "https://lyrics.kugou.com/download?ver=1&client=pc&id={id}&accesskey={access_key}&fmt=lrc&charset=utf8"
    );
    let download_json = get_json(&download_url, MOZILLA_UA).await?;
    let content = download_json.get("content")?.as_str()?;
    let decoded = STANDARD.decode(content).ok()?;
    let lrc = std::str::from_utf8(&decoded).ok()?;
    let lines = parse_lyrics(lrc, "");
    (!lines.is_empty()).then(|| Arc::new(lines))
}
