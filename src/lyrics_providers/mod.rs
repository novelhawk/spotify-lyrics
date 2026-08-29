pub mod lrc_parser;
pub mod lrclib;
pub mod lyrix;

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;

use crate::application::{Colors, LyricsLine};

#[derive(Clone, Debug)]
pub struct TrackQuery {
    pub track_id: Option<String>,
    pub track_name: String,
    pub artist_name: Option<String>,
    pub duration: Option<Duration>,
}

#[derive(Clone, Debug)]
pub struct LyricsResult {
    pub lyrics: Vec<LyricsLine>,
    pub colors: Option<Colors>,
    pub source_name: String,
}

#[derive(Debug, Clone)]
pub enum LyricsError {
    NotFound {
        provider: String,
        message: String,
    },
    Unexpected {
        provider: String,
        message: String,
    },
    AllSourcesFailed {
        message: String,
    },
}

impl LyricsError {
    pub fn user_message(&self) -> String {
        match self {
            LyricsError::NotFound { message, .. } => message.clone(),
            LyricsError::Unexpected { message, .. } => message.clone(),
            LyricsError::AllSourcesFailed { message } => message.clone(),
        }
    }
}

impl fmt::Display for LyricsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_message())
    }
}

impl std::error::Error for LyricsError {}

#[async_trait]
pub trait LyricsProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn is_enabled(&self) -> bool;
    async fn fetch_lyrics(&self, query: &TrackQuery) -> Result<LyricsResult, LyricsError>;
}

pub struct LyricsManager {
    providers: Vec<Box<dyn LyricsProvider>>,
}

impl LyricsManager {
    pub fn new(providers: Vec<Box<dyn LyricsProvider>>) -> Self {
        Self { providers }
    }

    pub fn from_env() -> Self {
        let providers: Vec<Box<dyn LyricsProvider>> = vec![
            Box::new(lrclib::LrclibSource::from_env()),
            Box::new(lyrix::LyrixSource::from_env()),
        ];
        Self::new(providers)
    }

    pub fn providers(&self) -> &[Box<dyn LyricsProvider>] {
        &self.providers
    }

    pub async fn fetch_lyrics(&self, query: &TrackQuery) -> Result<LyricsResult, LyricsError> {
        let enabled_providers: Vec<&Box<dyn LyricsProvider>> =
            self.providers.iter().filter(|p| p.is_enabled()).collect();

        if enabled_providers.is_empty() {
            return Err(LyricsError::AllSourcesFailed {
                message: "No lyrics providers are enabled".to_string(),
            });
        }

        let mut errors = Vec::new();
        let mut any_unexpected = false;

        for provider in &enabled_providers {
            match provider.fetch_lyrics(query).await {
                Ok(result) => {
                    if !result.lyrics.is_empty() {
                        return Ok(result);
                    }
                }
                Err(err) => {
                    if matches!(err, LyricsError::Unexpected { .. }) {
                        any_unexpected = true;
                    }
                    errors.push(err);
                }
            }
        }

        let artist_str = query
            .artist_name
            .as_deref()
            .map(|a| format!(" by {a}"))
            .unwrap_or_default();

        if any_unexpected {
            let err_details = errors
                .iter()
                .filter_map(|e| match e {
                    LyricsError::Unexpected { provider, message } => {
                        Some(format!("{provider}: {message}"))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("; ");
            Err(LyricsError::Unexpected {
                provider: "all".to_string(),
                message: format!("Error fetching lyrics: {err_details}"),
            })
        } else {
            Err(LyricsError::NotFound {
                provider: "all".to_string(),
                message: format!(
                    "No lyrics found for \"{}{}\" across all sources",
                    query.track_name, artist_str
                ),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::LyricsSegment;

    struct MockProvider {
        name: &'static str,
        enabled: bool,
        result: Option<Result<LyricsResult, LyricsError>>,
    }

    #[async_trait]
    impl LyricsProvider for MockProvider {
        fn name(&self) -> &'static str {
            self.name
        }

        fn is_enabled(&self) -> bool {
            self.enabled
        }

        async fn fetch_lyrics(&self, _query: &TrackQuery) -> Result<LyricsResult, LyricsError> {
            self.result
                .clone()
                .unwrap_or_else(|| Err(LyricsError::NotFound {
                    provider: self.name.into(),
                    message: "Mock not found".into(),
                }))
        }
    }

    #[tokio::test]
    async fn test_first_provider_succeeds() {
        let provider1 = MockProvider {
            name: "P1",
            enabled: true,
            result: Some(Ok(LyricsResult {
                lyrics: vec![LyricsLine {
                    start: Duration::ZERO,
                    segments: vec![LyricsSegment {
                        start: Duration::ZERO,
                        text: "Line 1".into(),
                    }],
                }],
                colors: None,
                source_name: "P1".into(),
            })),
        };

        let manager = LyricsManager::new(vec![Box::new(provider1)]);
        let query = TrackQuery {
            track_id: Some("123".into()),
            track_name: "Test Song".into(),
            artist_name: Some("Test Artist".into()),
            duration: None,
        };

        let res = manager.fetch_lyrics(&query).await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap().source_name, "P1");
    }

    #[tokio::test]
    async fn test_fallback_to_second_provider() {
        let provider1 = MockProvider {
            name: "P1",
            enabled: true,
            result: Some(Err(LyricsError::NotFound {
                provider: "P1".into(),
                message: "Not found on P1".into(),
            })),
        };

        let provider2 = MockProvider {
            name: "P2",
            enabled: true,
            result: Some(Ok(LyricsResult {
                lyrics: vec![LyricsLine {
                    start: Duration::ZERO,
                    segments: vec![LyricsSegment {
                        start: Duration::ZERO,
                        text: "Line from P2".into(),
                    }],
                }],
                colors: None,
                source_name: "P2".into(),
            })),
        };

        let manager = LyricsManager::new(vec![Box::new(provider1), Box::new(provider2)]);
        let query = TrackQuery {
            track_id: Some("123".into()),
            track_name: "Test Song".into(),
            artist_name: Some("Test Artist".into()),
            duration: None,
        };

        let res = manager.fetch_lyrics(&query).await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap().source_name, "P2");
    }

    #[tokio::test]
    async fn test_all_providers_fail_not_found() {
        let provider1 = MockProvider {
            name: "P1",
            enabled: true,
            result: Some(Err(LyricsError::NotFound {
                provider: "P1".into(),
                message: "Not found on P1".into(),
            })),
        };

        let provider2 = MockProvider {
            name: "P2",
            enabled: true,
            result: Some(Err(LyricsError::NotFound {
                provider: "P2".into(),
                message: "Not found on P2".into(),
            })),
        };

        let manager = LyricsManager::new(vec![Box::new(provider1), Box::new(provider2)]);
        let query = TrackQuery {
            track_id: Some("123".into()),
            track_name: "Bohemian Rhapsody".into(),
            artist_name: Some("Queen".into()),
            duration: None,
        };

        let res = manager.fetch_lyrics(&query).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(err, LyricsError::NotFound { .. }));
        assert_eq!(
            err.user_message(),
            "No lyrics found for \"Bohemian Rhapsody by Queen\" across all sources"
        );
    }

    #[tokio::test]
    async fn test_unexpected_error_reported() {
        let provider1 = MockProvider {
            name: "P1",
            enabled: true,
            result: Some(Err(LyricsError::Unexpected {
                provider: "P1".into(),
                message: "Connection timeout".into(),
            })),
        };

        let manager = LyricsManager::new(vec![Box::new(provider1)]);
        let query = TrackQuery {
            track_id: Some("123".into()),
            track_name: "Song".into(),
            artist_name: None,
            duration: None,
        };

        let res = manager.fetch_lyrics(&query).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(err, LyricsError::Unexpected { .. }));
        assert_eq!(
            err.user_message(),
            "Error fetching lyrics: P1: Connection timeout"
        );
    }

    #[tokio::test]
    async fn test_disabled_provider_skipped() {
        let provider1 = MockProvider {
            name: "P1",
            enabled: false,
            result: Some(Ok(LyricsResult {
                lyrics: vec![],
                colors: None,
                source_name: "P1".into(),
            })),
        };

        let manager = LyricsManager::new(vec![Box::new(provider1)]);
        let query = TrackQuery {
            track_id: Some("123".into()),
            track_name: "Song".into(),
            artist_name: None,
            duration: None,
        };

        let res = manager.fetch_lyrics(&query).await;
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), LyricsError::AllSourcesFailed { .. }));
    }
}
