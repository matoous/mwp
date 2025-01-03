use tantivy::{schema::*, Index, IndexWriter};
use time::OffsetDateTime;
use url::Url;

pub struct SearchIndex {
    pub index: Index,
}

#[derive(Clone)]
pub struct Doc {
    pub title: String,
    pub url: Url,
    pub domain: String,
    pub body: Option<String>,
    pub tags: Option<Vec<String>>,
    pub created_at: OffsetDateTime,
    pub scraped_at: Option<OffsetDateTime>,
}

impl Doc {
    pub fn new(title: String, url: Url, body: Option<String>, tags: Option<Vec<String>>) -> Self {
        let domain = url.domain().unwrap().into();
        Doc {
            title,
            url,
            domain,
            body,
            tags,
            created_at: OffsetDateTime::now_utc(),
            scraped_at: None,
        }
    }
}

impl SearchIndex {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("url", STRING | STORED);
        schema_builder.add_text_field("title", TEXT | STORED);
        schema_builder.add_text_field("body", TEXT | STORED);
        schema_builder.add_text_field("domain", STRING | STORED);
        schema_builder.add_text_field("tags", STRING | STORED | FAST);
        let schema = schema_builder.build();
        let index = Index::builder()
            .schema(schema)
            .settings(tantivy::IndexSettings {
                docstore_blocksize: 32_000_000,
                ..tantivy::IndexSettings::default()
            })
            .create_in_ram()?;

        Ok(SearchIndex { index })
    }

    pub fn builder(&self) -> SearchIndexBuilder {
        let index_writer = self.index.writer(32_000_000).unwrap();

        SearchIndexBuilder {
            writer: index_writer,
            schema: self.index.schema(),
        }
    }

    pub fn add_content(
        &self,
        content: Vec<mwp_content::Page>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let schema = self.index.schema();

        let mut index_writer = self.index.writer(32_000_000).unwrap();

        let title = schema.get_field("title").unwrap();
        let body = schema.get_field("body").unwrap();
        let url = schema.get_field("url").unwrap();
        let tags = schema.get_field("tags").unwrap();

        for page in content {
            let mut document = TantivyDocument::default();
            document.add_text(title, &page.title);
            document.add_text(body, &page.text);
            document.add_text(url, &page.path);
            for tag in page.tags {
                document.add_text(tags, tag);
            }

            index_writer.add_document(document)?;
        }

        index_writer.commit().expect("commit index");

        Ok(())
    }
}

pub struct SearchIndexBuilder {
    schema: Schema,
    writer: IndexWriter,
}

impl SearchIndexBuilder {
    pub fn add(&self, doc: Doc) -> Result<(), Box<dyn std::error::Error>> {
        let schema = &self.schema;

        let title = schema.get_field("title").unwrap();
        let body_field = schema.get_field("body").unwrap();
        let url = schema.get_field("url").unwrap();
        let domain = schema.get_field("domain").unwrap();
        let tags = schema.get_field("tags").unwrap();

        let mut document = TantivyDocument::default();
        document.add_text(title, &doc.title);
        if let Some(body) = doc.body {
            document.add_text(body_field, &body);
        }
        document.add_text(url, &doc.url);
        document.add_text(domain, &doc.domain);
        if let Some(doc_tags) = doc.tags {
            for tag in doc_tags {
                document.add_text(tags, tag);
            }
        }

        self.writer.add_document(document)?;

        Ok(())
    }

    pub fn commit(&mut self) {
        self.writer.commit().expect("commit index");
    }
}
