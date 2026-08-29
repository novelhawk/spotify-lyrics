use color_eyre::eyre::{Result, WrapErr};
use futures::{future, pin_mut, StreamExt};
use tokio::{
    sync::mpsc::{self, Sender},
    time::sleep,
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{application::ApplicationEvent, spotify_api::models::spotify_message::SpotifyMessage};

const WSS_URI: &str = "wss://dealer.spotify.com/";
const API_BASE: &str = "https://api.spotify.com";
const PING_MSG: &str = r#"{"type":"ping"}"#;

pub async fn connect(access_token: String, tx: Sender<ApplicationEvent>) -> Result<()> {
    let (ping_tx, ping_rx) = mpsc::channel::<Message>(1);
    tokio::spawn(pinger(ping_tx));

    let (ws_stream, _) = connect_async(&format!("{WSS_URI}?access_token={access_token}"))
        .await
        .wrap_err("Failed to connect to Spotify WebSocket")?;

    let (ws_write, ws_read) = ws_stream.split();

    let ws_reader = {
        ws_read.for_each(|message| async {
            if let Ok(Message::Text(text)) = message {
                if let Ok(event) = serde_json::from_str::<SpotifyMessage>(&text) {
                    tx.send(ApplicationEvent::Spotify(event))
                        .await
                        .expect("Failed to publish event");
                }
            };
        })
    };

    let pinger_task = ReceiverStream::new(ping_rx).map(Ok).forward(ws_write);

    pin_mut!(pinger_task, ws_reader);
    future::select(pinger_task, ws_reader).await;

    Ok(())
}

pub async fn subscribe_player(access_token: &str, connection_id: &str) -> Result<()> {
    let client = reqwest::Client::new();

    client
        .put(&format!(
            "{API_BASE}/v1/me/notifications/player?connection_id={connection_id}"
        ))
        .bearer_auth(&access_token)
        .header("content-length", 0)
        .send()
        .await?;

    // let _status = res.status();
    // let _body = res.text().await;

    Ok(())
}

async fn pinger(tx: Sender<Message>) -> Result<()> {
    let ping = PING_MSG.as_bytes().to_vec();
    loop {
        sleep(std::time::Duration::from_secs(30)).await;

        tx.try_send(Message::Ping(ping.clone()))
            .wrap_err("Failed to send ping")?;
    }
}
