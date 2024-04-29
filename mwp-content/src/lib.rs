use std::{
    collections::HashMap,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

use pulldown_cmark::{html, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use url::Url;
use walkdir::WalkDir;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub title: String,
    pub url: Url,
    pub starred: bool,
    pub tags: Vec<String>,
}

/// Represents a single page of content in the wiki.
#[derive(Clone, Debug)]
pub struct Page {
    /// The title of the page.
    pub title: String,

    /// The absolute path of the page.
    pub path: String,

    /// The HTML content of the page.
    pub html: String,

    /// The plain text content of the page.
    pub text: String,

    /// A list of tags associated with the page.
    pub tags: Vec<String>,

    /// A list of absolute paths of parents that this page belongs under.
    pub parents: Vec<String>,

    /// A list of links found on the page in (anchor text, url) format.
    pub links: Vec<Link>,
}

impl Page {
    // From parses a page content.
    pub fn from<P: AsRef<Path>>(file_name: P, content: String) -> Self {
        let mut clean_path = file_name.as_ref();
        if clean_path
            .file_name()
            .is_some_and(|name| name == "index" || name == "README")
        {
            clean_path = clean_path.parent().unwrap()
        }

        let tags = clean_path
            .iter()
            .filter_map(|component| {
                if *component == Path::new("/") {
                    None
                } else {
                    Some(component.to_str().unwrap().into())
                }
            })
            .collect::<Vec<String>>();

        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);

        let mut text_output = String::new();

        let mut page_title = String::new();
        let mut link_title = String::new();
        let mut links: Vec<Link> = Vec::new();
        let mut open_tags: Vec<Tag> = Vec::new();

        #[allow(clippy::unnecessary_filter_map)]
        let parser = Parser::new_ext(&content, options).filter_map(|event| match event.clone() {
            Event::Text(text) => {
                if page_title.is_empty()
                    && open_tags.last().is_some_and(|tag| {
                        matches!(
                            tag,
                            Tag::Heading {
                                level: HeadingLevel::H1,
                                ..
                            }
                        )
                    })
                {
                    page_title = text.to_string();
                }

                if open_tags
                    .last()
                    .is_some_and(|tag| matches!(tag, Tag::Link { .. }))
                {
                    link_title = text.to_string();
                }

                Some(event)
            }
            Event::Start(tag) => {
                open_tags.push(tag);
                Some(event)
            }
            Event::End(TagEnd::Heading(..) | TagEnd::Paragraph | TagEnd::Item) => {
                writeln!(&mut text_output).expect("write text output");
                open_tags.pop();
                Some(event)
            }
            Event::End(TagEnd::Link) => {
                if let Some(Tag::Link { dest_url, .. }) = open_tags.pop() {
                    if let Ok(url) = Url::parse(&dest_url) {
                        links.push(Link {
                            title: link_title.clone(),
                            url,
                            starred: open_tags.iter().any(|tag| matches!(tag, Tag::Strong)),
                            tags: tags.clone(),
                        });
                        link_title.clear();
                    }
                }
                Some(event)
            }
            event => Some(event),
        });

        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);

        let mut parents: Vec<String> = Vec::with_capacity(tags.len());
        let mut link = String::with_capacity(clean_path.display().to_string().len());

        parents.push("/".into());
        for parent in tags.iter() {
            link.push('/');
            link.push_str(parent.as_str());
            parents.push(link.clone());
        }

        let clean_path = clean_path.display().to_string();

        Page {
            title: page_title,
            tags: tags.clone(),
            links,
            html: html_output,
            text: text_output,
            path: clean_path,
            parents,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Content {
    pages: HashMap<String, Page>,
}

#[derive(Debug)]
pub struct Node {
    pub name: String,
    pub path: String,
    pub children: Vec<Node>,
}

impl Content {
    pub async fn from_dir(src: &str) -> Self {
        let mut pages = HashMap::new();

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

            let file_name = remove_extension(
                Path::new("/")
                    .join(entry.path().strip_prefix(src).unwrap())
                    .as_path(),
            );

            let page = Page::from(&file_name, contents);

            pages.insert(page.path.clone(), page);
        }

        Self { pages }
    }

    pub fn build_tree(&self) -> Vec<Node> {
        self.build_tree_impl(self.pages_under_path(""))
    }

    fn build_tree_impl(&self, pages: Vec<&Page>) -> Vec<Node> {
        pages
            .iter()
            .map(|p| Node {
                name: p.title.clone(),
                path: p.path.clone(),
                children: self.build_tree_impl(self.pages_under_path(&p.path)),
            })
            .collect()
    }

    pub fn get(&self, path: &str) -> Option<&Page> {
        self.pages.get(path)
    }

    // TODO: replace with iterator
    pub fn all(&self) -> &HashMap<String, Page> {
        &self.pages
    }

    pub fn pages_under_path(&self, path: &str) -> Vec<&Page> {
        let target_depth = path_depth(path) + 1;
        let mut pages: Vec<&Page> = self
            .pages
            .values()
            .filter(|p| p.path != "/") // skip index file
            .filter(|p| p.path.starts_with(path)) // match only files under path
            .filter(|p| path_depth(&p.path) == target_depth) // match only first layer under path
            .collect();
        pages.sort_by_key(|p| p.title.as_str());
        pages
    }

    pub fn keys(&self) -> Vec<String> {
        self.pages.keys().cloned().collect()
    }

    pub fn values(&self) -> Vec<Page> {
        self.pages.values().cloned().collect()
    }
}

#[inline(always)]
fn path_depth(s: &str) -> usize {
    s.chars().filter(|c| *c == '/').count()
}

#[inline(always)]
fn remove_extension(path: &Path) -> PathBuf {
    let mut new_path = PathBuf::new();
    let parent_dir = path.parent().unwrap();
    if let Some(file_name) = path.file_stem() {
        new_path.push(parent_dir);
        new_path.push(file_name);
    }
    new_path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_from() {
        let page = Page::from(
            "/",
            r#"
Links from text should be [included](https://included.com).

## Resources

- [test](https://test.com)

## Other links

- [other](https://other.com)
- **[starred](https://starred.com)**
            "#
            .into(),
        );

        assert_eq!(
            page.links,
            vec![
                Link {
                    title: "included".into(),
                    url: Url::parse("https://included.com").unwrap(),
                    starred: false,
                    tags: Vec::new(),
                },
                Link {
                    title: "test".into(),
                    url: Url::parse("https://test.com").unwrap(),
                    starred: false,
                    tags: Vec::new(),
                },
                Link {
                    title: "other".into(),
                    url: Url::parse("https://other.com").unwrap(),
                    starred: false,
                    tags: Vec::new(),
                },
                Link {
                    title: "starred".into(),
                    url: Url::parse("https://starred.com").unwrap(),
                    starred: true,
                    tags: Vec::new(),
                },
            ]
        );
    }
}
