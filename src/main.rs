use anyhow::Result;
use clap::Parser;

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
    
    let mut repl = Repl::new(config).await?;
    repl.run().await?;
    
    Ok(())
}
