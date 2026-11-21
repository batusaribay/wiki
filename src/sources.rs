use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use scraper::node::Node;
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
const USER_AGENT: &str = concat!(
    "wiki/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/batusaribay/wiki)"
);
const ITSFOSS_HOME: &str = "https://itsfoss.com/";
const WIKIHOW_SEARCH: &str = "https://www.wikihow.com/wikiHowTo?search=";
const BROWSER_TIMEOUT: Duration = Duration::from_secs(40);

static BROWSER: LazyLock<Option<String>> = LazyLock::new(|| {
    [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "brave",
        "brave-browser",
        "microsoft-edge",
    ]
    .into_iter()
    .find(|candidate| {
        Command::new(candidate)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
    .map(ToOwned::to_owned)
});

static RESULT_LINK: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a.result_link").expect("valid result selector"));
static PAGE_TITLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("title").expect("valid title selector"));
static SUBTITLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("#intro .subtitle").expect("valid subtitle selector"));
static INTRO: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("#mf-section-0").expect("valid intro selector"));
static STEP_SECTION: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".section.steps").expect("valid step section"));
static STEP_HEADING: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("h3").expect("valid step heading"));
static STEP: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".step").expect("valid step selector"));
static TIPS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("#tips li").expect("valid tips selector"));
static WARNINGS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("#warnings li").expect("valid warnings selector"));

pub struct Article {
    pub title: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Wikipedia,
    ItsFoss,
    WikiHow,
}

impl Source {
    pub const NAMES: &str = "wikipedia, itsfoss, wikihow";

    pub fn name(self) -> &'static str {
        match self {
            Self::Wikipedia => "wikipedia",
            Self::ItsFoss => "itsfoss",
            Self::WikiHow => "wikihow",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "wikipedia" | "wiki" => Some(Self::Wikipedia),
            "itsfoss" | "its-foss" | "foss" => Some(Self::ItsFoss),
            "wikihow" | "wiki-how" => Some(Self::WikiHow),
            _ => None,
        }
    }
}

pub fn fetch(source: Source, query: &str, lang: &str, full: bool) -> Result<Article, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .user_agent(USER_AGENT)
        .build();

    match source {
        Source::Wikipedia => wikipedia(&agent, query, lang, full),
        Source::ItsFoss => its_foss(&agent, query),
        Source::WikiHow => wiki_how(query),
    }
}

fn get(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let response = agent
        .get(url)
        .call()
        .map_err(|e| format!("request failed: {e}"))?;

    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_RESPONSE_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("could not read response: {e}"))?;
    let body = String::from_utf8_lossy(&bytes).into_owned();

    if body.is_empty() {
        return Err("the server returned an empty response".to_string());
    }

    Ok(body)
}

fn wikipedia(agent: &ureq::Agent, query: &str, lang: &str, full: bool) -> Result<Article, String> {
    validate_lang(lang)?;
    let url = format!(
        "https://{}.wikipedia.org/w/api.php?action=query&generator=search\
         &gsrsearch={}&gsrlimit=1&prop=extracts{}&explaintext=1&format=json&redirects=1",
        lang,
        urlencode(query),
        if full { "" } else { "&exintro=1" },
    );

    let body = get(agent, &url)?;
    let json: Value =
        serde_json::from_str(&body).map_err(|e| format!("could not parse response: {e}"))?;

    let pages = json["query"]["pages"]
        .as_object()
        .ok_or_else(|| format!("no wikipedia article found for \"{query}\""))?;

    let page = pages
        .values()
        .next()
        .ok_or_else(|| format!("no wikipedia article found for \"{query}\""))?;

    let title = page["title"].as_str().unwrap_or("untitled").to_string();
    let text = page["extract"].as_str().unwrap_or("").trim().to_string();

    if text.is_empty() {
        return Err(format!("article \"{title}\" has no readable text"));
    }

    Ok(Article { title, text })
}

fn its_foss(agent: &ureq::Agent, query: &str) -> Result<Article, String> {
    let page = get(agent, ITSFOSS_HOME)?;
    let api = html_attribute(&page, "data-api")
        .filter(|api| api.starts_with("https://itsfoss.com/"))
        .ok_or("could not find the It's FOSS content API")?;
    let api = if api.ends_with('/') {
        api
    } else {
        format!("{api}/")
    };
    let key =
        html_attribute(&page, "data-key").ok_or("could not find the It's FOSS content API key")?;

    let index_url = format!("{api}search-index/posts/?key={}", urlencode(&key));
    let index = get(agent, &index_url)?;
    let index: Value =
        serde_json::from_str(&index).map_err(|e| format!("could not parse search results: {e}"))?;
    let posts = index["posts"]
        .as_array()
        .ok_or("It's FOSS returned an unexpected search index")?;

    let post = best_post(posts, query)
        .ok_or_else(|| format!("no It's FOSS article found for \"{query}\""))?;
    let id = post["id"]
        .as_str()
        .ok_or("It's FOSS search result has no id")?;

    let post_url = format!(
        "{api}posts/{}/?key={}&formats=plaintext",
        urlencode(id),
        urlencode(&key)
    );
    let body = get(agent, &post_url)?;
    let post: Value =
        serde_json::from_str(&body).map_err(|e| format!("could not parse article: {e}"))?;
    let post = post["posts"]
        .get(0)
        .ok_or("It's FOSS returned no article")?;

    let title = post["title"].as_str().unwrap_or("untitled").to_string();
    let text = post["plaintext"].as_str().unwrap_or("").trim().to_string();

    if text.is_empty() {
        return Err(format!("article \"{title}\" has no readable text"));
    }

    Ok(Article { title, text })
}

fn best_post<'a>(posts: &'a [Value], query: &str) -> Option<&'a Value> {
    let phrase = query.trim().to_lowercase();
    if phrase.is_empty() {
        return posts.first();
    }

    let terms = search_terms(&phrase);
    if terms.is_empty() {
        return posts.first();
    }

    let total = posts.len() as f64;
    let mut document_frequency = vec![0usize; terms.len()];

    for post in posts {
        let title = post["title"].as_str().unwrap_or("").to_lowercase();
        let excerpt = post["excerpt"].as_str().unwrap_or("").to_lowercase();

        for (index, term) in terms.iter().enumerate() {
            if title.contains(term.as_str()) || excerpt.contains(term.as_str()) {
                document_frequency[index] += 1;
            }
        }
    }

    let weights: Vec<f64> = document_frequency
        .iter()
        .map(|&frequency| {
            if frequency == 0 {
                0.0
            } else {
                1.0 + (total / frequency as f64).ln()
            }
        })
        .collect();

    let mut best = None;
    let mut best_score = 0.0;

    for post in posts {
        let title = post["title"].as_str().unwrap_or("").to_lowercase();
        let excerpt = post["excerpt"].as_str().unwrap_or("").to_lowercase();
        let mut score = 0.0;

        for (index, term) in terms.iter().enumerate() {
            if weights[index] == 0.0 {
                continue;
            }
            if title.contains(term.as_str()) {
                score += 10.0 * weights[index];
            }
            if excerpt.contains(term.as_str()) {
                score += 3.0 * weights[index];
            }
        }

        if title.contains(&phrase) {
            score += 30.0;
        }

        if score > best_score {
            best_score = score;
            best = Some(post);
        }
    }

    best
}

fn search_terms(phrase: &str) -> Vec<String> {
    phrase
        .split_whitespace()
        .map(|term| {
            term.trim_matches(|character: char| {
                !character.is_alphanumeric() && character != '-' && character != '\''
            })
        })
        .filter(|term| term.chars().count() > 1 && !is_stop_word(term))
        .map(ToOwned::to_owned)
        .collect()
}

fn is_stop_word(word: &str) -> bool {
    matches!(
        word,
        "a" | "an"
            | "and"
            | "are"
            | "for"
            | "foss"
            | "how"
            | "in"
            | "is"
            | "it"
            | "it's"
            | "its"
            | "of"
            | "on"
            | "the"
            | "to"
            | "what"
            | "why"
            | "with"
    )
}

fn wiki_how(query: &str) -> Result<Article, String> {
    let search_url = format!("{WIKIHOW_SEARCH}{}", urlencode(query));
    let page = browser_html(&search_url)?;
    let article_url = parse_wikihow_result(&page)
        .ok_or_else(|| format!("no wikiHow article found for \"{query}\""))?;

    let page = browser_html(&article_url)?;
    parse_wikihow_article(&page)
}

fn browser_html(url: &str) -> Result<String, String> {
    let browser = BROWSER.as_deref().ok_or(
        "wikiHow needs Chromium or Chrome installed (a headless browser gets past the site's bot check)",
    )?;

    // A predictable name in the shared temp directory can be pre-created as a
    // symlink by another local user. Create a private, uniquely named work
    // directory instead (and keep Chromium's profile inside it).
    let (work, output) = private_work_dir()?;
    let profile = work.join("profile");

    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|e| format!("could not create browser output: {e}"))?;

    let mut command = Command::new(browser);
    command
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--blink-settings=imagesEnabled=false")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("--dump-dom")
        .arg(url)
        .stdout(Stdio::from(file))
        .stderr(Stdio::null());

    let result = run_with_timeout(command, BROWSER_TIMEOUT);
    let html = read_limited(&output);
    let _ = fs::remove_dir_all(&work);
    result?;

    if html.is_empty() {
        return Err("the browser returned an empty page".to_string());
    }

    Ok(html)
}

fn private_work_dir() -> Result<(PathBuf, PathBuf), String> {
    for attempt in 0..16 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("wiki-{}-{nanos}-{attempt}", std::process::id()));
        match fs::create_dir(&dir) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
                }
                let output = dir.join("page.html");
                return Ok((dir, output));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("could not create a temporary directory: {error}"));
            }
        }
    }
    Err("could not create a unique temporary directory".to_string())
}

fn read_limited(path: &Path) -> String {
    let Ok(file) = File::open(path) else {
        return String::new();
    };
    let mut bytes = Vec::new();
    let _ = file.take(MAX_RESPONSE_BYTES).read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn validate_lang(lang: &str) -> Result<(), String> {
    let valid = !lang.is_empty()
        && lang.len() <= 16
        && lang
            .chars()
            .all(|character| character.is_ascii_lowercase() || character == '-')
        && !lang.starts_with('-')
        && !lang.ends_with('-')
        && !lang.contains("--");
    if valid {
        Ok(())
    } else {
        Err(format!("invalid Wikipedia language code \"{lang}\""))
    }
}

fn run_with_timeout(mut command: Command, timeout: Duration) -> Result<(), String> {
    let mut child = command
        .spawn()
        .map_err(|e| format!("could not start the browser: {e}"))?;
    let started = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("the browser exited with {status}"))
                };
            }
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("could not wait for the browser: {e}"));
            }
        }

        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("the browser timed out".to_string());
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

fn parse_wikihow_result(page: &str) -> Option<String> {
    let document = Html::parse_document(page);

    document.select(&RESULT_LINK).find_map(|element| {
        let href = element.value().attr("href")?;
        let url = if href.starts_with("https://www.wikihow.com/") {
            href.to_string()
        } else {
            format!(
                "https://www.wikihow.com/{}",
                href.trim_start_matches("https://www.wikihow.com/")
                    .trim_start_matches('/')
            )
        };

        is_wikihow_article_url(&url).then_some(url)
    })
}

fn is_wikihow_article_url(url: &str) -> bool {
    url.strip_prefix("https://www.wikihow.com/")
        .is_some_and(|path| {
            !path.is_empty() && !path.contains(['/', ':', '#', '?']) && !path.contains("wikiHow")
        })
}

fn parse_wikihow_article(page: &str) -> Result<Article, String> {
    let document = Html::parse_document(page);
    let title = document
        .select(&PAGE_TITLE)
        .next()
        .map(|element| {
            element
                .text()
                .collect::<String>()
                .trim()
                .trim_end_matches(" - wikiHow")
                .trim()
                .to_string()
        })
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "wikiHow".to_string());

    let mut parts = Vec::new();

    if let Some(subtitle) = document.select(&SUBTITLE).next() {
        push_part(&mut parts, element_text(subtitle));
    }

    if let Some(intro) = document.select(&INTRO).next() {
        push_part(&mut parts, element_text(intro));
    }

    let mut step_number = 0;

    for section in document.select(&STEP_SECTION) {
        if let Some(heading) = section.select(&STEP_HEADING).next() {
            push_part(&mut parts, element_text(heading));
        }

        for step in section.select(&STEP) {
            let body = element_text(step);
            if !body.is_empty() {
                step_number += 1;
                parts.push(format!("{step_number}. {body}"));
            }
        }
    }

    let tips = collect_list(&document, &TIPS);
    if !tips.is_empty() {
        parts.push("Tips".to_string());
        parts.extend(tips.into_iter().map(|tip| format!("- {tip}")));
    }

    let warnings = collect_list(&document, &WARNINGS);
    if !warnings.is_empty() {
        parts.push("Warnings".to_string());
        parts.extend(warnings.into_iter().map(|warning| format!("- {warning}")));
    }

    let text = parts.join("\n\n");

    if text.trim().is_empty() {
        return Err("wikiHow returned no readable text".to_string());
    }

    Ok(Article { title, text })
}

fn push_part(parts: &mut Vec<String>, text: String) {
    let text = text.trim();
    if !text.is_empty() {
        parts.push(text.to_string());
    }
}

fn collect_list(document: &Html, selector: &Selector) -> Vec<String> {
    document
        .select(selector)
        .map(element_text)
        .filter(|text| !text.is_empty())
        .collect()
}

fn element_text(element: ElementRef<'_>) -> String {
    let mut text = String::new();
    collect_element(element, &mut text);
    normalize_text(&text)
}

fn collect_element(element: ElementRef<'_>, out: &mut String) {
    for child in element.children() {
        match child.value() {
            Node::Text(text) => out.push_str(text),
            Node::Element(_) => {
                let Some(child) = ElementRef::wrap(child) else {
                    continue;
                };
                let name = child.value().name();

                if matches!(name, "script" | "style" | "noscript" | "svg")
                    || child.value().classes().any(|class| {
                        matches!(
                            class,
                            "wh_vote_container"
                                | "wh_thumb_response"
                                | "clearall"
                                | "mwimg"
                                | "image_details"
                                | "pdf_link"
                                | "reference"
                                | "ts_popup"
                                | "ts_close"
                                | "ts_name"
                                | "ts_description"
                                | "ts_source"
                                | "ts_trustworthy"
                        )
                    })
                {
                    continue;
                }

                let block = matches!(
                    name,
                    "p" | "div"
                        | "li"
                        | "ul"
                        | "ol"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "br"
                        | "tr"
                        | "table"
                );

                if name == "li" {
                    out.push_str("\n- ");
                } else if block {
                    out.push('\n');
                }
                collect_element(child, out);
                if block {
                    out.push('\n');
                }
            }
            _ => {}
        }
    }
}

fn normalize_text(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();

    for raw in text.lines() {
        let line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            if lines.last().is_some_and(|last| !last.is_empty()) {
                lines.push(String::new());
            }
        } else {
            lines.push(line);
        }
    }

    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

fn html_attribute(html: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = html.find(&needle)? + needle.len();
    let rest = &html[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_source_names() {
        assert_eq!(Source::parse("wikipedia"), Some(Source::Wikipedia));
        assert_eq!(Source::parse("itsfoss"), Some(Source::ItsFoss));
        assert_eq!(Source::parse("Its-FOSS"), Some(Source::ItsFoss));
        assert_eq!(Source::parse("wikihow"), Some(Source::WikiHow));
        assert_eq!(Source::parse("example"), None);
    }

    #[test]
    fn finds_the_best_post() {
        let posts = vec![
            serde_json::json!({
                "title": "Best Linux Mint Tips",
                "excerpt": "Make the most of Linux Mint"
            }),
            serde_json::json!({
                "title": "A Guide to Arch Linux",
                "excerpt": "Installing Arch"
            }),
            serde_json::json!({
                "title": "Linux News",
                "excerpt": "Weekly roundup"
            }),
        ];

        let post = best_post(&posts, "linux mint").expect("a matching post");
        assert_eq!(post["title"], "Best Linux Mint Tips");
    }

    #[test]
    fn prefers_rare_terms_over_common_ones() {
        let posts = vec![
            serde_json::json!({
                "title": "It's Very Important to be Able to Use Open Source From Top to Bottom",
                "excerpt": "Interviews from the openEuler Summit"
            }),
            serde_json::json!({
                "title": "SSH to Port Other Than 22",
                "excerpt": "Learn how to connect via SSH to a non-standard port"
            }),
            serde_json::json!({
                "title": "How to Use Linux",
                "excerpt": "A guide"
            }),
            serde_json::json!({
                "title": "Use This Tool",
                "excerpt": "Another guide"
            }),
        ];

        let post = best_post(
            &posts,
            "how to use ssh with a non-standard port on it's foss",
        )
        .expect("a matching post");
        assert_eq!(post["title"], "SSH to Port Other Than 22");
    }

    #[test]
    fn ignores_stop_words_when_ranking() {
        let posts = vec![serde_json::json!({
            "title": "How to Back Up Your Files",
            "excerpt": "A guide"
        })];

        let post = best_post(&posts, "how to back up").expect("a matching post");
        assert_eq!(post["title"], "How to Back Up Your Files");
    }

    #[test]
    fn extracts_html_attributes() {
        let html =
            r#"<div data-key="abc123" data-api="https://itsfoss.com/ghost/api/content/"></div>"#;
        assert_eq!(html_attribute(html, "data-key").as_deref(), Some("abc123"));
        assert_eq!(
            html_attribute(html, "data-api").as_deref(),
            Some("https://itsfoss.com/ghost/api/content/")
        );
        assert_eq!(html_attribute(html, "data-missing"), None);
    }

    #[test]
    fn parses_the_first_wikihow_search_result() {
        let page = concat!(
            r#"<a class="result_link" href="https://www.wikihow.com/Category:Water">Water</a>"#,
            r#"<a class="result_link" href="https://www.wikihow.com/Boil-Water">How to Boil Water</a>"#,
            r#"<a class="result_link" href="https://www.wikihow.com/Boil-an-Egg">How to Boil an Egg</a>"#
        );

        assert_eq!(
            parse_wikihow_result(page).as_deref(),
            Some("https://www.wikihow.com/Boil-Water")
        );
    }

    #[test]
    fn rejects_wikihow_categories() {
        let page =
            r#"<a class="result_link" href="https://www.wikihow.com/Category:Water">Water</a>"#;
        assert_eq!(parse_wikihow_result(page), None);
    }

    #[test]
    fn extracts_wikihow_article_sections() {
        let page = r#"<!DOCTYPE html>
<html>
<head><title>How to Boil Water - wikiHow</title></head>
<body>
  <div id="intro"><div class="subtitle">Quick guide</div></div>
  <div id="mf-section-0"><p>Intro paragraph.</p></div>
  <div id="Steps">
    <div class="section steps">
      <h3>On the stove</h3>
      <ul><li><div class="step"><b>Fill the pot.</b> Use cold water.<ul><li>Do not overfill.</li></ul></div></li></ul>
    </div>
    <div class="section steps">
      <h3>In the microwave</h3>
      <ul><li><div class="step"><b>Use a safe bowl.</b> Heat it.</div></li></ul>
    </div>
  </div>
  <div id="tips"><ul><li><div class="tips_li_div">A tip.</div><div class="wh_vote_container">Thanks Helpful 7</div></li></ul></div>
  <div id="warnings"><ul><li><div class="warnings_li_div">A warning.</div></li></ul></div>
</body>
</html>"#;

        let article = parse_wikihow_article(page).expect("a parsed article");
        assert_eq!(article.title, "How to Boil Water");

        for expected in [
            "Quick guide",
            "Intro paragraph.",
            "On the stove",
            "1. Fill the pot. Use cold water.",
            "- Do not overfill.",
            "In the microwave",
            "2. Use a safe bowl. Heat it.",
            "Tips",
            "- A tip.",
            "Warnings",
            "- A warning.",
        ] {
            assert!(
                article.text.contains(expected),
                "missing {expected:?} in {:?}",
                article.text
            );
        }

        assert!(!article.text.contains("Thanks"));
        assert!(!article.text.contains("Helpful"));
    }

    #[test]
    fn accepts_only_plain_language_codes() {
        assert!(validate_lang("en").is_ok());
        assert!(validate_lang("zh-min-nan").is_ok());
        assert!(validate_lang("simple").is_ok());
        assert!(validate_lang("../evil").is_err());
        assert!(validate_lang("en/us").is_err());
        assert!(validate_lang("en_US").is_err());
        assert!(validate_lang("-en").is_err());
        assert!(validate_lang("").is_err());
    }

    #[test]
    fn urlencode_escapes_spaces_and_non_ascii() {
        assert_eq!(urlencode("rust language"), "rust%20language");
        assert_eq!(urlencode("kedi (hayvan)"), "kedi%20%28hayvan%29");
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn urlencode_keeps_unreserved_characters() {
        assert_eq!(urlencode("AZaz09-_.~"), "AZaz09-_.~");
    }
}
