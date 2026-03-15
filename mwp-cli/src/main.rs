#![forbid(unsafe_code)]

mod render;

use std::{
    collections::HashMap,
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use actix_files::Files;
use actix_web::{App, HttpServer};
use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Args, Parser, Subcommand, ValueHint};
use futures::{StreamExt, stream};
use grass::OutputStyle;
use html_escape::encode_safe;
use indicatif::{ProgressBar, ProgressStyle};
use maud::{DOCTYPE, PreEscaped, html};
use mwp_content::{Content, Node, Page};
use pagefind::api::PagefindIndex;
use pagefind::options::PagefindServiceConfig;
use pulldown_cmark::{Event, Options, Parser as MarkdownParser, Tag, TagEnd};
use reqwest::{
    Client, StatusCode,
    header::{ETAG, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED},
};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use url::Url;
use walkdir::{DirEntry, WalkDir};

const IGNORED_DIRECTORIES: &[&str] = &[
    ".cargo",
    ".git",
    ".github",
    ".mwp-cache",
    ".obsidian",
    "dist",
    "mwp-cli",
    "public",
    "static",
    "target",
    "vendor",
];

const MAX_FETCH_ATTEMPTS: usize = 3;

#[derive(Parser, Debug)]
#[command(
    name = "mwp",
    version,
    about = "Build and serve a static markdown wiki"
)]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Subcommand, Debug)]
enum CommandKind {
    /// Render wiki markdown into a fully static website
    Build(BuildArgs),
    /// Generate the static Pagefind search bundle from linked pages
    Index(IndexArgs),
    /// Serve a built static site locally
    Serve(ServeArgs),
}

#[derive(Args, Debug)]
struct BuildArgs {
    /// Root directory that contains the wiki markdown files
    #[arg(long, value_hint = ValueHint::DirPath, default_value = ".")]
    root: Utf8PathBuf,

    /// Output directory for the generated site
    #[arg(long, value_hint = ValueHint::DirPath, default_value = "dist")]
    output: Utf8PathBuf,

    /// Maximum number of concurrent HTTP downloads when building Pagefind
    #[arg(long, default_value_t = 10)]
    concurrency: usize,

    /// Directory for caching fetched remote pages
    #[arg(long, value_hint = ValueHint::DirPath, default_value = ".mwp-cache")]
    cache_dir: Utf8PathBuf,

    /// Revalidate cached pages older than this many hours
    #[arg(long, default_value_t = 168)]
    cache_ttl_hours: u64,

    /// Do not perform any network requests, use cached pages only
    #[arg(long, default_value_t = false)]
    offline: bool,
}

#[derive(Args, Debug)]
struct IndexArgs {
    /// Root directory that contains the wiki markdown files
    #[arg(long, value_hint = ValueHint::DirPath, default_value = ".")]
    root: Utf8PathBuf,

    /// Output directory for the generated Pagefind bundle
    #[arg(long, value_hint = ValueHint::DirPath, default_value = "dist/pagefind")]
    output: Utf8PathBuf,

    /// Maximum number of concurrent HTTP downloads
    #[arg(long, default_value_t = 10)]
    concurrency: usize,

    /// Directory for caching fetched remote pages
    #[arg(long, value_hint = ValueHint::DirPath, default_value = ".mwp-cache")]
    cache_dir: Utf8PathBuf,

    /// Revalidate cached pages older than this many hours
    #[arg(long, default_value_t = 168)]
    cache_ttl_hours: u64,

    /// Do not perform any network requests, use cached pages only
    #[arg(long, default_value_t = false)]
    offline: bool,
}

#[derive(Args, Debug)]
struct ServeArgs {
    /// Directory with the built static site
    #[arg(long, value_hint = ValueHint::DirPath, default_value = "dist")]
    dir: Utf8PathBuf,

    /// Address to serve on
    #[arg(long, default_value = "127.0.0.1:4444")]
    addr: String,
}

#[derive(Debug, Clone)]
struct FetchSettings {
    cache_dir: Utf8PathBuf,
    cache_ttl: Duration,
    offline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    url: String,
    fetched_at_epoch_s: u64,
    etag: Option<String>,
    last_modified: Option<String>,
    title_hint: String,
}

#[derive(Debug, Clone)]
struct CachePaths {
    body: Utf8PathBuf,
    meta: Utf8PathBuf,
}

#[derive(Debug, Clone)]
enum FetchOutcome {
    Fresh(DownloadedPage),
    Revalidated(DownloadedPage),
    Cached(DownloadedPage),
}

#[derive(Debug, Clone)]
struct WikiLink {
    title: String,
    url: Url,
    tags: Vec<String>,
    source_path: Utf8PathBuf,
    starred: bool,
}

#[derive(Debug, Clone)]
struct DownloadedPage {
    link: WikiLink,
    html: String,
    etag: Option<String>,
    last_modified: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        CommandKind::Build(args) => run_build(args).await?,
        CommandKind::Index(args) => run_index(args).await?,
        CommandKind::Serve(args) => run_serve(args).await?,
    }

    Ok(())
}

async fn run_build(args: BuildArgs) -> Result<()> {
    let wiki_root = absolute_path(&args.root)?;
    let output_dir = absolute_path(&args.output)?;
    let fetch = FetchSettings {
        cache_dir: absolute_path(&args.cache_dir)?,
        cache_ttl: Duration::from_secs(args.cache_ttl_hours.saturating_mul(60 * 60)),
        offline: args.offline,
    };

    info!(root = %wiki_root, output = %output_dir, "rendering wiki to static html");

    fs::create_dir_all(output_dir.as_std_path())
        .with_context(|| format!("failed to create output dir {}", output_dir))?;

    let content = Content::from_dir(wiki_root.as_str()).await;
    let navigation_tree = content.build_tree();
    let mut pages = content.values();
    pages.sort_by(|a, b| a.path.cmp(&b.path));

    if pages.is_empty() {
        bail!("no pages discovered under {}", wiki_root);
    }

    for page in pages {
        let rendered = render_page(&content, navigation_tree.as_slice(), &page);
        write_page(&output_dir, &page, &rendered)?;
    }

    write_assets(&output_dir)?;
    generate_pagefind_bundle(
        &wiki_root,
        &output_dir.join("pagefind"),
        &fetch,
        args.concurrency,
    )
    .await?;

    info!(output = %output_dir, "wiki rendered");

    Ok(())
}

async fn run_index(args: IndexArgs) -> Result<()> {
    let wiki_root = absolute_path(&args.root)?;
    let output_dir = absolute_path(&args.output)?;
    let fetch = FetchSettings {
        cache_dir: absolute_path(&args.cache_dir)?,
        cache_ttl: Duration::from_secs(args.cache_ttl_hours.saturating_mul(60 * 60)),
        offline: args.offline,
    };

    generate_pagefind_bundle(&wiki_root, &output_dir, &fetch, args.concurrency).await
}

async fn run_serve(args: ServeArgs) -> Result<()> {
    let site_dir = absolute_path(&args.dir)?;

    if !site_dir.exists() {
        bail!("site directory does not exist: {}", site_dir);
    }

    info!(dir = %site_dir, addr = %args.addr, "serving static site");

    HttpServer::new({
        let site_dir = site_dir.clone();
        move || {
            App::new().service(
                Files::new("/", site_dir.as_str())
                    .index_file("index.html")
                    .prefer_utf8(true)
                    .use_last_modified(true),
            )
        }
    })
    .bind(&args.addr)
    .with_context(|| format!("failed to bind to {}", args.addr))?
    .run()
    .await
    .context("static file server failed")?;

    Ok(())
}

async fn generate_pagefind_bundle(
    wiki_root: &Utf8Path,
    output_dir: &Utf8Path,
    fetch: &FetchSettings,
    concurrency: usize,
) -> Result<()> {
    info!(
        root = %wiki_root,
        output = %output_dir,
        cache = %fetch.cache_dir,
        offline = fetch.offline,
        "indexing linked pages"
    );

    prepare_output_dir(output_dir)?;
    fs::create_dir_all(fetch.cache_dir.as_std_path())
        .with_context(|| format!("failed to create cache directory {}", fetch.cache_dir))?;

    let links = collect_links(wiki_root)?;
    if links.is_empty() {
        bail!("no links discovered under {}", wiki_root);
    }

    info!(count = links.len(), "collected unique links");

    let downloaded = download_targets(&links, fetch, concurrency).await?;
    if downloaded.is_empty() {
        bail!("no remote pages could be downloaded");
    }

    let mut index = PagefindIndex::new(Some(
        PagefindServiceConfig::builder()
            .keep_index_url(true)
            .force_language("en".into())
            .build(),
    ))?;

    let progress = build_progress_bar(downloaded.len() as u64);
    for page in downloaded {
        progress.set_message(page.link.title.clone());
        let wrapped = wrap_remote_content(&page);
        if let Err(err) = index
            .add_html_file(None, Some(page.link.url.as_str().into()), wrapped)
            .await
        {
            warn!(error = %err, url = %page.link.url, "failed to add page to index");
        }
        progress.inc(1);
    }
    progress.finish_with_message("Indexed fetched pages");

    let written: String = index
        .write_files(Some(output_dir.to_string()))
        .await
        .context("failed to write pagefind bundle")?;

    info!(bundle = %written, "pagefind bundle generated");

    Ok(())
}

fn render_page(content: &Content, tree: &[Node], page: &Page) -> String {
    let breadcrumbs = build_breadcrumbs(content, page);
    let nav_hierarchy: Vec<String> = breadcrumbs.iter().map(|(name, _)| name.clone()).collect();
    let sidebar = render::content_navigation(tree, &nav_hierarchy);
    let page_title = format!("{} | Matt's Wiki", page.title);

    let markup = html! {
        (DOCTYPE)
        html {
            (render::header(page_title.as_str()))
            body {
                (render::layout(
                    sidebar,
                    html! {
                        ol .hiearchy {
                            @for (name, link) in &breadcrumbs {
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
                    },
                    html! {
                        article { (PreEscaped(&page.html)) }
                    }
                ))
            }
        }
    };

    markup.into_string()
}

fn build_breadcrumbs(content: &Content, page: &Page) -> Vec<(String, String)> {
    let mut trail = Vec::with_capacity(page.parents.len());
    for parent in &page.parents {
        if parent == "/" {
            trail.push(("Wiki".into(), parent.clone()));
            continue;
        }

        if let Some(parent_page) = content.get(parent) {
            trail.push((parent_page.title.clone(), parent.clone()));
        }
    }
    trail
}

fn write_page(output_dir: &Utf8Path, page: &Page, contents: &str) -> Result<()> {
    let destination = page_output_path(output_dir, &page.path);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent.as_std_path())
            .with_context(|| format!("failed to create {}", parent))?;
    }

    fs::write(destination.as_std_path(), contents)
        .with_context(|| format!("failed to write {}", destination))?;
    Ok(())
}

fn page_output_path(output_dir: &Utf8Path, page_path: &str) -> Utf8PathBuf {
    if page_path == "/" {
        return output_dir.join("index.html");
    }

    let trimmed = page_path.trim_start_matches('/');
    output_dir.join(trimmed).join("index.html")
}

fn write_assets(output_dir: &Utf8Path) -> Result<()> {
    let css = grass::from_string(
        include_str!("../assets/styles.scss").to_owned(),
        &grass::Options::default().style(OutputStyle::Compressed),
    )
    .context("failed to compile styles")?;
    fs::write(output_dir.join("styles.css").as_std_path(), css.as_bytes())
        .context("failed to write styles.css")?;

    fs::write(
        output_dir.join("script.js").as_std_path(),
        include_bytes!("../assets/script.js"),
    )
    .context("failed to write script.js")?;

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}

fn collect_links(root: &Utf8Path) -> Result<Vec<WikiLink>> {
    let mut links = Vec::new();

    for entry in WalkDir::new(root).into_iter().filter_entry(should_visit) {
        let entry = entry?;
        if !entry.file_type().is_file() || !is_markdown_file(&entry) {
            continue;
        }

        let full_path = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|_| anyhow!("paths must be valid utf-8"))?;
        let relative = full_path
            .strip_prefix(root)
            .with_context(|| format!("{} is not inside {}", full_path, root))?;

        let tags = derive_tags(relative);
        let contents = fs::read_to_string(full_path.as_std_path())
            .with_context(|| format!("failed to read {}", full_path))?;
        let page_links = parse_links(&contents, &tags, relative);
        links.extend(page_links);
    }

    Ok(dedupe_links(links))
}

fn parse_links(contents: &str, tags: &[String], source: &Utf8Path) -> Vec<WikiLink> {
    let mut found = Vec::new();
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let mut current_link: Option<Url> = None;
    let mut link_text = String::new();
    let mut strong_depth = 0usize;
    let mut starred = false;

    let parser = MarkdownParser::new_ext(contents, options);
    for event in parser {
        match event {
            Event::Start(Tag::Strong) => strong_depth += 1,
            Event::End(TagEnd::Strong) => strong_depth = strong_depth.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => {
                if let Ok(url) = Url::parse(&dest_url) {
                    if matches!(url.scheme(), "http" | "https") {
                        starred = strong_depth > 0;
                        link_text.clear();
                        current_link = Some(url);
                    }
                }
            }
            Event::Text(text) => {
                if current_link.is_some() {
                    link_text.push_str(&text);
                }
            }
            Event::End(TagEnd::Link) => {
                if let Some(url) = current_link.take() {
                    let title = if link_text.trim().is_empty() {
                        url.domain()
                            .map(|d| d.into())
                            .unwrap_or_else(|| url.as_str().into())
                    } else {
                        link_text.trim().into()
                    };
                    found.push(WikiLink {
                        title,
                        url,
                        tags: tags.to_vec(),
                        source_path: source.to_owned(),
                        starred,
                    });
                    link_text.clear();
                    starred = false;
                }
            }
            _ => {}
        }
    }

    found
}

fn dedupe_links(links: Vec<WikiLink>) -> Vec<WikiLink> {
    let mut map = HashMap::new();
    for link in links {
        map.entry(link.url.clone()).or_insert(link);
    }
    map.into_values().collect()
}

fn derive_tags(relative: &Utf8Path) -> Vec<String> {
    let mut dir = relative.to_owned();
    let mut tags = Vec::new();
    if dir.file_name().is_some() {
        dir.pop();
    }
    tags.extend(
        dir.components()
            .filter_map(|component| match component.as_str() {
                "" => None,
                name if name.eq_ignore_ascii_case("index") => None,
                name => Some(name.replace('_', " ")),
            }),
    );
    if tags.is_empty() {
        if let Some(stem) = relative.file_stem() {
            tags.push(stem.replace('_', " "));
        }
    }
    tags
}

fn should_visit(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|name| !IGNORED_DIRECTORIES.contains(&name))
        .unwrap_or(true)
}

fn is_markdown_file(entry: &DirEntry) -> bool {
    entry
        .path()
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

async fn download_targets(
    links: &[WikiLink],
    fetch: &FetchSettings,
    concurrency: usize,
) -> Result<Vec<DownloadedPage>> {
    let client = Client::builder()
        .user_agent("mwp-indexer/0.2 (+https://github.com/matoous/mwp)")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .tcp_keepalive(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let pb = build_progress_bar(links.len() as u64);
    pb.set_message("Fetching");

    let mut results = Vec::new();
    let fetch = fetch.clone();

    let stream = stream::iter(links.iter().cloned().map(|link| {
        let client = client.clone();
        let fetch = fetch.clone();
        async move { fetch_page(&client, &fetch, link).await }
    }))
    .buffer_unordered(concurrency.max(1));

    tokio::pin!(stream);

    while let Some(item) = stream.next().await {
        pb.inc(1);
        match item {
            Ok(FetchOutcome::Fresh(page)) => results.push(page),
            Ok(FetchOutcome::Revalidated(page)) => results.push(page),
            Ok(FetchOutcome::Cached(page)) => results.push(page),
            Err(err) => warn!(error = %format!("{err:#}"), "failed to fetch page"),
        }
    }

    pb.finish_with_message("Fetched");

    Ok(results)
}

async fn fetch_page(
    client: &Client,
    fetch: &FetchSettings,
    link: WikiLink,
) -> Result<FetchOutcome> {
    let cache_paths = cache_paths(&fetch.cache_dir, &link.url);
    let cached = match read_cache(&cache_paths, &link) {
        Ok(cached) => cached,
        Err(err) => {
            warn!(
                url = %link.url,
                error = %format!("{err:#}"),
                "failed to read cache entry, ignoring cache for this URL"
            );
            None
        }
    };

    if fetch.offline {
        return match cached {
            Some((_, cached_page)) => Ok(FetchOutcome::Cached(cached_page)),
            None => bail!(
                "offline mode enabled and no cached copy exists for {}",
                link.url
            ),
        };
    }

    if let Some((entry, page)) = &cached {
        if !is_cache_stale(entry, fetch.cache_ttl) {
            return Ok(FetchOutcome::Cached(page.clone()));
        }
    }

    let request_meta = cached.as_ref().map(|(entry, _)| entry);
    match fetch_remote_page(client, &link, request_meta).await {
        Ok(FetchOutcome::Fresh(page)) => {
            persist_cache(&cache_paths, &page)?;
            Ok(FetchOutcome::Fresh(page))
        }
        Ok(FetchOutcome::Revalidated(_)) => {
            let (_, page) = cached.context("received 304 without a cached page")?;
            Ok(FetchOutcome::Revalidated(page))
        }
        Ok(FetchOutcome::Cached(page)) => Ok(FetchOutcome::Cached(page)),
        Err(err) => {
            if let Some((_, page)) = cached {
                warn!(
                    url = %link.url,
                    error = %format!("{err:#}"),
                    "network fetch failed, falling back to stale cached copy"
                );
                Ok(FetchOutcome::Cached(page))
            } else {
                Err(err)
            }
        }
    }
}

async fn fetch_remote_page(
    client: &Client,
    link: &WikiLink,
    cache_entry: Option<&CacheEntry>,
) -> Result<FetchOutcome> {
    for attempt in 1..=MAX_FETCH_ATTEMPTS {
        let mut request = client
            .get(link.url.clone())
            .header("Accept", "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1")
            .header("Accept-Language", "en-US,en;q=0.8")
            .header("Cache-Control", "max-age=0");

        if let Some(entry) = cache_entry {
            if let Some(etag) = &entry.etag {
                if let Ok(value) = HeaderValue::from_str(etag) {
                    request = request.header(IF_NONE_MATCH, value);
                }
            }
            if let Some(last_modified) = &entry.last_modified {
                if let Ok(value) = HeaderValue::from_str(last_modified) {
                    request = request.header(IF_MODIFIED_SINCE, value);
                }
            }
        }

        match request.send().await {
            Ok(response) => {
                if response.status() == StatusCode::NOT_MODIFIED {
                    info!(url = %link.url, "revalidated cached page");
                    return Ok(FetchOutcome::Revalidated(DownloadedPage {
                        link: link.clone(),
                        html: String::new(),
                        etag: cache_entry.and_then(|entry| entry.etag.clone()),
                        last_modified: cache_entry.and_then(|entry| entry.last_modified.clone()),
                    }));
                }

                let etag = response
                    .headers()
                    .get(ETAG)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                let last_modified = response
                    .headers()
                    .get(LAST_MODIFIED)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                let status = response.status();
                let response = response
                    .error_for_status()
                    .with_context(|| format!("HTTP error for {}: {}", link.url, status))?;
                let body = response
                    .text()
                    .await
                    .with_context(|| format!("failed to read response body for {}", link.url))?;

                return Ok(FetchOutcome::Fresh(DownloadedPage {
                    link: link.clone(),
                    html: body,
                    etag,
                    last_modified,
                }));
            }
            Err(err) => {
                let message = describe_reqwest_error(&err, &link.url);
                if attempt == MAX_FETCH_ATTEMPTS {
                    return Err(anyhow!(message));
                }

                warn!(
                    url = %link.url,
                    attempt,
                    max_attempts = MAX_FETCH_ATTEMPTS,
                    error = %message,
                    "request failed, retrying"
                );
                tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
            }
        }
    }

    unreachable!("retry loop always returns")
}

fn describe_reqwest_error(err: &reqwest::Error, url: &Url) -> String {
    let kind = if err.is_timeout() {
        "timeout"
    } else if err.is_connect() {
        "connect"
    } else if err.is_redirect() {
        "redirect"
    } else if err.is_status() {
        "http-status"
    } else if err.is_request() {
        "request"
    } else if err.is_body() {
        "body"
    } else if err.is_decode() {
        "decode"
    } else {
        "unknown"
    };

    let source_url = err.url().map(|u| u.as_str()).unwrap_or(url.as_str());
    format!("{} error for {}: {:#}", kind, source_url, err)
}

fn cache_paths(cache_dir: &Utf8Path, url: &Url) -> CachePaths {
    let key = cache_key(url);
    CachePaths {
        body: cache_dir.join(format!("{key}.html")),
        meta: cache_dir.join(format!("{key}.json")),
    }
}

fn cache_key(url: &Url) -> String {
    let mut hasher = Sha1::new();
    hasher.update(url.as_str().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn read_cache(paths: &CachePaths, link: &WikiLink) -> Result<Option<(CacheEntry, DownloadedPage)>> {
    if !paths.meta.exists() || !paths.body.exists() {
        return Ok(None);
    }

    let meta = fs::read_to_string(paths.meta.as_std_path())
        .with_context(|| format!("failed to read cache metadata {}", paths.meta))?;
    let entry: CacheEntry = serde_json::from_str(&meta)
        .with_context(|| format!("failed to parse cache metadata {}", paths.meta))?;
    let html = fs::read_to_string(paths.body.as_std_path())
        .with_context(|| format!("failed to read cache body {}", paths.body))?;

    Ok(Some((
        entry.clone(),
        DownloadedPage {
            link: link.clone(),
            html,
            etag: entry.etag.clone(),
            last_modified: entry.last_modified.clone(),
        },
    )))
}

fn persist_cache(paths: &CachePaths, page: &DownloadedPage) -> Result<()> {
    let parsed = Html::parse_document(&page.html);
    let title_selector = Selector::parse("title").expect("valid title selector");
    let title_hint = parsed
        .select(&title_selector)
        .next()
        .map(|node| node.text().collect::<String>())
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| page.link.title.clone());

    let entry = CacheEntry {
        url: page.link.url.to_string(),
        fetched_at_epoch_s: unix_now()?,
        etag: page.etag.clone(),
        last_modified: page.last_modified.clone(),
        title_hint,
    };

    fs::write(paths.body.as_std_path(), page.html.as_bytes())
        .with_context(|| format!("failed to write cache body {}", paths.body))?;
    fs::write(
        paths.meta.as_std_path(),
        serde_json::to_vec_pretty(&entry).context("failed to encode cache metadata")?,
    )
    .with_context(|| format!("failed to write cache metadata {}", paths.meta))?;

    Ok(())
}

fn is_cache_stale(entry: &CacheEntry, ttl: Duration) -> bool {
    if ttl.is_zero() {
        return true;
    }

    match unix_now() {
        Ok(now) => now.saturating_sub(entry.fetched_at_epoch_s) > ttl.as_secs(),
        Err(_) => true,
    }
}

fn unix_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| anyhow!("system clock is before unix epoch: {}", err))?
        .as_secs())
}

fn prepare_output_dir(path: &Utf8Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to clean previous output at {}", path))?;
    }
    fs::create_dir_all(path).with_context(|| format!("failed to create directory {}", path))?;
    Ok(())
}

fn absolute_path(path: &Utf8Path) -> Result<Utf8PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_owned());
    }

    let cwd = Utf8PathBuf::from_path_buf(std::env::current_dir()?).map_err(|path| {
        anyhow!(
            "current directory contains invalid UTF-8: {}",
            path.display()
        )
    })?;
    Ok(cwd.join(path))
}

fn build_progress_bar(len: u64) -> ProgressBar {
    let pb = ProgressBar::new(len);
    let style =
        ProgressStyle::with_template("{spinner:.green} [{elapsed}] {pos}/{len} steps | {msg}")
            .expect("progress style must be valid");
    pb.set_style(style);
    pb
}

fn wrap_remote_content(page: &DownloadedPage) -> String {
    let parsed = Html::parse_document(&page.html);
    let title_selector = Selector::parse("title").expect("valid title selector");
    let body_selector = Selector::parse("body").expect("valid body selector");

    let remote_title = parsed
        .select(&title_selector)
        .next()
        .map(|title| title.inner_html().trim().to_owned());
    let body = parsed
        .select(&body_selector)
        .next()
        .map(|body| body.inner_html())
        .filter(|html| !html.trim().is_empty())
        .unwrap_or_else(|| page.html.clone());

    let title = remote_title
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| page.link.title.clone());

    let mut document = String::new();
    document.push_str("<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\" />");
    document.push_str(&format!(
        "<meta data-pagefind-meta=\"title\" content=\"{}\" />",
        encode_safe(&title)
    ));
    document.push_str(&format!(
        "<meta data-pagefind-meta=\"source\" content=\"{}\" />",
        encode_safe(page.link.source_path.as_str())
    ));
    if page.link.starred {
        document.push_str("<meta data-pagefind-filter=\"starred\" content=\"true\" />");
    }
    for tag in &page.link.tags {
        document.push_str(&format!(
            "<meta data-pagefind-filter=\"tag\" content=\"{}\" />",
            encode_safe(tag)
        ));
    }
    document.push_str("</head><body data-pagefind-body><article>");
    document.push_str(&body);
    document.push_str("</article></body></html>");

    document
}
