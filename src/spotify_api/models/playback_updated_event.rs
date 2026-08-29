use std::time::Duration;

use chrono::{DateTime, Utc};

#[derive(Debug)]
pub struct PlayerUpdatedEvent {
    pub spotify_track_id: String,
    pub song: String,
    pub artist: Option<String>,
    pub duration: Duration,
    pub timestamp: DateTime<Utc>,
    pub progress: Duration,
    pub paused: bool,
}
