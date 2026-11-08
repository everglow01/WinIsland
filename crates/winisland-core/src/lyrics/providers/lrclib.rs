use std::sync::Arc;

use serde_json::Value;

use super::{SongQuery, get_json, ranked, url_encode, winisland_ua};
use crate::lyrics::{LyricLine, parse_lyrics};

pub(super) async fn fetch(
    title: &str,
    artist: &str,
    duration_secs: u64,
    fuzzy: bool,
) -> Option<Arc<Vec<LyricLine>>> {
    let query = SongQuery::new(title, artist, duration_secs, fuzzy);
    if duration_secs > 0
        && !artist.trim().is_empty()
        && let Some(lyrics) = fetch_exact(&query, title, artist, duration_secs).await
    {
        return Some(lyrics);
    }
    for term in query.search_terms() {
        let url = format!("https://lrclib.net/api/search?q={}", url_encode(&term));
        let Some(json) = get_json(&url, &winisland_ua()).await else {
            continue;
        };
        let Some(items) = json.as_array() else {
            continue;
        };
        for item in ranked(items, |item| candidate_score(&query, item)) {
            if let Some(lyrics) = parse_candidate(item) {
                return Some(lyrics);
            }
        }
    }
    None
}

async fn fetch_exact(
    query: &SongQuery,
    title: &str,
    artist: &str,
    duration_secs: u64,
) -> Option<Arc<Vec<LyricLine>>> {
    let url = format!(
        "https://lrclib.net/api/get?track_name={}&artist_name={}&duration={}",
        url_encode(title),
        url_encode(artist),
        duration_secs
    );
    let json = get_json(&url, &winisland_ua()).await?;
    candidate_score(query, &json)?;
    parse_candidate(&json)
}

fn candidate_score(query: &SongQuery, item: &Value) -> Option<u16> {
    let title = item.get("trackName")?.as_str()?;
    let artists = item.get("artistName").and_then(Value::as_str).into_iter();
    let duration = item
        .get("duration")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.round() as u64);
    item.get("syncedLyrics")?
        .as_str()
        .filter(|value| !value.trim().is_empty())?;
    query.score(title, artists, duration)
}

fn parse_candidate(item: &Value) -> Option<Arc<Vec<LyricLine>>> {
    let lines = parse_lyrics(item.get("syncedLyrics")?.as_str()?, "");
    (!lines.is_empty()).then(|| Arc::new(lines))
}
