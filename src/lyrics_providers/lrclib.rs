use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::Deserialize;

use crate::lyrics_providers::lrc_parser::parse_lrc;
use crate::lyrics_providers::{LyricsError, LyricsProvider, LyricsResult, TrackQuery};

pub const DEFAULT_LRCLIB_BASE_URL: &str = "https://lrclib.net";
pub const DEFAULT_LRCLIB_USER_AGENT: &str =
    "spotify-lyrics/0.1.0 (https://github.com/spotify-lyrics)";

#[derive(Deserialize, Debug)]
pub struct LrclibRecord {
    pub id: Option<u64>,
    #[serde(rename = "trackName")]
    pub track_name: Option<String>,
    #[serde(rename = "artistName")]
    pub artist_name: Option<String>,
    #[serde(rename = "albumName")]
    pub album_name: Option<String>,
    pub duration: Option<f64>,
    pub instrumental: Option<bool>,
    #[serde(rename = "plainLyrics")]
    pub plain_lyrics: Option<String>,
    #[serde(rename = "syncedLyrics")]
    pub synced_lyrics: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LrclibSource {
    base_url: String,
    user_agent: String,
    enabled: bool,
    client: reqwest::Client,
}

impl LrclibSource {
    pub fn new(base_url: String, user_agent: String, enabled: bool) -> Self {
        let mut headers = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(&user_agent) {
            headers.insert(header::USER_AGENT, val);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url,
            user_agent,
            enabled,
            client,
        }
    }

    pub fn from_env() -> Self {
        let base_url = std::env::var("LRCLIB_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_LRCLIB_BASE_URL.to_string());
        let user_agent = std::env::var("LRCLIB_USER_AGENT")
            .unwrap_or_else(|_| DEFAULT_LRCLIB_USER_AGENT.to_string());
        let enabled = std::env::var("ENABLE_LRCLIB_SOURCE")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(true);

        Self::new(base_url, user_agent, enabled)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    async fn fetch_record(&self, query: &TrackQuery) -> Result<LrclibRecord, LyricsError> {
        let mut get_url = format!(
            "{}/api/get?track_name={}",
            self.base_url,
            urlencoding(&query.track_name)
        );

        if let Some(artist) = &query.artist_name {
            get_url.push_str(&format!("&artist_name={}", urlencoding(artist)));
        }
        if let Some(dur) = query.duration {
            get_url.push_str(&format!("&duration={}", dur.as_secs()));
        }

        let res = self
            .client
            .get(&get_url)
            .send()
            .await
            .map_err(|e| LyricsError::Unexpected {
                provider: "LRCLIB".into(),
                message: format!("Network request failed: {e}"),
            })?;

        let status = res.status();
        if status.is_success() {
            let record: LrclibRecord = res.json().await.map_err(|e| LyricsError::Unexpected {
                provider: "LRCLIB".into(),
                message: format!("Failed to parse JSON response: {e}"),
            })?;
            return Ok(record);
        } else if status == reqwest::StatusCode::NOT_FOUND {
            // Try fallback search endpoint: /api/search
            let mut search_url = format!(
                "{}/api/search?track_name={}",
                self.base_url,
                urlencoding(&query.track_name)
            );
            if let Some(artist) = &query.artist_name {
                search_url.push_str(&format!("&artist_name={}", urlencoding(artist)));
            }

            let search_res = self
                .client
                .get(&search_url)
                .send()
                .await
                .map_err(|e| LyricsError::Unexpected {
                    provider: "LRCLIB".into(),
                    message: format!("Search request failed: {e}"),
                })?;

            if search_res.status().is_success() {
                let records: Vec<LrclibRecord> =
                    search_res.json().await.map_err(|e| LyricsError::Unexpected {
                        provider: "LRCLIB".into(),
                        message: format!("Failed to parse search JSON response: {e}"),
                    })?;

                // Find first record with synced lyrics or instrumental
                if let Some(found) = records.into_iter().find(|r| {
                    r.synced_lyrics.as_ref().map_or(false, |s| !s.is_empty())
                        || r.instrumental == Some(true)
                }) {
                    return Ok(found);
                }
            }

            return Err(LyricsError::NotFound {
                provider: "LRCLIB".into(),
                message: format!("Track '{}' not found", query.track_name),
            });
        } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LyricsError::Unexpected {
                provider: "LRCLIB".into(),
                message: "Rate limited (HTTP 429)".into(),
            });
        } else {
            return Err(LyricsError::Unexpected {
                provider: "LRCLIB".into(),
                message: format!("HTTP error {}", status),
            });
        }
    }
}

#[async_trait]
impl LyricsProvider for LrclibSource {
    fn name(&self) -> &'static str {
        "LRCLIB"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    async fn fetch_lyrics(&self, query: &TrackQuery) -> Result<LyricsResult, LyricsError> {
        let record = self.fetch_record(query).await?;

        if let Some(synced) = &record.synced_lyrics {
            let parsed = parse_lrc(synced);
            if !parsed.is_empty() {
                return Ok(LyricsResult {
                    lyrics: parsed,
                    colors: None,
                    source_name: "LRCLIB".to_string(),
                });
            }
        }

        if record.instrumental == Some(true) {
            return Ok(LyricsResult {
                lyrics: vec![crate::application::LyricsLine {
                    start: Duration::ZERO,
                    segments: vec![crate::application::LyricsSegment {
                        start: Duration::ZERO,
                        text: "♫ Instrumental ♫".to_string(),
                    }],
                }],
                colors: None,
                source_name: "LRCLIB".to_string(),
            });
        }

        Err(LyricsError::NotFound {
            provider: "LRCLIB".into(),
            message: format!("No synchronized lyrics available for '{}'", query.track_name),
        })
    }
}

fn urlencoding(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push_str("%20"),
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lrclib_record_deserialization() {
        let json_data = r#"{
            "id": 12345,
            "name": "Bohemian Rhapsody",
            "trackName": "Bohemian Rhapsody",
            "artistName": "Queen",
            "albumName": "A Night at the Opera",
            "duration": 354.0,
            "instrumental": false,
            "plainLyrics": "Is this the real life?",
            "syncedLyrics": "[00:12.43] Thunderbolt and lightning"
        }"#;

        let record: LrclibRecord = serde_json::from_str(json_data).unwrap();
        assert_eq!(record.id, Some(12345));
        assert_eq!(record.track_name.as_deref(), Some("Bohemian Rhapsody"));
        assert_eq!(record.artist_name.as_deref(), Some("Queen"));
        assert_eq!(
            record.synced_lyrics.as_deref(),
            Some("[00:12.43] Thunderbolt and lightning")
        );
    }

    #[test]
    fn test_urlencoding() {
        assert_eq!(urlencoding("Bohemian Rhapsody"), "Bohemian%20Rhapsody");
        assert_eq!(urlencoding("AC/DC"), "AC%2FDC");
    }
}
