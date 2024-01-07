use std::{
    collections::HashMap,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

use pulldown_cmark::{html, Options, Parser};
use walkdir::WalkDir;

#[derive(Clone)]
pub struct Page {
    pub title: String,
    pub path: String,
    pub html: String,
    pub text: String,
    pub tags: Vec<String>,
    pub hiearchy: String,
}

fn remove_extension(path: &Path) -> PathBuf {
    let mut new_path = PathBuf::new();
    let parent_dir = path.parent().unwrap();
    if let Some(file_name) = path.file_stem() {
        new_path.push(parent_dir);
        new_path.push(file_name);
    }
    new_path
}

pub async fn read_dir(src: &str) -> HashMap<String, Page> {
    let mut entries = HashMap::new();

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);

    for entry in WalkDir::new(src)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| !e.file_type().is_dir())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext.to_str() == Some("md"))
        })
    {
        let contents =
            fs::read_to_string(entry.path()).expect("Something went wrong reading the file");

        let mut text_output = String::new();

        let mut in_heading = false;
        let mut title = String::new();
        #[allow(clippy::unnecessary_filter_map)]
        let parser = Parser::new_ext(&contents, options).filter_map(|event| {
            match event.clone() {
                pulldown_cmark::Event::Text(text) => {
                    write!(&mut text_output, "{}", text).expect("write text output");
                    if in_heading && title.is_empty() {
                        title = text.to_string();
                    }
                }
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Heading(
                    pulldown_cmark::HeadingLevel::H1,
                    _,
                    _,
                )) => {
                    in_heading = true;
                }
                pulldown_cmark::Event::End(pulldown_cmark::Tag::Heading(
                    pulldown_cmark::HeadingLevel::H1,
                    _,
                    _,
                )) => {
                    in_heading = false;
                }
                _ => (),
            }
            Some(event)
        });

        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);

        let clean_path =
            Path::new("/").join(remove_extension(entry.path().strip_prefix(src).unwrap()));

        let parent = clean_path.parent().unwrap();

        let tags = parent
            .iter()
            .filter_map(|component| {
                if *component == Path::new("/") {
                    None
                } else {
                    Some(component.to_str().unwrap().into())
                }
            })
            .collect::<Vec<String>>();

        let clean_path = clean_path.display().to_string();

        entries.insert(
            clean_path.to_owned(),
            Page {
                title,
                path: clean_path,
                html: html_output,
                text: text_output,
                tags,
                hiearchy: parent.display().to_string(),
            },
        );
    }

    entries
}
