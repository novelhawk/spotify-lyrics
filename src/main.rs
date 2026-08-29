use std::{sync::Arc, time::Duration};

use application::{Alert, ApplicationEvent, ApplicationStatus, PlaybackStatus, Song};
use color_eyre::eyre::{Context, ContextCompat, Result};
use discord_api::get_spotify_token;
use lyrics_providers::{LyricsManager, TrackQuery};
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Flex, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use spotify_api::{
    models::spotify_message::SpotifyMessage,
    spotify_api::{connect, subscribe_player},
};
use tokio::sync::{mpsc, watch};
use utils::time::print_hhmm;

pub mod application;
pub mod discord_api;
pub mod lyrics_context;
pub mod lyrics_providers;
pub mod lyrix_api;
pub mod spotify_api;
pub mod utils;

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

    let (tx, rx) = mpsc::channel::<ApplicationEvent>(32);
    let lyrics_manager = Arc::new(LyricsManager::from_env());

    tokio::spawn(connect(spotify_access_token.clone(), tx.clone()));
    tokio::spawn(application_loop(
        spotify_access_token.clone(),
        rx,
        tx.clone(),
        state_tx,
        lyrics_manager,
    ));

    console_render(state_rx).await?;

    Ok(())
}

async fn console_render(state_rx: watch::Receiver<ApplicationStatus>) -> Result<()> {
    let mut terminal = ratatui::init();

    loop {
        let status = state_rx.borrow();

        terminal
            .draw(|frame| {
                let area = frame.area();
                let mut text = vec![];

                let mut foreground = Color::Rgb(200, 200, 200);
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
                        .skip(index.checked_sub(3).unwrap_or(0))
                        .take(7)
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
                .split(area);

                frame.render_widget(Block::new().bg(background), area);
                frame.render_widget(
                    Paragraph::new(text.into_iter().map(Line::from).collect::<Vec<_>>()).centered(),
                    areas[1],
                );

                // Render alert banner if active and not expired
                if let Some(alert) = &status.alert {
                    if !alert.is_expired() {
                        let alert_msg = &alert.message;
                        let max_w = area.width.saturating_sub(4).max(10);
                        let box_w = ((alert_msg.len() as u16 + 6).min(max_w)).max(28);
                        let box_h = 3u16;

                        let v_chunks = Layout::vertical([
                            Constraint::Length(1),
                            Constraint::Length(box_h),
                            Constraint::Min(0),
                        ])
                        .split(area);

                        let h_chunks = Layout::horizontal([
                            Constraint::Fill(1),
                            Constraint::Length(box_w),
                            Constraint::Fill(1),
                        ])
                        .split(v_chunks[1]);

                        let alert_block = Block::bordered()
                            .border_style(Style::default().fg(Color::Yellow))
                            .bg(Color::Rgb(25, 20, 20))
                            .title(Span::styled(
                                " ⚠ Alert ",
                                Style::default().fg(Color::Yellow).bold(),
                            ));

                        let alert_p = Paragraph::new(Line::from(vec![Span::styled(
                            alert_msg,
                            Style::default().fg(Color::White).bold(),
                        )]))
                        .block(alert_block)
                        .centered();

                        frame.render_widget(alert_p, h_chunks[1]);
                    }
                }
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
    event_tx: mpsc::Sender<ApplicationEvent>,
    state_tx: watch::Sender<ApplicationStatus>,
    lyrics_manager: Arc<LyricsManager>,
) -> Result<()> {
    let mut cloned = ApplicationStatus::default();
    let mut last_track = String::new();
    let alert_duration = Duration::from_secs(
        std::env::var("ALERT_DURATION_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5),
    );

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
                        spotify_track_id: Some(event.spotify_track_id.clone()),
                        author: event.artist.clone(),
                        name: event.song.clone(),
                    });

                    if last_track != event.spotify_track_id {
                        last_track = event.spotify_track_id.clone();
                        cloned.lyrics = vec![];
                        cloned.colors = None;
                        cloned.alert = None; // Reset alert on track change

                        let manager = lyrics_manager.clone();
                        let tx = event_tx.clone();
                        let track_id = event.spotify_track_id.clone();
                        let query = TrackQuery {
                            track_id: Some(track_id.clone()),
                            track_name: event.song.clone(),
                            artist_name: event.artist.clone(),
                            duration: Some(event.duration),
                        };

                        // Async, non-blocking lyrics fetching across providers
                        tokio::spawn(async move {
                            let result = manager.fetch_lyrics(&query).await;
                            let _ = tx
                                .send(ApplicationEvent::LyricsFetched { track_id, result })
                                .await;
                        });
                    }

                    state_tx
                        .send(cloned.clone())
                        .wrap_err("Failed to update application state")?;
                }
            },
            ApplicationEvent::LyricsFetched { track_id, result } => {
                // Ensure the result corresponds to the currently active track
                if track_id == last_track {
                    match result {
                        Ok(lyrics_res) => {
                            cloned.lyrics = lyrics_res.lyrics;
                            cloned.colors = lyrics_res.colors;
                            cloned.alert = None;
                        }
                        Err(err) => {
                            cloned.lyrics = vec![];
                            cloned.alert = Some(Alert::new(err.user_message(), alert_duration));
                        }
                    }

                    state_tx
                        .send(cloned.clone())
                        .wrap_err("Failed to update application state with lyrics")?;
                }
            }
        }
    }

    Ok(())
}
