use std::ops::{Div, Rem};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::application::{Colors, LyricsLine, LyricsSegment, RGB};
use crate::lyrics_providers::{LyricsError, LyricsProvider, LyricsResult, TrackQuery};
use crate::utils::time::duration_ms_str;

pub const DEFAULT_LYRIX_BASE_URL: &str = "https://lyrix.vercel.app";

#[derive(Serialize, Deserialize, Debug)]
pub struct LyrixColors {
    pub background: i64,
    pub text: i64,
    #[serde(rename = "highlightText")]
    pub highlight_text: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LyrixLine {
    #[serde(rename = "startTimeMs", deserialize_with = "duration_ms_str")]
    pub start_time: Duration,
    pub words: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LyrixLyrics {
    #[serde(rename = "syncType")]
    pub sync_type: String,
    pub lines: Vec<LyrixLine>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LyrixResponse {
    pub lyrics: LyrixLyrics,
    pub colors: LyrixColors,
}

#[derive(Clone, Debug)]
pub struct LyrixSource {
    base_url: String,
    enabled: bool,
    client: reqwest::Client,
}

impl LyrixSource {
    pub fn new(base_url: String, enabled: bool) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base_url,
            enabled,
            client,
        }
    }

    pub fn from_env() -> Self {
        let base_url = std::env::var("LYRIX_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_LYRIX_BASE_URL.to_string());
        // Disabled by default per requirements
        let enabled = std::env::var("ENABLE_LYRIX_SOURCE")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);
        Self::new(base_url, enabled)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[async_trait]
impl LyricsProvider for LyrixSource {
    fn name(&self) -> &'static str {
        "Lyrix"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    async fn fetch_lyrics(&self, query: &TrackQuery) -> Result<LyricsResult, LyricsError> {
        let track_id = query
            .track_id
            .as_deref()
            .ok_or_else(|| LyricsError::NotFound {
                provider: "Lyrix".into(),
                message: "No Spotify track ID available for Lyrix provider".into(),
            })?;

        let url = format!("{}/getLyrics/{}", self.base_url, track_id);
        let res = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| LyricsError::Unexpected {
                provider: "Lyrix".into(),
                message: format!("Request failed: {e}"),
            })?;

        let status = res.status();
        if !status.is_success() {
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(LyricsError::NotFound {
                    provider: "Lyrix".into(),
                    message: format!("Track ID '{track_id}' not found on Lyrix"),
                });
            }
            return Err(LyricsError::Unexpected {
                provider: "Lyrix".into(),
                message: format!("HTTP error {}", status),
            });
        }

        let data: LyrixResponse = res.json().await.map_err(|e| LyricsError::Unexpected {
            provider: "Lyrix".into(),
            message: format!("Failed to parse response: {e}"),
        })?;

        let lyrics = data
            .lyrics
            .lines
            .into_iter()
            .map(|l| LyricsLine {
                start: l.start_time,
                segments: vec![LyricsSegment {
                    start: l.start_time,
                    text: l.words,
                }],
            })
            .collect();

        let colors = Some(Colors {
            background: parse_color(data.colors.background),
            text: parse_color(data.colors.text),
            highlight: parse_color(data.colors.highlight_text),
        });

        Ok(LyricsResult {
            lyrics,
            colors,
            source_name: "Lyrix".to_string(),
        })
    }
}

fn parse_color(color: i64) -> RGB {
    let hex = 16777216 + color;
    RGB(
        hex.div(256 * 256) as u8,
        hex.div(256).rem(256) as u8,
        hex.rem(256) as u8,
    )
}
