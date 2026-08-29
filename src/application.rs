use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::lyrics_providers::{LyricsError, LyricsResult};
use crate::spotify_api::models::spotify_message::SpotifyMessage;

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

#[derive(Clone, Debug)]
pub struct Alert {
    pub message: String,
    pub created_at: Instant,
    pub duration: Duration,
}

impl Alert {
    pub fn new(message: impl Into<String>, duration: Duration) -> Self {
        Self {
            message: message.into(),
            created_at: Instant::now(),
            duration,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }
}

#[derive(Default, Clone, Debug)]
pub struct ApplicationStatus {
    pub playback: Option<PlaybackStatus>,
    pub song: Option<Song>,
    pub lyrics: Vec<LyricsLine>,
    pub colors: Option<Colors>,
    pub alert: Option<Alert>,
}

pub enum ApplicationEvent {
    Spotify(SpotifyMessage),
    LyricsFetched {
        track_id: String,
        result: Result<LyricsResult, LyricsError>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_expiration() {
        let alert = Alert::new("Test message", Duration::from_millis(50));
        assert!(!alert.is_expired());
        std::thread::sleep(Duration::from_millis(60));
        assert!(alert.is_expired());
    }
}
