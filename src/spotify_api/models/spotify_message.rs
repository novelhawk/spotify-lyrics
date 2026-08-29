use std::time::Duration;

use chrono::{serde::ts_milliseconds, DateTime, Utc};
use serde::{Deserialize, Deserializer};
use serde_json::{from_value, Value};

use crate::utils::time::duration_ms;

use super::playback_updated_event::PlayerUpdatedEvent;

#[derive(Deserialize, Debug)]
pub struct PlayerStateChangedArtist {
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub struct PlayerStateChangedItem {
    pub artists: Vec<PlayerStateChangedArtist>,
    pub name: String,
    #[serde(rename = "duration_ms", deserialize_with = "duration_ms")]
    pub duration: Duration,
    pub id: String,
}

#[derive(Deserialize, Debug)]
pub struct PlayerStateChangedState {
    #[serde(with = "ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "progress_ms", deserialize_with = "duration_ms")]
    pub progress: Duration,
    pub item: PlayerStateChangedItem,
    pub is_playing: bool,
}

#[derive(Deserialize)]
struct Event {
    source: String,
    #[serde(rename = "type")]
    t: String,
    href: String,
    event: Value,
}

#[derive(Deserialize)]
struct Payload {
    events: Vec<Event>,
}

#[derive(Debug)]
pub enum SpotifyMessage {
    Connect(String),
    PlayerStateChanged(PlayerUpdatedEvent),
}

impl<'de> Deserialize<'de> for SpotifyMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        if let Some(uri) = value.get("uri").and_then(|v| v.as_str()) {
            if uri == "wss://event" {
                let state = value
                    .get("payloads")
                    .and_then(|v| v.get(0))
                    .and_then(|v| v.get("events"))
                    .and_then(|v| v.get(0))
                    .and_then(|v| v.get("event"))
                    .and_then(|v| v.get("state"))
                    .ok_or(serde::de::Error::custom("Missing state"))?;

                let event: PlayerStateChangedState =
                    from_value(state.clone()).map_err(serde::de::Error::custom)?;

                return Ok(SpotifyMessage::PlayerStateChanged(PlayerUpdatedEvent {
                    spotify_track_id: event.item.id,
                    song: event.item.name,
                    artist: event.item.artists.into_iter().next().map(|a| a.name),
                    duration: event.item.duration,
                    timestamp: Utc::now(),
                    progress: event.progress,
                    paused: !event.is_playing,
                }));
            } else if uri.starts_with("hm://pusher/v1/connections/") {
                let headers = value
                    .get("headers")
                    .ok_or(serde::de::Error::custom("Missing headers"))?;

                let conn_id = headers
                    .get("Spotify-Connection-Id")
                    .and_then(|v| v.as_str())
                    .ok_or(serde::de::Error::custom("Missing connection id"))?;

                return Ok(SpotifyMessage::Connect(conn_id.to_string()));
            }
        }

        Err(serde::de::Error::custom("Failed to parse"))
    }
}
