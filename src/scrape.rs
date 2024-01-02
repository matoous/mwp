use crate::parser::{DomParser, DomParserResult};

pub async fn scrape(link: &str) -> Result<DomParserResult, Box<dyn std::error::Error>> {
    let response = reqwest::get(link).await?;

    let html_text = response.text().await?;

    let mut rewriter = DomParser::new();
    rewriter.write(html_text.as_bytes())?;

    Ok(rewriter.wrap())
}
