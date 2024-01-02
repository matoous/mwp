use std::fs;

#[derive(Debug)]
pub struct Link {
    pub url: String,
    pub facet: String,
}

pub fn collect_links() -> Vec<Link> {
    use pulldown_cmark::{Event, Parser, Tag};
    use walkdir::WalkDir;

    let mut links = Vec::new();
    let base = "../wiki";

    for entry in WalkDir::new(base).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && path.extension().unwrap_or_default() == "md" {
            println!("{}", path.display());

            let file = fs::read_to_string(path).unwrap();

            let facet = path
                .strip_prefix(base)
                .unwrap()
                .parent()
                .unwrap()
                .to_str()
                .unwrap();

            let mut in_code_block = false;

            for event in Parser::new(file.as_str()) {
                match event {
                    Event::Start(Tag::CodeBlock(_)) => in_code_block = true,
                    Event::End(Tag::CodeBlock(_)) => in_code_block = false,
                    Event::Start(Tag::Link(_, url, _)) => {
                        if !in_code_block {
                            links.push(Link {
                                url: url.to_string(),
                                facet: facet.into(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    links
}
