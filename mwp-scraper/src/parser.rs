use std::cell::RefCell;
use std::collections::HashMap;
use std::default::Default;
use std::rc::Rc;

use lazy_static::lazy_static;
use lol_html::{element, text, HtmlRewriter, Settings};
use regex::Regex;

lazy_static! {
    static ref ALL_SPACES: Regex = Regex::new("\\s").unwrap();
    static ref SENTENCE_CHARS: Regex = Regex::new("[\\w'\"\\)\\$\\*]").unwrap();
    static ref ATTRIBUTE_MATCH: Regex =
        Regex::new("^\\s*(?P<name>[^:\\[\\]]+)\\[(?P<attribute>.+)\\]\\s*$").unwrap();
}

lazy_static! {
    static ref NEWLINES: Regex = Regex::new("(\n|\r\n)+").unwrap();
    static ref TRIM_NEWLINES: Regex = Regex::new("^[\n\r\\s]+|[\n\r\\s]+$").unwrap();
    static ref EXTRANEOUS_SPACES: Regex = Regex::new("\\s{2,}").unwrap();
    // TODO: i18n?
    static ref SPECIAL_CHARS: Regex = Regex::new("[^\\w]").unwrap();
}

const SENTENCE_SELECTORS: &[&str] = &[
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "p",
    "td",
    "div",
    "ul",
    "li",
    "article",
    "section",
    "blockquote",
    "details",
];

const INLINE_SELECTORS: &[&str] = &[
    "a", "abbr", "acronym", "b", "bdo", "big", "br", "button", "cite", "code", "dfn", "em", "i",
    "img", "input", "kbd", "label", "map", "object", "output", "q", "samp", "script", "select",
    "small", "span", "strong", "sub", "sup", "textarea", "time", "tt", "var",
];

const REMOVE_SELECTORS: &[&str] = &[
    "head", "style", "script", "noscript", "label", "form", "svg", "footer", "nav", "iframe",
    "template",
];

const SPACE_SELECTORS: &[&str] = &["br"];

// We aren't transforming HTML, just parsing, so we dump the output.
#[derive(Default)]
struct EmptySink;
impl lol_html::OutputSink for EmptySink {
    fn handle_chunk(&mut self, _: &[u8]) {}
}

/// Houses the HTML parsing instance and the internal data while parsing
pub struct DomParser<'a> {
    rewriter: HtmlRewriter<'a, EmptySink>,
    data: Rc<RefCell<DomParserData>>,
}

// The internal state while parsing,
// with a reference to the deepest HTML element
// that we're currently reading
#[derive(Default, Debug)]
struct DomParserData {
    current_node: Rc<RefCell<DomParsingNode>>,
    meta: HashMap<String, String>,
    language: Option<String>,
    has_html_element: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum NodeStatus {
    Indexing,
    // Our content & children should not be indexed
    Ignored,
    Body,
    // There was a body element below us,
    // so our content should be ignored.
    ParentOfBody,
}

impl Default for NodeStatus {
    fn default() -> Self {
        Self::Indexing
    }
}

// A single HTML element that we're reading into.
// Contains a reference to the parent element,
// and since we collapse this tree upwards while we parse,
// we don't need to store tree structure.
#[derive(Default, Debug)]
struct DomParsingNode {
    current_value: String,
    parent: Option<Rc<RefCell<DomParsingNode>>>,
    status: NodeStatus,
}

/// The search-relevant data that was retrieved from the given input
#[derive(Debug)]
pub struct DomParserResult {
    pub title: String,
    pub digest: String,
    pub meta: HashMap<String, String>,
    pub has_html_element: bool,
    pub language: String,
}

// Some shorthand to clean up our use of Rc<RefCell<*>> in the lol_html macros
// From https://github.com/rust-lang/rfcs/issues/2407#issuecomment-385291238
macro_rules! enclose {
    ( ($( $x:ident ),*) $y:expr ) => {
        {
            $(let $x = $x.clone();)*
            $y
        }
    };
}

impl<'a> DomParser<'a> {
    pub fn new() -> Self {
        let data = Rc::new(RefCell::new(DomParserData::default()));
        let root_selector = "html";
        let root = format!("{}, {} *", root_selector, root_selector);
        let exclusions = REMOVE_SELECTORS
            .iter()
            .map(|s| s.to_string())
            .map(|e| format!("{} {}", root_selector, e))
            .collect::<Vec<_>>()
            .join(", ");

        let rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: vec![
                    enclose! { (data) element!("html", move |el| {
                        let mut data = data.borrow_mut();
                        data.has_html_element = true;
                        if let Some(lang) = el.get_attribute("lang") {
                            data.language = Some(lang.to_lowercase());
                        }
                        Ok(())
                    })},
                    enclose! { (data) element!(root, move |el| {
                        let tag_name = el.tag_name();

                        // Handle adding spaces between words separated by <br/> tags and the like
                        if SPACE_SELECTORS.contains(&el.tag_name().as_str()) {
                            let parent = &data.borrow().current_node;
                            let mut parent = parent.borrow_mut();
                            parent.current_value.push(' ');
                        }

                        let node = {
                            let mut data = data.borrow_mut();
                            let parent_node = data.current_node.borrow();
                            let parent_status = parent_node.status;

                            let node = Rc::new(RefCell::new(DomParsingNode{
                                parent: Some(Rc::clone(&data.current_node)),
                                status: parent_status,
                                current_value: String::default(),
                            }));

                            drop(parent_node);
                            data.current_node = Rc::clone(&node);
                            node
                        };

                        if let Some(handlers) = el.end_tag_handlers() {
                            let data = data.clone();
                            let node = node.clone();
                            let tag_name = tag_name.clone();
                            handlers.push(Box::new(move |end| {
                                let mut data = data.borrow_mut();
                                let mut node = node.borrow_mut();

                                // When we reach an end tag, we need to
                                // make sure to move focus back to the parent node.
                                if let Some(parent) = &node.parent {
                                    data.current_node = Rc::clone(parent);
                                }

                                // Try to capture the first title on the page (if unset)
                                if tag_name == "h1" && !data.meta.contains_key("auto_title") && !node.current_value.trim().is_empty() {
                                    data.meta.insert("auto_title".into(), normalize_content(&node.current_value));
                                }
                                // Try to capture the actual title of the page as a fallback for later
                                if tag_name == "title" && !data.meta.contains_key("auto_page_title") {
                                    data.meta.insert("auto_page_title".into(), normalize_content(&node.current_value));
                                }

                                let tag_name = end.name();
                                if SENTENCE_SELECTORS.contains(&tag_name.as_str()) {
                                    // For block elements, we want to make sure sentences
                                    // don't hug each other without whitespace.
                                    // We normalize repeated whitespace later, so we
                                    // can add this indiscriminately.
                                    node.current_value.insert(0, ' ');

                                    // Similarly, we want to separate block elements
                                    // with punctuation, so that the excerpts read nicely.
                                    // (As long as it doesn't already end with, say, a . or ?)
                                    if node.current_value.chars()
                                        .last()
                                        .filter(|c| SENTENCE_CHARS.is_match(&c.to_string()))
                                        .is_some() {
                                            node.current_value.push('.');
                                    }
                                    node.current_value.push(' ');
                                }

                                // Huck all of the content we have onto the end of the
                                // content that the parent node has (so far)
                                // This will include all of our children's content,
                                // and the order of tree traversal will mean that it
                                // is inserted in the correct position in the parent's content.
                                let mut parent = data.current_node.borrow_mut();

                                // If the parent is a parent of a body, we don't want to append
                                // any more content to it. (Unless, of course, we are representing another body)
                                if parent.status == NodeStatus::ParentOfBody
                                    && node.status != NodeStatus::Body
                                    && node.status != NodeStatus::ParentOfBody {
                                        return Ok(());
                                }
                                match node.status {
                                    NodeStatus::Ignored => {},
                                    NodeStatus::Indexing => {
                                        parent.current_value.push_str(&node.current_value);
                                    },
                                    NodeStatus::Body | NodeStatus::ParentOfBody => {
                                        // If our parent is already a parent of a body, then
                                        // we're probably a subsequent body. Avoid clearing it out.
                                        if parent.status != NodeStatus::ParentOfBody {
                                            parent.current_value.clear();
                                        }
                                        parent.current_value.push_str(&node.current_value);
                                        parent.status = NodeStatus::ParentOfBody;
                                    }
                                };

                                Ok(())
                            }))
                        };

                        // Try to handle tags like <img /> which have no end tag,
                        // and thus will never hit the logic to reset the current node.
                        // TODO: This could still be missed for tags with implied ends?
                        if !el.can_have_content(){
                            let mut data = data.borrow_mut();
                            let node = node.borrow();
                            if let Some(parent) = &node.parent {
                                data.current_node = Rc::clone(parent);
                            }


                            // Try to capture the first image _after_ a title (if unset)
                            if tag_name == "img"
                                && !data.meta.contains_key("auto_image")
                                && (data.meta.contains_key("auto_title") || data.meta.contains_key("title")) {
                                if let Some(src) = el.get_attribute("src") {
                                data.meta.insert("auto_image".into(), src);

                                    if let Some(alt) = el.get_attribute("alt") {
                                        data.meta.insert("auto_image_alt".into(), alt);
                                    }
                                }
                            }
                        }
                        Ok(())
                    })},
                    // If we hit a selector that should be excluded, mark whatever the current node is as such
                    enclose! { (data) element!(exclusions, move |_el| {
                        let data = data.borrow_mut();
                        let mut node = data.current_node.borrow_mut();
                        node.status = NodeStatus::Ignored;
                        Ok(())
                    })},
                    // Slap any text we encounter inside the body into the current node's current value
                    enclose! { (data) text!(root_selector, move |el| {
                        let data = data.borrow_mut();
                        let mut node = data.current_node.borrow_mut();
                        let element_text = el.as_str();
                        node.current_value.push_str(element_text);
                        Ok(())
                    })},
                ],
                strict: false,
                ..Settings::default()
            },
            EmptySink,
        );

        Self { rewriter, data }
    }

    /// Writes a chunk of data to the underlying HTML parser
    pub fn write(&mut self, data: &[u8]) -> Result<(), lol_html::errors::RewritingError> {
        self.rewriter.write(data)
    }

    /// Performs any post-processing and returns the summated search results
    pub fn wrap(self) -> DomParserResult {
        drop(self.rewriter); // Clears the extra Rcs on and within data
        let mut data = Rc::try_unwrap(self.data).unwrap().into_inner();
        let mut node = data.current_node;

        // Fallback: If we are left with a tree, collapse it up into the parents
        // until we get to the root node.
        while node.borrow().parent.is_some() {
            {
                let node = node.borrow();
                let mut parent = node.parent.as_ref().unwrap().borrow_mut();
                if parent.status != NodeStatus::ParentOfBody {
                    match node.status {
                        NodeStatus::Ignored => {}
                        NodeStatus::Indexing => {
                            parent.current_value.push_str(&node.current_value);
                        }
                        NodeStatus::Body | NodeStatus::ParentOfBody => {
                            parent.current_value.clear();
                            parent.current_value.push_str(&node.current_value);
                            parent.status = NodeStatus::ParentOfBody;
                        }
                    };
                }
            }
            let old_node = node.borrow();
            let new_node = Rc::clone(old_node.parent.as_ref().unwrap());
            drop(old_node);
            node = new_node;
        }

        if let Some(image) = data.meta.remove("auto_image") {
            let alt = data.meta.remove("auto_image_alt").unwrap_or_default();
            if !data.meta.contains_key("image") {
                data.meta.insert("image".into(), image);
                data.meta.insert("image_alt".into(), alt);
            }
        }

        if let Some(title) = data.meta.remove("auto_title") {
            if !data.meta.contains_key("title") {
                data.meta.insert("title".into(), title);
            }
        }
        if let Some(title) = data.meta.remove("auto_page_title") {
            if !data.meta.contains_key("title") {
                data.meta.insert("title".into(), title);
            }
        }

        let title = data.meta.get("title").cloned().unwrap_or_default();

        let node = node.borrow();

        DomParserResult {
            title,
            digest: normalize_content(&node.current_value),
            meta: data.meta,
            has_html_element: data.has_html_element,
            language: data.language.unwrap_or("unknown".into()),
        }
    }
}

fn normalize_content(content: &str) -> String {
    let content = html_escape::decode_html_entities(content);
    let content = TRIM_NEWLINES.replace_all(&content, "");
    let content = NEWLINES.replace_all(&content, " ");
    let content = EXTRANEOUS_SPACES.replace_all(&content, " ");

    content.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_raw_parse(input: Vec<&'static str>) -> DomParserResult {
        let mut rewriter = DomParser::new();
        for line in input {
            let _ = rewriter.write(line.as_bytes());
        }
        rewriter.wrap()
    }

    fn test_parse(mut input: Vec<&'static str>) -> DomParserResult {
        input.insert(0, "<html><body>");
        input.push("</body></html>");
        test_raw_parse(input)
    }

    #[test]
    fn block_tag_formatting() {
        let data = test_parse(vec![
            "<p>Sentences should have periods</p>",
            "<p>Unless one exists.</p>",
            "<div>Or it ends with punctuation:</div>",
            "<article>Except for 'quotes'</article>",
        ]);

        assert_eq!(
            data.digest,
            "Sentences should have periods. Unless one exists. Or it ends with punctuation: Except for 'quotes'."
        )
    }

    #[test]
    fn inline_tag_formatting() {
        let data = test_parse(vec![
            "<p>Inline tags like <span>span</span>",
            " and <b>bol",
            "d</b> shouldn't have periods</p>",
            "<p>And should n<i>o</i>t add any space.</p>",
        ]);

        assert_eq!(
            data.digest,
            "Inline tags like span and bold shouldn't have periods. And should not add any space."
        )
    }

    #[test]
    fn ignored_elements() {
        let data = test_parse(vec![
            "<p>Elements like:</p>",
            "<form>Should <b>not</b> be indexed</form>",
            "<p>forms</p>",
            "*crickets*</div>",
        ]);

        assert_eq!(data.digest, "Elements like: forms. *crickets*");
    }
}
