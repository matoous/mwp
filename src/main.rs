use serde::{Deserialize, Serialize};
use tantivy::{schema::*, Index};
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

pub struct SearchIndex {
    pub index: Index,
}

impl SearchIndex {
    pub fn new(dir: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let index = if let Ok(index) = Index::open_in_dir(dir) {
            index
        } else {
            let mut schema_builder = Schema::builder();
            schema_builder.add_text_field("title", TEXT | STORED);
            schema_builder.add_text_field("body", TEXT | STORED);
            schema_builder.add_text_field("url", STRING | STORED);
            schema_builder.add_text_field("domain", STRING | STORED);
            schema_builder.add_text_field("tags", STRING | STORED | FAST);
            schema_builder.add_text_field("kind", STRING | STORED);
            schema_builder.add_facet_field("hiearchy", FacetOptions::default());
            let schema = schema_builder.build();
            Index::builder()
                .schema(schema)
                .settings(tantivy::IndexSettings {
                    docstore_blocksize: 32_000_000,
                    ..tantivy::IndexSettings::default()
                })
                .create_in_dir(dir)?
        };

        Ok(SearchIndex { index })
    }

    fn add(&self, docs: Vec<Doc>) -> Result<(), Box<dyn std::error::Error>> {
        let schema = self.index.schema();

        let mut index_writer = self.index.writer(32_000_000).unwrap();

        let title = schema.get_field("title").unwrap();
        let body = schema.get_field("body").unwrap();
        let url = schema.get_field("url").unwrap();
        let domain = schema.get_field("domain").unwrap();
        let tags = schema.get_field("tags").unwrap();
        let kind = schema.get_field("kind").unwrap();

        for doc in docs {
            let mut document = Document::default();
            document.add_text(title, &doc.title);
            document.add_text(body, &doc.body);
            document.add_text(url, &doc.url);
            document.add_text(domain, &doc.domain);
            document.add_text(tags, "test");
            document.add_text(tags, "example");
            document.add_text(tags, "something");
            document.add_text(kind, "link");

            println!("Adding: {} ({})\n{}\n", doc.title, doc.url, doc.body);

            index_writer.add_document(document)?;
        }

        index_writer.commit().expect("commit index");

        Ok(())
    }

    fn add_content(
        &self,
        content: Vec<mwp_content::Page>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let schema = self.index.schema();

        let mut index_writer = self.index.writer(32_000_000).unwrap();

        let title = schema.get_field("title").unwrap();
        let body = schema.get_field("body").unwrap();
        let kind = schema.get_field("kind").unwrap();
        let url = schema.get_field("url").unwrap();
        let tags = schema.get_field("tags").unwrap();
        let hiearchy = schema.get_field("hiearchy").unwrap();

        for page in content {
            let mut document = Document::default();
            document.add_text(title, &page.title);
            document.add_text(body, &page.text);
            document.add_text(url, &page.path);
            document.add_text(kind, "link");
            for tag in page.tags {
                document.add_text(tags, tag);
            }
            if !page.hiearchy.is_empty() {
                document.add_facet(hiearchy, &page.hiearchy);
            }

            index_writer.add_document(document)?;
        }

        index_writer.commit().expect("commit index");

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = sled::open("./db").unwrap();
    let index = SearchIndex::new("./index")?;

    let content = mwp_content::read_dir("../wiki").await;
    index.add_content(content.into_values().collect())?;

    let links = extract::collect_links();

    println!("{:?}", links);

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
    }

    db.flush()?;

    let all_docs = db
        .iter()
        .values()
        .filter_map(|value| value.ok())
        .map(|v| serde_json::from_slice::<Doc>(v.as_ref()).unwrap())
        .collect::<Vec<Doc>>();

    index.add(all_docs)?;

    Ok(())
}
