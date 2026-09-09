#![deny(clippy::allow_attributes)]

//! Teams CLI - Lightweight Microsoft Teams client
//!
//! A terminal-based Teams client for Linux.

mod api;
mod auth;
mod calling {
    pub use teams_cli::calling::*;
}
mod config;
mod trouter {
    pub use teams_cli::trouter::*;
}
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "teams-cli")]
#[command(about = "Lightweight CLI client for Microsoft Teams", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    Login {
        /// Force interactive login even if cached token exists
        #[arg(short, long)]
        force: bool,

        #[arg(long)]
        personal: bool,
    },

    Logout,

    Status,

    Chats {
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    Read {
        /// Chat thread ID (from `chats` output)
        chat_id: String,

        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    Send {
        /// Chat thread ID (from `chats` output)
        #[arg(short, long)]
        to: String,

        message: String,
    },

    Teams,

    Whoami,

    /// Connect to Trouter WebSocket push service
    Trouter,

    CallTest {
        /// Duration in seconds to keep the call active
        #[arg(short, long, default_value = "15")]
        duration: u64,

        /// Enable call recording via recorder bot injection
        #[arg(long)]
        record: bool,

        /// Call the Echo / Call Quality Tester bot instead of channel meeting
        #[arg(long)]
        echo: bool,

        /// 1:1 chat thread ID to call (e.g., 19:guid1_guid2@unq.gbl.spaces)
        #[arg(long)]
        thread: Option<String>,

        #[arg(long)]
        callee: Option<String>,

        /// Enable camera capture (V4L2) for video send (requires video-capture feature)
        #[arg(long)]
        camera: bool,

        /// Enable video display window for received video (requires video-capture feature)
        #[arg(long)]
        display: bool,

        /// Use 1kHz test tone instead of real microphone (debug mode)
        #[arg(long)]
        tone: bool,
    },

    /// Test microphone capture: record 3 seconds then play back
    #[cfg(feature = "audio")]
    MicTest,

    #[cfg(any(feature = "video-capture", feature = "video-capture-windows"))]
    CamTest,

    Tui,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter_str = if cli.verbose { "debug" } else { "info" };

    if matches!(cli.command, Commands::Tui) {
        let log_buffer = tui::LogBuffer::new();
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| filter_str.into()),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_ansi(false)
                    .with_writer(log_buffer.clone()),
            )
            .init();

        return tui::run(log_buffer).await;
    }

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| filter_str.into()),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    match cli.command {
        Commands::Login { force, personal } => {
            tracing::info!("Starting authentication flow...");
            auth::login(force, personal).await?;
        }
        Commands::Logout => {
            tracing::info!("Logging out...");
            auth::logout().await?;
        }
        Commands::Status => {
            auth::status().await?;
        }
        Commands::Teams => {
            api::list_teams().await?;
        }
        Commands::Whoami => {
            api::whoami().await?;
        }
        Commands::Chats { limit } => {
            tracing::info!("Fetching chats...");
            api::list_chats(limit).await?;
        }
        Commands::Read { chat_id, limit } => {
            api::read_messages(&chat_id, limit).await?;
        }
        Commands::Send { to, message } => {
            tracing::info!("Sending message...");
            api::send_message(&to, &message).await?;
        }
        Commands::Trouter => {
            trouter::connect_and_run().await?;
        }
        Commands::CallTest {
            duration,
            record,
            echo,
            thread,
            callee,
            camera,
            display,
            tone,
        } => {
            calling::call_test::run_call_test(calling::call_test::CallTestOptions {
                duration_secs: duration,
                record,
                echo,
                thread_override: thread,
                callee_user_id: callee,
                use_camera: camera,
                use_display: display,
                tone_mode: tone,
            })
            .await?;
        }
        #[cfg(feature = "audio")]
        Commands::MicTest => {
            calling::audio::mic_test()?;
        }
        #[cfg(any(feature = "video-capture", feature = "video-capture-windows"))]
        Commands::CamTest => {
            calling::camera::cam_test()?;
        }
        Commands::Tui => unreachable!(),
    }

    Ok(())
}
