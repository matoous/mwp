use serde::{Deserialize, Serialize};
use tantivy::{collector::TopDocs, query::QueryParser, schema::*, Index};
use time::OffsetDateTime;
use url::Url;

mod extract;

// TODO: use proto?
#[derive(Serialize, Deserialize)]
struct Doc {
    pub url: Url,
    pub domain: String,
    pub title: String,
    pub body: String,
    pub facet: Option<String>,
    pub tags: Option<Vec<String>>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub scraped_at: Option<OffsetDateTime>,
}

impl Doc {
    fn new(
        url: Url,
        title: String,
        body: String,
        facet: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Self {
        let domain = url.domain().unwrap().into();
        Doc {
            url,
            domain,
            title,
            body,
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
    let db = sled::open("./db").unwrap();

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
        schema_builder.add_text_field("domain", STRING | STORED);
        schema_builder.add_text_field("tags", STRING | STORED | FAST);
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
    let domain = schema.get_field("domain").unwrap();
    let tags = schema.get_field("tags").unwrap();

    for link in links.iter().take(50) {
        if db.get(link.url.as_str())?.is_some() {
            continue;
        }

        let data = mwp_scraper::scrape(&link.url).await;
        let data = match data {
            Ok(data) => data,
            Err(err) => {
                println!("Scrape {}: {}", link.url, err);
                continue;
            }
        };

        let mut doc = Doc::new(
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

        doc.scraped_at = Some(OffsetDateTime::now_utc());

        db.insert(doc.url.as_str(), serde_json::to_vec(&doc).unwrap())?;

        let mut document = Document::default();
        document.add_text(title, &doc.title);
        document.add_text(body, &doc.body);
        document.add_text(url, &doc.url);
        document.add_text(domain, &doc.domain);
        document.add_text(tags, "test");
        document.add_text(tags, "example");
        document.add_text(tags, "something");

        println!("Adding: {} ({})\n{}\n", doc.title, doc.url, doc.body);

        index_writer.add_document(document)?;
    }

    db.flush()?;

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

    Ok(())
}
