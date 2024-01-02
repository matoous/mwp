use parser::{DomParser, DomParserResult};
use rusqlite::Connection;
use tantivy::{collector::TopDocs, query::QueryParser, schema::*, Index};
use time::OffsetDateTime;

mod extract;
mod parser;
mod scrape;

struct Doc {
    pub title: String,
    pub body: String,
    pub url: String,
    pub facet: Option<String>,
    pub tags: Option<Vec<String>>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub scraped_at: Option<OffsetDateTime>,
}

impl Doc {
    fn new(
        url: String,
        title: String,
        body: String,
        facet: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Self {
        Doc {
            title,
            body,
            url,
            facet,
            tags,
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
            scraped_at: None,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open("./data/db.db3")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS links (
            url   TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            body  TEXT NOT NULL,
            facet TEXT,
            tags  TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            scraped_at TEXT
        )",
        (),
    )?;

    let links = extract::collect_links();

    println!("{:?}", links);

    let index_path = "./index";
    let index = if let Ok(index) = Index::open_in_dir(index_path) {
        index
    } else {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("title", TEXT | STORED);
        schema_builder.add_text_field("body", TEXT | STORED);
        schema_builder.add_text_field("url", STRING | STORED);
        schema_builder.add_text_field("tags", STRING | STORED);
        let schema = schema_builder.build();
        Index::builder()
            .schema(schema)
            .settings(tantivy::IndexSettings {
                docstore_blocksize: 32_000_000,
                ..tantivy::IndexSettings::default()
            })
            .create_in_dir(index_path)?
    };

    let schema = index.schema();

    let mut index_writer = index.writer(32_000_000).unwrap();

    let title = schema.get_field("title").unwrap();
    let body = schema.get_field("body").unwrap();
    let url = schema.get_field("url").unwrap();
    let tags = schema.get_field("tags").unwrap();

    for link in links.iter().take(50) {
        let data = scrape::scrape(&link.url).await;
        let data = match data {
            Ok(data) => data,
            Err(err) => {
                println!("Scrape {}: {}", link.url, err);
                continue;
            }
        };

        let doc = Doc::new(
            link.url.clone(),
            data.title,
            data.digest,
            Some(link.facet.clone()),
            Some(
                link.facet
                    .split('/')
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>(),
            ),
        );

        conn.execute(
            "INSERT INTO links
                (url, title, body, facet, tags, created_at, updated_at, scraped_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            (
                &doc.url,
                &doc.title,
                &doc.body,
                &doc.facet,
                doc.tags.map(|f| f.join(",")),
                &doc.created_at,
                &doc.updated_at,
                &doc.scraped_at,
            ),
        )?;

        let mut document = Document::default();
        document.add_text(title, &doc.title);
        document.add_text(body, &doc.body);
        document.add_text(url, &doc.url);
        document.add_text(tags, "test");
        document.add_text(tags, "example");
        document.add_text(tags, "something");

        println!("Adding: {} ({})\n{}\n", doc.title, doc.url, doc.body);

        index_writer.add_document(document)?;
    }

    index_writer.commit().expect("commit index");

    let reader = index.reader()?;
    let searcher = reader.searcher();

    let query_parser = QueryParser::for_index(&index, vec![title, body]);

    let query = query_parser.parse_query("Rust AND parser")?;
    let top_docs = searcher.search(&query, &TopDocs::with_limit(10))?;

    for (score, doc_address) in top_docs {
        let retrieved_doc = searcher.doc(doc_address)?;
        let title = retrieved_doc.get_first(title).unwrap().as_text().unwrap();
        let url = retrieved_doc.get_first(url).unwrap().as_text().unwrap();

        println!("{} ({}) - Score: {}", title, url, score);
        println!("{:?}", retrieved_doc);
    }

    conn.close().expect("close db connection");

    Ok(())
}
