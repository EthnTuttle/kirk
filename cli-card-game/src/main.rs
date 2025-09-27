use anyhow::Result;
use clap::Parser;

mod app;
mod config;
mod repl;

use app::GameApp;
use config::GameConfig;

#[derive(Parser)]
#[command(name = "kirk-cards")]
#[command(about = "A CLI card game demonstrating the Kirk gaming protocol")]
#[command(version)]
struct Cli {
    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,
    
    /// Master seed file path for key persistence
    #[arg(short, long)]
    seed_file: Option<String>,
    
    /// Default wager amount for challenges
    #[arg(short, long, default_value = "100")]
    wager: u64,
    
    /// Game timeout in seconds
    #[arg(short, long, default_value = "300")]
    timeout: u64,
    
    /// Embedded mint port
    #[arg(long, default_value = "3338")]
    mint_port: u16,
    
    /// Embedded relay port
    #[arg(long, default_value = "7000")]
    relay_port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Parse command line arguments
    let cli = Cli::parse();
    
    // Create configuration
    let config = GameConfig {
        mint_port: cli.mint_port,
        relay_port: cli.relay_port,
        default_wager: cli.wager,
        game_timeout_seconds: cli.timeout,
        master_seed_file: cli.seed_file,
    };
    
    // Create and run the game application
    let mut app = GameApp::new(config).await?;
    app.run().await?;
    
    Ok(())
}