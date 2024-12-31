#![forbid(unsafe_code)]

use clap::Parser;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Helper commands for wiki management")]
enum Xtask {
    #[command(about = "Scrape latest content for full-text search")]
    Scrape,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let xtask = Xtask::parse();

    match xtask {
        Xtask::Scrape => scrape().await,
    }
}

async fn scrape() -> Result<(), Box<dyn std::error::Error>> {
    mwp_scraper::scrape_all().await?;
    Ok(())
}
