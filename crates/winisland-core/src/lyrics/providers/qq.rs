use std::sync::Arc;

use serde_json::Value;

use super::{MOZILLA_UA, SongQuery, get_json_with_referer, ranked, url_encode};
use crate::lyrics::{LyricLine, parse_lyrics};

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
    let search_url = format!(
        "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?format=json&p=1&n=20&w={}",
        url_encode(term)
    );
    let search_json = get_json_with_referer(&search_url, MOZILLA_UA, "https://y.qq.com/").await?;
    let songs = search_json
        .get("data")?
        .get("song")?
        .get("list")?
        .as_array()?;
    let candidates = ranked(songs, |song| {
        let title = song.get("songname")?.as_str()?;
        let artists = song
            .get("singer")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|singer| singer.get("name")?.as_str());
        query.score(title, artists, song.get("interval").and_then(Value::as_u64))
    });
    for song in candidates {
        if let Some(mid) = song.get("songmid").and_then(Value::as_str)
            && let Some(lyrics) = fetch_song(mid).await
        {
            return Some(lyrics);
        }
    }
    None
}

async fn fetch_song(song_mid: &str) -> Option<Arc<Vec<LyricLine>>> {
    let lyric_url = format!(
        "https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg?songmid={song_mid}&format=json&nobase64=1&g_tk=5381"
    );
    let lyric_json = get_json_with_referer(&lyric_url, MOZILLA_UA, "https://y.qq.com/").await?;
    if lyric_json.get("retcode").and_then(Value::as_i64) != Some(0) {
        return None;
    }
    let lrc = lyric_json.get("lyric")?.as_str()?;
    let translated_lrc = lyric_json
        .get("trans")
        .and_then(Value::as_str)
        .unwrap_or("");
    let lines = parse_lyrics(lrc, translated_lrc);
    (!lines.is_empty()).then(|| Arc::new(lines))
}
