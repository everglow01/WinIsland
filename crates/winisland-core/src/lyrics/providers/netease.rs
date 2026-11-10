use std::collections::BTreeMap;
use std::sync::Arc;

use super::{MOZILLA_UA, SongQuery, get_json, ranked, url_encode};
use crate::lyrics::{LyricLine, LyricTiming, parse_lyrics};

pub(super) async fn fetch(
    title: &str,
    artist: &str,
    duration_secs: u64,
    fuzzy: bool,
) -> Option<Arc<Vec<LyricLine>>> {
    let query = SongQuery::new(title, artist, duration_secs, fuzzy);
    for term in query.search_terms() {
        if let Some(lyrics) = fetch_inner(&query, &term).await {
            return Some(lyrics);
        }
    }
    None
}

async fn fetch_inner(query: &SongQuery, term: &str) -> Option<Arc<Vec<LyricLine>>> {
    let url = format!(
        "https://music.163.com/api/search/get/web?s={}&type=1&offset=0&total=true&limit=20",
        url_encode(term)
    );
    let json = get_json(&url, MOZILLA_UA).await?;
    let songs = json.get("result")?.get("songs")?.as_array()?;
    let candidates = ranked(songs, |song| {
        let title = song.get("name")?.as_str()?;
        if !query.accepts_edition(title) {
            return None;
        }
        let artists = song
            .get("artists")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|artist| artist.get("name")?.as_str());
        let duration = song
            .get("duration")
            .or_else(|| song.get("dt"))
            .and_then(serde_json::Value::as_u64)
            .map(|ms| ms / 1000);
        let artists: Vec<_> = artists.collect();
        std::iter::once(title)
            .chain(
                song.get("alias")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_str),
            )
            .filter_map(|name| query.score(name, artists.iter().copied(), duration))
            .max()
    });
    for song in candidates {
        if let Some(song_id) = song.get("id").and_then(serde_json::Value::as_i64)
            && let Some(lyrics) = fetch_song(song_id).await
        {
            return Some(lyrics);
        }
    }
    None
}

async fn fetch_song(song_id: i64) -> Option<Arc<Vec<LyricLine>>> {
    let lyric_url = format!(
        "https://music.163.com/api/song/lyric?id={}&lv=1&kv=1&tv=-1&yv=1&ytv=1",
        song_id
    );
    let lyric_json = get_json(&lyric_url, MOZILLA_UA).await?;
    let translated_lrc = lyric_json
        .get("ytlrc")
        .or_else(|| lyric_json.get("tlyric"))
        .and_then(|lyrics| lyrics.get("lyric"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if let Some(yrc) = lyric_json
        .get("yrc")
        .and_then(|lyrics| lyrics.get("lyric"))
        .and_then(serde_json::Value::as_str)
    {
        let lines = parse_yrc(yrc, translated_lrc);
        if !lines.is_empty() {
            return Some(Arc::new(lines));
        }
    }

    let lrc = lyric_json.get("lrc")?.get("lyric")?.as_str().unwrap_or("");
    let lines = parse_lyrics(lrc, translated_lrc);
    (!lines.is_empty()).then(|| Arc::new(lines))
}

fn parse_yrc(yrc: &str, translated_lrc: &str) -> Vec<LyricLine> {
    let translations = parse_lyrics(translated_lrc, "")
        .into_iter()
        .map(|line| (line.time_ms, line.text))
        .collect::<BTreeMap<_, _>>();
    yrc.lines()
        .filter_map(parse_yrc_line)
        .map(|mut line| {
            line.secondary_text = translations.get(&line.time_ms).cloned();
            line
        })
        .collect()
}

fn parse_yrc_line(line: &str) -> Option<LyricLine> {
    let line = line.trim_start_matches('\u{feff}').strip_prefix('[')?;
    let header_end = line.find(']')?;
    let time_ms = parse_pair(line.get(..header_end)?)?.0;
    let content = line.get(header_end + 1..)?;

    let mut tags = Vec::new();
    let mut search_from = 0;
    while let Some(relative_start) = content.get(search_from..)?.find('(') {
        let start = search_from + relative_start;
        let Some(relative_end) = content.get(start + 1..)?.find(')') else {
            break;
        };
        let end = start + relative_end + 2;
        if let Some((word_start_ms, word_duration_ms)) =
            parse_pair(content.get(start + 1..end - 1)?)
        {
            tags.push((start, end, word_start_ms, word_duration_ms));
        }
        search_from = end;
    }

    let mut text = String::new();
    let mut timings = Vec::new();
    for (index, &(_, segment_start, start_time_ms, duration_ms)) in tags.iter().enumerate() {
        let segment_end = tags
            .get(index + 1)
            .map_or(content.len(), |(start, _, _, _)| *start);
        let segment = content.get(segment_start..segment_end)?;
        if segment.is_empty() {
            continue;
        }
        text.push_str(segment);
        timings.push(LyricTiming {
            start_time_ms,
            end_time_ms: start_time_ms.checked_add(duration_ms),
            end_byte: text.len(),
        });
    }

    if text.is_empty() || timings.is_empty() {
        return None;
    }
    Some(LyricLine {
        time_ms,
        text,
        secondary_text: None,
        font_family: None,
        timings,
    })
}

fn parse_pair(value: &str) -> Option<(u64, u64)> {
    let mut fields = value.split(',');
    let start = fields.next()?.parse().ok()?;
    let duration = fields.next()?.parse().ok()?;
    Some((start, duration))
}
