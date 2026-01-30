use anyhow::Result;
use clap::Parser;
use tokio::signal;
use tokio::sync::mpsc;

mod cli;
mod config;
mod providers;
mod session;
mod repl;
mod history;
mod commands;
mod ui;
mod prompts;
mod input_history;
mod variable;

use cli::Args;
use config::Config;
use repl::Repl;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = Config::from_args(&args)?;
    
    // Create a channel for Ctrl-C signals
    let (ctrl_c_tx, ctrl_c_rx) = mpsc::unbounded_channel();
    
    // Set up Ctrl-C handler
    tokio::spawn(async move {
        loop {
            if let Ok(()) = signal::ctrl_c().await {
                let _ = ctrl_c_tx.send(());
            }
        }
    });
    
    let mut repl = Repl::new(config, ctrl_c_rx).await?;
    repl.run().await?;
    
    Ok(())
}
