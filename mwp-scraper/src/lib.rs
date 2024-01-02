use url::Url;

use crate::parser::{DomParser, DomParserResult};

mod parser;

pub async fn scrape(link: &Url) -> Result<DomParserResult, Box<dyn std::error::Error>> {
    let response = reqwest::get(link.clone()).await?;

    let html_text = response.text().await?;

    let mut rewriter = DomParser::new();
    rewriter.write(html_text.as_bytes())?;

    Ok(rewriter.wrap())
}
