use mwp_content::Link;
use mwp_search::Doc;
use rusqlite::Connection;
use time::OffsetDateTime;
use url::Url;

#[macro_use]
extern crate log;

mod parser;

use crate::parser::{DomParser, DomParserResult};

pub async fn scrape(link: &Url) -> Result<DomParserResult, Box<dyn std::error::Error>> {
    let response = reqwest::get(link.clone()).await?;

    let html_text = response.text().await?;

    let mut rewriter = DomParser::new();
    rewriter.write(html_text.as_bytes())?;

    Ok(rewriter.wrap())
}

pub async fn scrape_all() -> Result<(), Box<dyn std::error::Error>> {
    info!("scraping all links");

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

    let links = content
        .all()
        .values()
        .flat_map(|p| p.links.clone())
        .collect::<Vec<Link>>();

    info!("collected {} links", links.len());

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

        info!("inserting new link {}", url);

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

    let docs = conn.prepare(
        "SELECT title, url, domain, body, tags, created_at, scraped_at FROM links WHERE body IS NULL AND scraped_at IS NULL",
    )?.query_map(
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
    )?.collect::<Result<Vec<_>, _>>()?;

    info!("will scrape {} documents", docs.len());

    for doc in docs {
        info!("scraping link {}, tags {:?}", doc.url, doc.tags);

        let data = scrape(&doc.url).await;
        let data = match data {
            Ok(data) => data,
            Err(err) => {
                error!("scrape {}: {}", doc.url, err);
                conn.execute(
                    "UPDATE links SET scraped_at = datetime('now') WHERE url = ?1",
                    rusqlite::params![doc.url],
                )?;
                continue;
            }
        };

        conn.execute(
            "UPDATE links SET body = ?1, scraped_at = datetime('now') WHERE url = ?2",
            rusqlite::params![data.digest, doc.url],
        )?;
    }

    Ok(())
}
