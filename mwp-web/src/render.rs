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

fn tags_list(v: Vec<&str>) -> Markup {
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

fn footer() -> Markup {
    html! {
        footer {
            div {
                "© Matous Dzivjak, 2024"
            }
            div {
                "Software Engineer @ SumUp"
            }
            address {
                "Berlin, Germany · Litomerice, Czech Republic"
            }
            div {
                "matousdzivjak@gmail.com · GitHub · Keybase · LinkedIn · Instagram"
            }
        }
    }
}

pub fn layout(sidebar: Markup, meta: Markup, content: Markup) -> Markup {
    html! {
        .layout {
            .nav {
                a .logo href="/" {
                    "Matt's Wiki_"
                }
                .search {
                    form method="GET" action="/search" {
                        input type="search" name="query" id="query" placeholder="Search..." accesskey="f";
                    }
                }
            }
            .sidebar {(sidebar)}
            .meta {(meta)}
            main {(content)}
            (footer())
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

fn tree_node(n: mwp_content::Node) -> Markup {
    html! {
        .entry {
            a href=(n.path) {
                (n.name)
            }
            @if !n.children.is_empty() {
                .folder {
                    @for child in n.children {
                        (tree_node(child))
                    }
                }
            }
        }
    }
}

pub fn content_navigation(children: Vec<mwp_content::Node>) -> Markup {
    html! {
        .tree {
            @for child in children {
                (tree_node(child))
            }
        }
    }
}

pub fn link(title: &str, url: &str, tags: Vec<&str>) -> Markup {
    html! {
        div .link {
            div .title {
                @if !url.starts_with('/') {
                    "↗ "
                }
                a href=(url) {
                    (title)
                }
            }
            div .url {
                a href=(url) {
                    (url)
                }
            }
            (tags_list(tags))
        }
    }
}
