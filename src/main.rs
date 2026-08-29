use std::{
    collections::HashMap,
    ops::{Div, Rem},
    time::Duration,
};

use application::{
    ApplicationEvent, ApplicationStatus, Colors, LyricsLine, LyricsSegment, PlaybackStatus, Song,
    RGB,
};
use color_eyre::eyre::{Context, ContextCompat, Result};
use discord_api::get_spotify_token;
use lyrix_api::{get_lyrics, LyrixLyrics};
use opentelemetry::{
    global::{self, tracer_provider},
    trace::{Tracer, TracerProvider as _},
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::TracerProvider;
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Flex, Layout},
    style::{Color, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use spotify_api::{
    models::spotify_message::SpotifyMessage,
    spotify_api::{connect, subscribe_player},
};
use tokio::sync::{
    mpsc::{self},
    watch::{self},
};
use tracing::info;
use utils::time::print_hhmm;

pub mod application;
pub mod discord_api;
pub mod lyrics_context;
pub mod lyrix_api;
pub mod spotify_api;
pub mod utils;

// fn init_opentelemetry() {
//     let tracer_provider = opentelemetry_otlp::new_pipeline()
//         .tracing()
//         .with_exporter(
//             opentelemetry_otlp::new_exporter()
//                 .tonic()
//                 .with_endpoint("http://localhost:4317")
//                 .with_timeout(Duration::from_secs(3)),
//         )
//         .install_batch(opentelemetry_sdk::runtime::Tokio)
//         .wrap_err("Failed to install OpenTelemetry tracing pipeline")?;
//
//     global::set_tracer_provider(tracer_provider);
//
//     let tracer = global::tracer("spotify_lyrics_tracer");
//
//     {
//         let _span = tracer.start("test");
//         info!("Test");
//     }
//
// info!("Test");

// global::shutdown_tracer_provider();

// }

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    color_eyre::install()?;

    let discord_access_token = std::env::var("DISCORD_ACCESS_TOKEN")
        .wrap_err("DISCORD_ACCESS_TOKEN environment variable not set")?;
    let user_id = std::env::var("USER_ID")
        .or_else(|_| std::env::var("DISCORD_USER_ID"))
        .wrap_err("USER_ID environment variable not set")?;

    let spotify_access_token = get_spotify_token(&discord_access_token, &user_id)
        .await
        .wrap_err("Failed to fetch spotify access token")?;

    let (state_tx, state_rx) = watch::channel(ApplicationStatus::default());

    let (tx, rx) = mpsc::channel::<ApplicationEvent>(5);
    tokio::spawn(connect(spotify_access_token.clone(), tx));
    tokio::spawn(application_loop(spotify_access_token.clone(), rx, state_tx));

    console_render(state_rx).await?;

    Ok(())
}

async fn console_render(state_rx: watch::Receiver<ApplicationStatus>) -> Result<()> {
    let mut terminal = ratatui::init();

    loop {
        let status = state_rx.borrow();

        terminal
            .draw(|frame| {
                let mut text = vec![];

                let mut foreground = Color::Rgb(255, 255, 255);
                let mut highlight = Color::Rgb(255, 255, 255);
                let mut background = Color::Rgb(0, 0, 0);
                if let Some(colors) = &status.colors {
                    foreground = Color::Rgb(colors.text.0, colors.text.1, colors.text.2);
                    highlight =
                        Color::Rgb(colors.highlight.0, colors.highlight.1, colors.highlight.2);
                    background = Color::Rgb(
                        colors.background.0,
                        colors.background.1,
                        colors.background.2,
                    );
                }

                if let Some(playback) = &status.playback {
                    let position = playback.position();

                    let index = status
                        .lyrics
                        .iter()
                        .position(|it| it.start > position)
                        .and_then(|it| it.checked_sub(1))
                        .unwrap_or(0);

                    text = status
                        .lyrics
                        .clone()
                        .into_iter()
                        .enumerate()
                        .map(|(ix, lyrics)| {
                            lyrics
                                .segments
                                .into_iter()
                                .map(|seg| {
                                    Span::default().content(seg.text).fg(if ix == index {
                                        highlight
                                    } else {
                                        foreground
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .skip(index.checked_sub(1).unwrap_or(0))
                        .take(3)
                        .collect();

                    text.push(vec![Span::default().content(format!(
                        "{} / {}",
                        print_hhmm(position),
                        print_hhmm(playback.duration)
                    ))]);
                }

                let areas = Layout::vertical(&[
                    Constraint::Fill(1),
                    Constraint::Length(text.len() as u16),
                    Constraint::Fill(1),
                ])
                .flex(Flex::Center)
                .split(frame.area());

                frame.render_widget(Block::new().bg(background), frame.area());
                frame.render_widget(
                    Paragraph::new(text.into_iter().map(|l| Line::from(l)).collect::<Vec<_>>())
                        .centered(),
                    areas[1],
                )
            })
            .wrap_err("Failed to draw to terminal")?;

        if event::poll(Duration::from_millis(20))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    ratatui::restore();
    Ok(())
}

async fn application_loop(
    spotify_token: String,
    mut rx: mpsc::Receiver<ApplicationEvent>,
    state_tx: watch::Sender<ApplicationStatus>,
) -> Result<()> {
    let mut cloned = ApplicationStatus::default();
    let mut last_track = String::new();

    while let Some(event) = rx.recv().await {
        match event {
            ApplicationEvent::Spotify(spotify) => match spotify {
                SpotifyMessage::Connect(conn_id) => subscribe_player(&spotify_token, &conn_id)
                    .await
                    .wrap_err("Failed to subscribe to Spotify player events")?,
                SpotifyMessage::PlayerStateChanged(event) => {
                    // HACK: Timestamp not updated on volume change
                    if let Some(old) = &cloned.playback {
                        if old.last_update == event.timestamp {
                            continue;
                        }
                    }

                    cloned.playback = Some(PlaybackStatus {
                        paused: event.paused,
                        position: event.progress,
                        duration: event.duration,
                        last_update: event.timestamp,
                    });

                    cloned.song = Some(Song {
                        spotify_track_id: None,
                        author: event.artist,
                        name: event.song,
                    });

                    if last_track != event.spotify_track_id {
                        last_track = event.spotify_track_id.clone();
                        cloned.lyrics = vec![];
                        if let Ok(lyrics) = get_lyrics(&event.spotify_track_id).await {
                            cloned.lyrics = flatten_lyrics(lyrics.lyrics);
                            cloned.colors = Some(Colors {
                                background: parse_color(lyrics.colors.background),
                                text: parse_color(lyrics.colors.text),
                                highlight: parse_color(lyrics.colors.highlight_text),
                            });
                        }
                    }

                    state_tx
                        .send(cloned.clone())
                        .wrap_err("Failed to update application state")?;
                }
            },
        }
    }

    Ok(())
}

fn parse_color(color: i64) -> RGB {
    let hex = 16777216 + color;

    RGB(
        hex.div(256 * 256) as u8,
        hex.div(256).rem(256) as u8,
        hex.rem(256) as u8,
    )
}

fn flatten_lyrics(lyrics: LyrixLyrics) -> Vec<LyricsLine> {
    lyrics
        .lines
        .into_iter()
        .map(|line| LyricsLine {
            start: line.start_time,
            segments: vec![LyricsSegment {
                start: line.start_time,
                text: line.words,
            }],
        })
        .collect()
}
