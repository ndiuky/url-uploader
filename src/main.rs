use std::time::Duration;

use anyhow::{Context, Result};
use bot::Bot;
use dotenv::dotenv;
use grammers_client::{Client, Config, InitParams};
use grammers_mtsender::{FixedReconnect, ReconnectionPolicy};
use grammers_session::Session;
use log::info;
use simplelog::TermLogger;

mod bot;
mod command;

const SESSION_FILE: &str = "session.bin";
const RECONNECTION_ATTEMPTS: usize = 3;
const RECONNECTION_DELAY_SECS: u64 = 5;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    initialize_logger();

    let credentials = load_credentials()?;
    let client = create_and_connect_client(credentials.clone()).await?;
    
    authorize_bot_if_needed(&client, &credentials.bot_token).await?;
    client.session().save_to_file(SESSION_FILE)?;

    let bot = Bot::new(client).await?;
    bot.run().await;

    Ok(())
}

#[derive(Clone)]
struct Credentials {
    api_id: i32,
    api_hash: String,
    bot_token: String,
}

fn initialize_logger() {
    TermLogger::init(
        log::LevelFilter::Info,
        simplelog::ConfigBuilder::new()
            .set_time_format_rfc3339()
            .build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .expect("Failed to initialize logger");
}

fn load_credentials() -> Result<Credentials> {
    let api_id = std::env::var("API_ID")
        .context("API_ID env is not set")?
        .parse()
        .context("Failed to parse API_ID")?;
    let api_hash = std::env::var("API_HASH").context("API_HASH env is not set")?;
    let bot_token = std::env::var("BOT_TOKEN").context("BOT_TOKEN env is not set")?;

    Ok(Credentials {
        api_id,
        api_hash,
        bot_token,
    })
}

fn create_reconnection_policy() -> &'static dyn ReconnectionPolicy {
    static POLICY: FixedReconnect = FixedReconnect {
        attempts: RECONNECTION_ATTEMPTS,
        delay: Duration::from_secs(RECONNECTION_DELAY_SECS),
    };
    &POLICY
}

async fn create_and_connect_client(credentials: Credentials) -> Result<Client> {
    let config = Config {
        api_id: credentials.api_id,
        api_hash: credentials.api_hash.clone(),
        session: Session::load_file_or_create(SESSION_FILE)?,
        params: InitParams {
            reconnection_policy: create_reconnection_policy(),
            ..Default::default()
        },
    };

    Client::connect(config).await.map_err(Into::into)
}

async fn authorize_bot_if_needed(client: &Client, bot_token: &str) -> Result<()> {
    if !client.is_authorized().await? {
        info!("Not authorized, signing in");
        client.bot_sign_in(bot_token).await?;
    }
    Ok(())
}
