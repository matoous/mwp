use maud::{html, Markup, PreEscaped};
use tantivy::aggregation::agg_result::BucketEntry;

const EXPAND_ICON: &str = include_str!("static/expand.svg");

pub fn header(page_title: &str) -> Markup {
    html! {
        meta charset="utf-8";
        title { (page_title) }
        link rel="stylesheet" href="/styles.css";
        script type="text/javascript" defer="" src="/script.js"{}
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

fn tree_node(n: mwp_content::Node, hiearchy: &[String]) -> Markup {
    let expanded = hiearchy.contains(&n.name);
    html! {
        .entry {
            a .active[expanded] href=(n.path) {
                (n.name)
            }
            @if !n.children.is_empty() {
                button aria-controls=(n.name) aria-expanded=(expanded.to_string()) {
                    span .icon {
                        (PreEscaped(EXPAND_ICON))
                    }
                }
                .folder .expanded[expanded] id=(n.name) {
                    @for child in n.children {
                        (tree_node(child, hiearchy.get(1..).unwrap_or_default()))
                    }
                }
            }
        }
    }
}

pub fn content_navigation(children: Vec<mwp_content::Node>, hiearchy: Vec<String>) -> Markup {
    html! {
        .tree {
            @for child in children {
                (tree_node(child, hiearchy.get(1..).unwrap_or_default()))
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
