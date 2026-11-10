use std::time::{Duration, Instant};

use crate::platform::WindowRef;
use crate::ui::expanded::pager::ExpandedPage;

use super::super::App;

const COVER_WAIT: Duration = Duration::from_millis(2500);
const LYRIC_TRANSITION_STEP: f32 = 0.05;
const LYRIC_TRANSITION_LEAD_MS: u64 = (1000.0 / (60.0 * LYRIC_TRANSITION_STEP as f64)) as u64;

impl App {
    pub(super) fn poll_media_info(&mut self, window: &WindowRef) -> (bool, bool) {
        self.smtc
            .set_enabled(self.config.smtc_enabled && crate::platform::capabilities().media_session);
        if let Some(media) = self.smtc.take_info_if_changed() {
            self.smtc_media_info = media;
        }
        self.audio.set_target_app_id(self.audio_target_app_id());
        let music_active = self.media_active();
        let media_is_playing = music_active && self.current_media_info().is_playing;
        let music_became_available = music_active && !self.music_page_available;
        self.set_music_page_available(music_active);
        if music_became_available && self.expanded {
            self.current_page = ExpandedPage::Music;
        }
        let media = self.current_media_info();
        let track_changed = music_active && media.title != self.last_media_title;
        let cover_hash = if media.thumbnail.is_some() {
            media.thumbnail_hash
        } else {
            0
        };
        if track_changed {
            log::info!(
                "Track changed: {} - {} / {}",
                media.title,
                media.artist,
                media.album
            );
            self.last_media_title = media.title.clone();
            window.request_redraw();
        }
        if !music_active {
            self.audio.set_gate_override(false);
            self.cover_hash = 0;
            self.cover_seen = false;
            self.cover_wait = None;
            crate::ui::expanded::music_view::set_cover_hold(false);
            if !self.last_media_title.is_empty() {
                self.last_media_title.clear();
                crate::ui::expanded::music_view::clear_cover_cache();
                crate::utils::backdrop::clear_blurred_cover_cache();
            }
        }
        if music_active && cover_hash != 0 {
            if cover_hash != self.cover_hash {
                if self.cover_seen {
                    crate::ui::expanded::music_view::trigger_cover_flip();
                }
                self.cover_hash = cover_hash;
                self.cover_seen = true;
                window.request_redraw();
            }
            self.cover_wait = None;
            crate::ui::expanded::music_view::set_cover_hold(false);
        } else if track_changed && self.cover_seen {
            self.cover_wait = Some(Instant::now() + COVER_WAIT);
            crate::ui::expanded::music_view::set_cover_hold(true);
        } else if self
            .cover_wait
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.cover_wait = None;
            self.cover_hash = 0;
            crate::ui::expanded::music_view::trigger_cover_flip();
            window.request_redraw();
        }
        (music_active, media_is_playing)
    }

    pub(super) fn update_lyrics(&mut self, window: &WindowRef, music_active: bool, dt: f32) {
        let current_lyric = if music_active {
            let media = self.current_media_info();
            if self.config.show_lyrics
                && media.lyrics.as_ref().is_some_and(|lines| !lines.is_empty())
            {
                media
                    .current_lyric(
                        (self.config.lyrics_delay * 1000.0) as i64,
                        LYRIC_TRANSITION_LEAD_MS,
                    )
                    .map(|lyric| {
                        (
                            lyric.text,
                            lyric.secondary_text.unwrap_or_default(),
                            lyric.font_family,
                            lyric.highlight,
                            lyric.started,
                        )
                    })
            } else if self.config.show_title_when_no_lyrics && !media.title.trim().is_empty() {
                Some((media.title.as_str(), "", None, None, false))
            } else {
                None
            }
        } else {
            None
        };
        if let Some((lyric, secondary_lyric, font_family, highlight, started)) = current_lyric {
            if lyric != self.lyrics.current_text
                || secondary_lyric != self.lyrics.current_secondary_text
                || font_family != self.lyrics.current_font_family.as_deref()
            {
                self.lyrics.transition_to(
                    lyric.to_owned(),
                    secondary_lyric.to_owned(),
                    font_family.map(str::to_owned),
                    highlight,
                    started,
                    self.config.lyrics_transition_animation,
                );
                window.request_redraw();
            } else if highlight != self.lyrics.highlight {
                self.lyrics.highlight = highlight;
                window.request_redraw();
            }
        } else if !self.lyrics.current_text.is_empty() {
            self.lyrics.transition_to(
                String::new(),
                String::new(),
                None,
                None,
                false,
                self.config.lyrics_transition_animation,
            );
        }

        if self.lyrics.transition < 1.0 {
            self.lyrics.transition += LYRIC_TRANSITION_STEP * dt;
            if self.lyrics.transition > 1.0 {
                self.lyrics.transition = 1.0;
            }
            window.request_redraw();
        }
        if self.lyrics.transition >= 1.0 && !self.lyrics.old_text.is_empty() {
            self.lyrics.old_text = String::new();
            self.lyrics.old_font_family = None;
            self.lyrics.old_secondary_text = String::new();
        }
    }
}
