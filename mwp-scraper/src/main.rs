use mwp_content::Link;
use mwp_search::Doc;
use rusqlite::Connection;
use time::OffsetDateTime;
use url::Url;

mod parser;

use crate::parser::{DomParser, DomParserResult};

pub async fn scrape(link: &Url) -> Result<DomParserResult, Box<dyn std::error::Error>> {
    let response = reqwest::get(link.clone()).await?;

    let html_text = response.text().await?;

    let mut rewriter = DomParser::new();
    rewriter.write(html_text.as_bytes())?;

    Ok(rewriter.wrap())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open("./db.db3")?;
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS links (
            url    TEXT PRIMARY KEY,
            title  TEXT NOT NULL,
            body   TEXT,
            domain TEXT,
            tags   TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
            scraped_at TIMESTAMP
        );
        "#,
        (), // empty list of parameters.
    )?;

    let content = mwp_content::Content::from_dir("../wiki").await;

    // only needed before links from content are migrated to bookmarking system
    let links = content
        .all()
        .values()
        .flat_map(|p| p.links.clone())
        .collect::<Vec<Link>>();

    let mut stmt = conn.prepare("SELECT * FROM links WHERE url = ?1")?;
    for Link {
        title, url, tags, ..
    } in links.into_iter()
    {
        if stmt.exists([url.as_str()])? {
            continue;
        };

        // let data = mwp_scraper::scrape(&link.url).await;
        // let data = match data {
        //     Ok(data) => data,
        //     Err(err) => {
        //         println!("Scrape {}: {}", link.url, err);
        //         continue;
        //     }
        // };

        // println!("Link {} to {}", link.title, link.url);
        let mut doc = Doc::new(
            title,
            url.clone(),
            // Some(data.digest),
            None,
            Some(tags),
        );

        doc.scraped_at = Some(OffsetDateTime::now_utc());

        conn.execute(
            "INSERT INTO links (url, title, body, domain, tags) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                doc.url.as_str(),
                doc.title,
                Option::<String>::None,
                doc.domain,
                doc.tags.map(|tags| tags.join(";")),
            ],
        )?;
    }

    loop {
        let link =  match conn.query_row(
            "SELECT title, url, domain, body, tags, created_at, scraped_at FROM links WHERE body IS NULL AND scraped_at IS NULL LIMIT 1",
            [],
            |row| Ok(Doc {
                 title: row.get(0)?,
                 url: row.get(1)?,
                 domain: row.get(2)?,
                 body: row.get(3)?,
                 tags: row.get::<usize, Option<String>>(4).map(|res| res.map(|s| s.split(';').map(|s| s.into()).collect::<Vec<String>>()))?,
                 created_at: row.get(5)?,
                 scraped_at: row.get(6)?,
            }),
        ) {
            Ok(link) => link,
            Err(e) => {
                println!("query link: {:?}", e);
                break;
            },
        };

        let data = scrape(&link.url).await;
        let data = match data {
            Ok(data) => data,
            Err(err) => {
                println!("Scrape {}: {}", link.url, err);
                conn.execute(
                    "UPDATE links SET scraped_at = datetime('now') WHERE url = ?1",
                    rusqlite::params![link.url],
                )?;
                continue;
            }
        };

        conn.execute(
            "UPDATE links SET body = ?1, scraped_at = datetime('now') WHERE url = ?2",
            rusqlite::params![data.digest, link.url],
        )?;
    }

    Ok(())
}
