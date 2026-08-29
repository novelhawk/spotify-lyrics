use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::lyrics_providers::lyrix::{LyrixColors as Colors, LyrixLine};

pub struct LyricsContext {
    pub colors: Colors,
    pub lines: Vec<LyrixLine>,
    pub song_position: Duration,
    pub last_update: DateTime<Utc>,
    pub paused: bool,
}

impl LyricsContext {
    pub fn current_line(&self) -> Option<usize> {
        self.lines
            .iter()
            .position(|it| it.start_time >= self.song_time())
    }

    pub fn song_time(&self) -> Duration {
        match self.paused {
            true => self.song_position,
            false => {
                let delta = Utc::now() - self.last_update;
                self.song_position + delta.to_std().unwrap_or(Duration::ZERO)
            }
        }
    }
}
