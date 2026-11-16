use std::io::Read;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde_json::Value;

const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
const USER_AGENT: &str = concat!(
    "wiki-search/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/batusaribay/wiki-search)"
);
const ITSFOSS_HOME: &str = "https://itsfoss.com/";
const WIKIHOW_SEARCH: &str = "https://www.wikihow.com/wikiHowTo?search=";
const JINA_READER: &str = "https://r.jina.ai/";

static IMAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[[^\]]*\]\([^)]*\)").expect("valid image regex"));
static REFERENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[\d+\]\]\([^)]*\)").expect("valid reference regex"));
static LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").expect("valid link regex"));
static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^#{1,6}\s*").expect("valid heading regex"));
static BOLD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*").expect("valid bold regex"));
static BULLET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*[*-]\s+").expect("valid bullet regex"));

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
        Source::WikiHow => wiki_how(&agent, query),
    }
}

fn get(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let response = agent
        .get(url)
        .call()
        .map_err(|e| format!("request failed: {e}"))?;

    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_RESPONSE_BYTES)
        .read_to_string(&mut body)
        .map_err(|e| format!("could not read response: {e}"))?;

    if body.is_empty() {
        return Err("the server returned an empty response".to_string());
    }

    Ok(body)
}

fn wikipedia(agent: &ureq::Agent, query: &str, lang: &str, full: bool) -> Result<Article, String> {
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

    let terms: Vec<String> = phrase
        .split_whitespace()
        .filter(|term| term.chars().count() > 1 && !is_stop_word(term))
        .map(ToOwned::to_owned)
        .collect();

    if terms.is_empty() {
        return posts.first();
    }

    let mut best = None;
    let mut best_score = 0;

    for post in posts {
        let title = post["title"].as_str().unwrap_or("").to_lowercase();
        let excerpt = post["excerpt"].as_str().unwrap_or("").to_lowercase();
        let mut score = 0;

        for term in &terms {
            if title.contains(term) {
                score += 10;
            }
            if excerpt.contains(term) {
                score += 3;
            }
        }

        if title.contains(&phrase) {
            score += 30;
        }

        if score > best_score {
            best_score = score;
            best = Some(post);
        }
    }

    best
}

fn is_stop_word(word: &str) -> bool {
    matches!(
        word,
        "a" | "an"
            | "and"
            | "are"
            | "for"
            | "how"
            | "in"
            | "is"
            | "it"
            | "of"
            | "on"
            | "the"
            | "to"
            | "what"
            | "why"
            | "with"
    )
}

fn wiki_how(agent: &ureq::Agent, query: &str) -> Result<Article, String> {
    let search_url = format!("{JINA_READER}{WIKIHOW_SEARCH}{}", urlencode(query));
    let page = get(agent, &search_url)?;
    let article_url = parse_wikihow_result(&page)
        .ok_or_else(|| format!("no wikiHow article found for \"{query}\""))?;

    let page = get(agent, &format!("{JINA_READER}{article_url}"))?;
    parse_wikihow_article(&page)
}

fn parse_wikihow_result(page: &str) -> Option<String> {
    let marker = "](https://www.wikihow.com/";
    let mut offset = 0;

    while let Some(position) = page[offset..].find(marker) {
        let close = offset + position;
        let url_start = close + 2;
        let rest = &page[url_start..];
        let url_len = rest.find(')')?;
        let url = &rest[..url_len];
        let title = page[..close]
            .rfind('[')
            .map(|open| &page[open + 1..close])
            .unwrap_or("");

        if title.starts_with("How to ") && is_wikihow_article_url(url) {
            return Some(url.to_string());
        }

        offset = url_start + url_len + 1;
    }

    None
}

fn is_wikihow_article_url(url: &str) -> bool {
    url.strip_prefix("https://www.wikihow.com/")
        .is_some_and(|path| {
            !path.is_empty() && !path.contains(['/', ':', '#', '?']) && !path.contains("wikiHow")
        })
}

fn parse_wikihow_article(page: &str) -> Result<Article, String> {
    let title = page
        .lines()
        .find_map(|line| line.strip_prefix("Title:"))
        .unwrap_or("wikiHow")
        .trim()
        .to_string();
    let content = page
        .split_once("Markdown Content:")
        .map(|(_, content)| content)
        .ok_or("wikiHow returned an unexpected response")?;

    let text = clean_wikihow_markdown(truncate_wikihow(content));

    if text.is_empty() {
        return Err("wikiHow returned no readable text".to_string());
    }

    Ok(Article { title, text })
}

fn truncate_wikihow(content: &str) -> &str {
    let end = [
        "\n## References",
        "\n## About This Article",
        "\nDid this article help you?",
    ]
    .iter()
    .filter_map(|marker| content.find(marker))
    .min()
    .unwrap_or(content.len());

    content[..end].trim_end()
}

fn clean_wikihow_markdown(content: &str) -> String {
    let text = IMAGE_RE.replace_all(content, "");
    let text = REFERENCE_RE.replace_all(&text, "");
    let text = LINK_RE.replace_all(&text, "$1");

    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();

        if line.is_empty() {
            if lines.last().is_some_and(|last| !last.is_empty()) {
                lines.push(String::new());
            }
            continue;
        }

        if line == "Advertisement"
            || line == "Download PDF"
            || is_step_number(line)
            || is_video(line)
        {
            continue;
        }

        if line.starts_with('|') {
            if lines.last().is_some_and(|last| last.starts_with("- ")) {
                lines.pop();
            }
            continue;
        }

        let line = HEADING_RE.replace(line, "");
        let line = BOLD_RE.replace_all(&line, "");
        let line = BULLET_RE.replace(&line, "- ");
        let line = line.trim();

        if !line.is_empty() {
            lines.push(line.to_string());
        }
    }

    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }

    collapse_periods(&lines.join("\n"))
}

fn collapse_periods(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        if character != '.' {
            out.push(character);
            continue;
        }

        let mut count = 1;
        while characters.peek() == Some(&'.') {
            characters.next();
            count += 1;
        }

        if count == 2 {
            out.push('.');
        } else {
            for _ in 0..count {
                out.push('.');
            }
        }
    }

    out
}

fn is_step_number(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty()
        && line
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.' || character == ')')
}

fn is_video(line: &str) -> bool {
    line.strip_prefix("Video ").is_some_and(|number| {
        !number.is_empty() && number.chars().all(|character| character.is_ascii_digit())
    })
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
            "[Home](https://www.wikihow.com/Main-Page)",
            "[How to Boil Water * 883,089 views](https://www.wikihow.com/Boil-Water)",
            "[How to Boil an Egg](https://www.wikihow.com/Boil-an-Egg)"
        );

        assert_eq!(
            parse_wikihow_result(page).as_deref(),
            Some("https://www.wikihow.com/Boil-Water")
        );
    }

    #[test]
    fn rejects_wikihow_categories() {
        let page = "[HOW TO Category: Water](https://www.wikihow.com/Category:Water)";
        assert_eq!(parse_wikihow_result(page), None);
    }

    #[test]
    fn cleans_wikihow_markdown() {
        let markdown = concat!(
            "Intro paragraph.\n\n",
            "Download PDF\n\n",
            "*   [First section](https://example.com/#first)\n",
            "|*   [Second section](https://example.com/#second)\n\n",
            "![Image 1: step](https://www.wikihow.com/images/step.jpg)\n",
            "1.   \n",
            "[Video 23](https://example.com/video.mp4)\n\n",
            "**Step text** with [a link](https://example.com) and [[1]](https://example.com/ref).\n\n",
            "Double period.[[2]](https://example.com/ref2).\n\n",
            "Advertisement\n\n",
            "## Things You Need\n\n",
            "*   A wrench\n",
            "*   A jack\n"
        );

        assert_eq!(
            clean_wikihow_markdown(markdown),
            "Intro paragraph.\n\nStep text with a link and .\n\nDouble period.\n\nThings You Need\n\n- A wrench\n- A jack"
        );
    }

    #[test]
    fn truncates_wikihow_footer() {
        let content = "Body text.\n\n## References\n\nstuff\n\n## About This Article\n\nmore";
        assert_eq!(truncate_wikihow(content), "Body text.");
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
