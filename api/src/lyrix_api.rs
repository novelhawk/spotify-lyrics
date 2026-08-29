use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::utils::time::duration_ms_str;
use color_eyre::eyre::{Result, WrapErr};

#[derive(Serialize, Deserialize, Debug)]
pub struct Colors {
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
    pub colors: Colors,
}

const BASE_URL: &str = "https://lyrix.io";

pub async fn get_lyrics(track_id: &str) -> Result<LyrixResponse> {
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{BASE_URL}/getLyrics/{track_id}"))
        .send()
        .await
        .wrap_err("Call failed")?;

    let text = response
        .text()
        .await
        .wrap_err("Failed to read response body")?;

    let res: LyrixResponse = serde_json::from_str(&text)
        .wrap_err("Failed to parse the lyrics from the response body")?;

    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn call_works() {
        // let lyrics = get_lyrics("7MPxEoT36YBCDbrk3ng85S").await;
        // assert!(lyrics.is_some());
        let lyrics = get_lyrics("32M9aAAuuhybp33wiLWtes").await;
        assert!(lyrics.is_ok());
    }
}
