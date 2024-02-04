use actix_files::Files;
use actix_web::{
    get,
    guard::{Guard, GuardContext},
    web, App, HttpServer, Result as AwResult,
};
use maud::{html, Markup, PreEscaped};
use mwp_content::Content;
use serde::Deserialize;
use tantivy::{
    query::{AllQuery, QueryParser, TermQuery},
    schema::{IndexRecordOption, Schema},
    DocAddress, Index, Searcher, Term,
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
        html {
            (render::header("Search | Matt's Wiki"))
            body {
                (render::layout(
                    html! {
                        div {(render::tags_filter(result.tags))}
                    },
                    html! {
                        div {(format!("{:.2?}", result.timing))}
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
        html {
            (render::header("Tags | Matt's Wiki"))
            body {
                (render::layout(
                    html! {
                        div {(render::tags_filter(result.tags))}
                   },
                    listing(searcher, schema, result.docs)
                ))
            }
        }
    })
}

async fn content_page(
    path: web::Path<String>,
    content: web::Data<Content>,
    index: web::Data<Index>,
) -> AwResult<Markup> {
    let mwp_content::Page {
        title,
        html,
        tags,
        parents,
        ..
    } = content.get(format!("/{}", path.as_str()).as_str()).unwrap();

    let schema = index.schema();
    let reader = index.reader().unwrap();
    let searcher = reader.searcher();

    let tags_field = schema.get_field("tags").unwrap();

    let result = match tags.last() {
        Some(tag) => {
            let query = TermQuery::new(
                Term::from_field_text(tags_field, tag),
                IndexRecordOption::Basic,
            );
            search::search(index.into_inner(), &query, 0).unwrap()
        }
        None => search::search(index.into_inner(), &AllQuery, 0).unwrap(),
    };

    let mut hiearchy: Vec<(String, &String)> = Vec::with_capacity(parents.len());
    for parent in parents {
        if parent == "/" {
            hiearchy.push(("Wiki".into(), parent));
        } else {
            hiearchy.push((content.get(parent).unwrap().title.clone(), parent));
        }
    }

    Ok(html! {
        html {
            (render::header(format!("{} | Matt's Wiki", title).as_str()))
            body {
                (render::layout(
                    html! {
                        (render::content_navigation(content.build_tree()))
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
                        article { (PreEscaped(html)) }
                        .links {
                            (listing(searcher, schema, result.docs))
                        }
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let index_path = "./index";
    let index = Index::open_in_dir(index_path).unwrap();
    let content = Content::from_dir("../wiki").await;

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(index.clone()))
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
            .service(Files::new("/", "./mwp-web/static/"))
    })
    .bind(("127.0.0.1", 4444))?
    .run()
    .await
}
