use actix_files::Files;
use actix_web::{
    get,
    guard::{Guard, GuardContext},
    web, App, HttpServer, Result as AwResult,
};
use actix_web_static_files::ResourceFiles;
use clap::{command, Parser};
use maud::{html, Markup, PreEscaped, DOCTYPE};
use mwp_content::Content;
use mwp_search::{Doc, SearchIndex};
use rusqlite::Connection;
use serde::Deserialize;
use tantivy::{query::QueryParser, schema::Schema, DocAddress, Index, Searcher};

mod render;
mod search;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

fn listing(searcher: Searcher, schema: Schema, docs: Vec<(f32, DocAddress)>) -> Markup {
    let title = schema.get_field("title").unwrap();
    let tags = schema.get_field("tags").unwrap();
    let url = schema.get_field("url").unwrap();

    html! {
        div .listing {
            @for (_score, doc_address) in docs {
                @let doc = searcher.doc(doc_address).unwrap();
                @let title = doc.get_first(title).unwrap().as_text().unwrap();
                @let url = doc.get_first(url).unwrap().as_text().unwrap();
                @let tags = doc.get_all(tags).map(|i| i.as_text().unwrap()).collect::<Vec<&str>>();
                (render::link(title, url, tags))
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    query: String,
    page: Option<usize>,
}

#[get("/search")]
async fn search_page(q: web::Query<SearchQuery>, index: web::Data<Index>) -> AwResult<Markup> {
    let schema = index.schema();
    let reader = index.reader().unwrap();
    let searcher = reader.searcher();

    let title = schema.get_field("title").unwrap();
    let body = schema.get_field("body").unwrap();
    let query_parser = QueryParser::for_index(&index, vec![title, body]);

    let query = query_parser.parse_query(q.query.as_str()).unwrap();
    let result = search::search(index.into_inner(), &*query, q.page.unwrap_or(0)).unwrap();

    Ok(html! {
        (DOCTYPE)
        html {
            (render::header("Search | Matt's Wiki"))
            body {
                (render::layout(
                    html! {
                        div {(render::tags_filter(result.tags))}
                    },
                    html! {
                        .metadata {
                            div {"Search: " b{(q.query)}}
                            div {"·"}
                            div {(format!("{} results in {:.2?}", result.count, result.timing))}
                        }
                    },
                    html! {
                        (listing(searcher, schema, result.docs))
                    }
                ))
            }
        }
    })
}

#[get("/tags/{tag}")]
async fn tag_page(tag: web::Path<String>, index: web::Data<Index>) -> AwResult<Markup> {
    let reader = index.reader().unwrap();
    let searcher = reader.searcher();

    let schema = index.schema();
    let tags = schema.get_field("tags").unwrap();
    let query_parser = QueryParser::for_index(&index, vec![tags]);
    let query = query_parser.parse_query(tag.as_str()).unwrap();

    let result = search::search(index.into_inner(), &*query, 0).unwrap();

    Ok(html! {
        (DOCTYPE)
        html {
            (render::header("Tags | Matt's Wiki"))
            body {
                (render::layout(
                    html! {
                        div {(render::tags_filter(result.tags))}
                    },
                    html! {
                        .metadata {
                            div {"Tag: " b{(tag)}}
                            div {"·"}
                            div {(format!("{} results in {:.2?}", result.count, result.timing))}
                        }
                    },
                    listing(searcher, schema, result.docs)
                ))
            }
        }
    })
}

async fn content_page(path: web::Path<String>, content: web::Data<Content>) -> AwResult<Markup> {
    let mwp_content::Page {
        title,
        html,
        parents,
        ..
    } = content.get(format!("/{}", path.as_str()).as_str()).unwrap();

    let mut hiearchy: Vec<(String, &String)> = Vec::with_capacity(parents.len());
    for parent in parents {
        if parent == "/" {
            hiearchy.push(("Wiki".into(), parent));
        } else {
            hiearchy.push((content.get(parent).unwrap().title.clone(), parent));
        }
    }

    Ok(html! {
        (DOCTYPE)
        html {
            (render::header(format!("{} | Matt's Wiki", title).as_str()))
            body {
                (render::layout(
                    html! {
                        (render::content_navigation(
                            content.build_tree(),
                            hiearchy.iter().map(|(name, _)| name.clone()).collect(),
                        ))
                    },
                    html! {
                        ol .hiearchy {
                            @for (name, link) in hiearchy {
                                li {
                                    @if link != "/" {
                                        span .separator {
                                            "/"
                                        }
                                    }
                                    a href=(link) {
                                        (name)
                                    }
                                }
                            }
                        }
                    },
                    html! {
                        article { (PreEscaped(html)) }
                    }
                ))
            }
        }
    })
}

struct ContentGuard {
    pub contents: Vec<String>,
}

impl Guard for ContentGuard {
    fn check(&self, req: &GuardContext<'_>) -> bool {
        self.contents.contains(&req.head().uri.path().to_string())
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Source of the wikipedia
    #[arg(short, long, default_value = "./wiki")]
    src: String,

    /// The database file
    #[arg(short, long, default_value = "./db.db3")]
    db: String,

    /// Address to serve on
    #[arg(long, default_value = "127.0.0.1:4444")]
    adr: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let args = Args::parse();

    let index = SearchIndex::new().unwrap();

    let conn = Connection::open(args.db).unwrap();
    let mut stmt = conn
        .prepare("SELECT title, url, domain, body, tags, created_at, scraped_at FROM links")
        .unwrap();
    let docs_iter = stmt
        .query_map([], |row| {
            Ok(Doc {
                title: row.get(0)?,
                url: row.get(1)?,
                domain: row.get(2)?,
                body: row.get(3)?,
                tags: row.get::<usize, Option<String>>(4).map(|res| {
                    res.map(|s| s.split(';').map(|s| s.into()).collect::<Vec<String>>())
                })?,
                created_at: row.get(5)?,
                scraped_at: row.get(6)?,
            })
        })
        .unwrap();

    let mut builder = index.builder();
    for doc in docs_iter {
        builder.add(doc.unwrap()).unwrap();
    }
    builder.commit();

    let content = Content::from_dir(&args.src).await;

    HttpServer::new(move || {
        let generated = generate();
        App::new()
            .app_data(web::Data::new(index.index.clone()))
            .app_data(web::Data::new(content.clone()))
            .service(tag_page)
            .service(search_page)
            .route(
                "/{path:.*}",
                web::get()
                    .guard(ContentGuard {
                        contents: content.keys(),
                    })
                    .to(content_page),
            )
            .service(Files::new("/static", format!("{}/static", &args.src)))
            .service(ResourceFiles::new("/", generated))
    })
    .bind(&args.adr)?
    .run()
    .await
}
