use anyhow::Result;
use clap::Parser;
use tokio::signal;
use tokio_util::sync::CancellationToken;

mod cli;
mod config;
mod providers;
mod session;
mod repl;
mod history;
mod commands;
mod ui;

use cli::Args;
use config::Config;
use repl::Repl;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = Config::from_args(&args)?;
    
    // Create a global cancellation token for Ctrl-C handling
    let global_cancel_token = CancellationToken::new();
    let cancel_token_for_signal = global_cancel_token.clone();
    
    // Set up Ctrl-C handler that can be reset
    let cancel_token_for_signal_clone = cancel_token_for_signal.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(()) = signal::ctrl_c().await {
                cancel_token_for_signal_clone.cancel();
                // Wait a bit before listening for the next Ctrl-C
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
    });
    
    let mut repl = Repl::new(config, global_cancel_token).await?;
    repl.run().await?;
    
    Ok(())
}
