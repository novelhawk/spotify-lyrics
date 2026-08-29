use reqwest::header;
use serde::Deserialize;
use tracing::{info, instrument, span, Level};

const BASE_URI: &str = "https://discord.com/api";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

#[derive(Deserialize, Debug)]
struct AccessTokenResponse {
    access_token: String,
}

#[instrument]
pub async fn get_spotify_token(access_token: &str, user_id: &str) -> Option<String> {
    let client = reqwest::Client::new();

    info!("Test");
    let uri = format!("{BASE_URI}/v9/users/@me/connections/spotify/{user_id}/access-token");
    let res = client
        .get(uri)
        .header(header::AUTHORIZATION, access_token)
        .header(header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .ok()?;

    span!(Level::INFO, "New Span");

    info!("New message");

    let res: AccessTokenResponse = res.json().await.ok()?;

    Some(res.access_token)
}
