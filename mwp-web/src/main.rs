use std::collections::HashMap;

use actix_files::Files;
use actix_web::{
    get,
    guard::{Guard, GuardContext},
    web, App, HttpServer, Result as AwResult,
};
use maud::{html, Markup, PreEscaped};
use serde::Deserialize;
use tantivy::{
    query::{AllQuery, QueryParser},
    schema::Schema,
    DocAddress, Index, Searcher, SnippetGenerator,
};

mod render;
mod search;

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
                div {
                    div .title {
                        h3 {
                            a href=(url) {
                                (title)
                            }
                        }
                    }
                    (render::url(url))
                    (render::tags(tags))
                }
            }
        }
    }
}

#[get("/")]
async fn index_page(index: web::Data<Index>) -> AwResult<Markup> {
    let schema = index.schema();
    let reader = index.reader().unwrap();
    let searcher = reader.searcher();

    let result = search::search(index.into_inner(), &AllQuery).unwrap();

    Ok(html! {
        html {
            (render::header("MWP"))
            body {
                h1 { "MWP" };
                form method="GET" action="/search" {
                    input type="text" name="query" id="query";
                }
                (render::layout(
                    html! {
                        div { "Sidebar "}
                        div {(render::tags_filter(result.tags))}
                    },
                    listing(searcher, schema, result.docs),
                ))
            }
        }
    })
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    query: String,
}

#[get("/search")]
async fn search_page(q: web::Query<SearchQuery>, index: web::Data<Index>) -> AwResult<Markup> {
    let schema = index.schema();
    let reader = index.reader().unwrap();
    let searcher = reader.searcher();

    let title = schema.get_field("title").unwrap();
    let body = schema.get_field("body").unwrap();
    let tags = schema.get_field("tags").unwrap();
    let query_parser = QueryParser::for_index(&index, vec![title, body]);

    let query = query_parser.parse_query(q.query.as_str()).unwrap();
    let result = search::search(index.into_inner(), &*query).unwrap();

    let snippet_generator = SnippetGenerator::create(&searcher, &*query, body).unwrap();

    Ok(html! {
        html {
            (render::header("Search | MWP"))
            body {
                h1 { "Search for " (q.query) };
                form method="GET" action="/search" {
                    input type="text" name="query" id="query";
                }
                (render::layout(
                    html! {
                        div { "Sidebar "}
                        div {(render::tags_filter(result.tags))}
                    },
                    html! {
                        @for (_score, doc_address) in result.docs {
                            @let doc = searcher.doc(doc_address).unwrap();
                            @let title = doc.get_first(title).unwrap().as_text().unwrap();
                            @let snippet = snippet_generator.snippet_from_doc(&doc);
                            @let tags = doc.get_all(tags).map(|i| i.as_text().unwrap()).collect::<Vec<&str>>();
                            div {
                                div { (title) }
                                div { (PreEscaped(snippet.to_html())) }
                                (render::tags(tags))
                            }
                        }
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

    let result = search::search(index.into_inner(), &*query).unwrap();

    Ok(html! {
        html {
            (render::header("Tags | MWP"))
            body {
                h1 { (tag) };
                (render::layout(
                    html! {
                        div { "Sidebar "}
                        div {(render::tags_filter(result.tags))}
                    },
                    listing(searcher, schema, result.docs)
                ))
            }
        }
    })
}

async fn content_page(
    path: web::Path<Vec<String>>,
    content: web::Data<Content>,
) -> AwResult<Markup> {
    match content
        .docs
        .get(format!("/{}", path.join("/").as_str()).as_str())
    {
        Some(mwp_content::Page { html: content, .. }) => Ok(html! {
            html {
                (render::header("Content | MWP"))
                body {
                    h1 { (path.join(",")) };
                    main {
                        article {
                            (PreEscaped(content))
                        }
                    }
                }
            }
        }),
        None => Ok(html! {
            html {
                (render::header("Not found | MWP"))
                body {
                    h1 { "Not found" };
                }
            }
        }),
    }
}

#[derive(Clone)]
struct Content {
    pub docs: HashMap<String, mwp_content::Page>,
}

struct ContentGuard {
    pub contents: Vec<String>,
}

impl Guard for ContentGuard {
    fn check(&self, req: &GuardContext<'_>) -> bool {
        self.contents.contains(&req.head().uri.path().to_string())
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let index_path = "../index";
    let index = Index::open_in_dir(index_path).unwrap();
    let content = Content {
        docs: mwp_content::read_dir("../../wiki").await,
    };

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(index.clone()))
            .app_data(web::Data::new(content.clone()))
            .service(index_page)
            .service(tag_page)
            .service(search_page)
            .route(
                "/{path:.*}",
                web::get()
                    .guard(ContentGuard {
                        contents: content.docs.keys().cloned().collect(),
                    })
                    .to(content_page),
            )
            .service(Files::new("/", "./static/"))
    })
    .bind(("127.0.0.1", 4444))?
    .run()
    .await
}
