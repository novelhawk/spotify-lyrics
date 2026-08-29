use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::{spotify_api::models::spotify_message::SpotifyMessage, utils::time::print_hhmm};

#[derive(Clone, Debug)]
pub struct PlaybackStatus {
    pub paused: bool,
    pub position: Duration,
    pub duration: Duration,
    pub last_update: DateTime<Utc>,
}

impl PlaybackStatus {
    pub fn position(&self) -> Duration {
        match self.paused {
            true => self.position,
            false => {
                let elapsed = (Utc::now() - self.last_update)
                    .to_std()
                    .unwrap_or(Duration::ZERO);
                self.position + elapsed
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Song {
    pub spotify_track_id: Option<String>,
    pub author: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct LyricsSegment {
    pub start: Duration,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct LyricsLine {
    pub start: Duration,
    pub segments: Vec<LyricsSegment>,
}

#[derive(Clone, Debug)]
pub struct RGB(pub u8, pub u8, pub u8);

#[derive(Clone, Debug)]
pub struct Colors {
    pub background: RGB,
    pub text: RGB,
    pub highlight: RGB,
}

#[derive(Default, Clone, Debug)]
pub struct ApplicationStatus {
    pub playback: Option<PlaybackStatus>,
    pub song: Option<Song>,
    pub lyrics: Vec<LyricsLine>,
    pub colors: Option<Colors>,
}

pub enum ApplicationEvent {
    Spotify(SpotifyMessage),
}

async fn event_listener() {}
