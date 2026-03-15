use maud::{html, Markup, PreEscaped};
use mwp_content::Node;

const EXPAND_ICON: &str = include_str!("static/expand.svg");
const BURGER_ICON: &str = include_str!("static/burger.svg");

pub fn header(page_title: &str) -> Markup {
    html! {
        meta charset="utf-8";
        meta content="width=device-width,minimum-scale=1" name="viewport";
        title { (page_title) }
        link href="/styles.css" rel="stylesheet";
        link href="/pagefind/pagefind-ui.css" rel="stylesheet";
        script src="/pagefind/pagefind-ui.js" {}
        script type="text/javascript" defer="" src="/script.js" {}
    }
}

fn footer() -> Markup {
    html! {
        footer {
            div {
                "© Matous Dzivjak, 2024 · "
                a href="https://dzx.cz" {
                    "dzx.cz"
                }
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
                .options {
                    .search {
                        button
                            type="button"
                            id="search-open"
                            aria-haspopup="dialog"
                            aria-controls="search-dialog"
                            .search-button {
                            "Search"
                        }
                    }
                    button .burger aria-controls="sidebar" aria-expanded="sidebar" {
                        (PreEscaped(BURGER_ICON))
                    }
                }
            }
            #sidebar { (sidebar) }
            .meta { (meta) }
            main { (content) }
            (footer())
        }
        dialog #search-dialog {
            .search-modal {
                .search-modal__header {
                    h2 { "Search" }
                    button type="button" id="search-close" .search-close {
                        "Close"
                    }
                }
                div id="search" {}
            }
        }
        script {
            (PreEscaped(
                r#"
                window.addEventListener('DOMContentLoaded', function() {
                    new PagefindUI({ element: '#search', showSubResults: true });
                });
                "#
            ))
        }
    }
}

fn slice_tail(hierarchy: &[String]) -> &[String] {
    if hierarchy.len() > 1 {
        &hierarchy[1..]
    } else {
        &[]
    }
}

fn tree_node(node: &Node, hierarchy: &[String]) -> Markup {
    let expanded = hierarchy.iter().any(|name| name == &node.name);
    let tail = slice_tail(hierarchy);

    html! {
        .entry {
            a .active[expanded] href=(node.path) {
                (node.name)
            }
            @if !node.children.is_empty() {
                button aria-controls=(node.name) aria-expanded=(expanded.to_string()) {
                    span .icon {
                        (PreEscaped(EXPAND_ICON))
                    }
                }
                .folder .expanded[expanded] id=(node.name) {
                    @for child in &node.children {
                        (tree_node(child, tail))
                    }
                }
            }
        }
    }
}

pub fn content_navigation(children: &[Node], hierarchy: &[String]) -> Markup {
    let top = slice_tail(hierarchy);
    html! {
        .tree {
            @for child in children {
                (tree_node(child, top))
            }
        }
    }
}
