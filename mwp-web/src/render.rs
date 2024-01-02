use maud::{html, Markup, DOCTYPE};
use tantivy::aggregation::agg_result::BucketEntry;

pub fn header(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="utf-8";
        link rel="stylesheet" href="/styles.css";
        title { (page_title) }
    }
}

pub fn tags(v: Vec<&str>) -> Markup {
    html! {
        div .tags {
            @for tag in v {
                a .tag href={"/tags/" (tag)} {
                    "#" (tag)
                }
            }
        }
    }
}

pub fn url(url: &str) -> Markup {
    html! {
        div .url {
            a href=(url) {
                (url)
            }
        }
    }
}

pub fn layout(sidebar: Markup, content: Markup) -> Markup {
    html! {
        .layout {
            .sidebar {(sidebar)}
            main .content {(content)}
        }
    }
}

pub fn tags_filter(agg: Option<Vec<BucketEntry>>) -> Markup {
    match agg {
        Some(agg) => {
            html! {
                .filter {
                    .filterName { "Tags" }
                    .entries {
                        @for BucketEntry { key, doc_count, .. } in agg {
                            div {
                                a .tag href={"/tags/" (key)} {
                                    "#" (key)
                                }
                            }
                            div { (doc_count) }
                        }
                    }
                }
            }
        }
        None => html! {},
    }
}
