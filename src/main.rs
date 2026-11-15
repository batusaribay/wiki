use std::env;
use std::io::{self, IsTerminal, Read, Write};
use std::process;
use std::thread::sleep;
use std::time::Duration;

const DEFAULT_WPM: u64 = 238;
const DEFAULT_WIDTH: usize = 80;
const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
const USER_AGENT: &str = concat!(
    "wiki-search/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/batusaribay/wiki-search)"
);
const RESET: &str = "\x1b[0m";

#[derive(Clone, Copy, Debug, PartialEq)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

struct Args {
    query: String,
    wpm: u64,
    full: bool,
    lang: String,
    color: ColorMode,
}

fn usage(code: i32) -> ! {
    let text = format!(
        "usage: wiki-search [options] <query>

searches wikipedia and prints the top result at an average reading speed.

options:
  -w, --wpm <n>      words per minute (default: {DEFAULT_WPM})
  -f, --full         print the whole article, not just the intro
  -l, --lang <code>  wikipedia language code (default: en)
      --color <when> use color: auto, always, or never (default: auto)
  -h, --help         show this help
  -V, --version      show the version

examples:
  wiki-search rust programming language
  wiki-search --wpm 300 --full alan turing
"
    );

    if code == 0 {
        print!("{text}");
    } else {
        eprint!("{text}");
    }
    process::exit(code);
}

fn parse_args() -> Args {
    let mut args = Args {
        query: String::new(),
        wpm: DEFAULT_WPM,
        full: false,
        lang: "en".to_string(),
        color: ColorMode::Auto,
    };
    let mut words = Vec::new();
    let mut argv = env::args().skip(1);

    while let Some(arg) = argv.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with('-') => (f.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };

        let mut take_value = |name: &str| -> String {
            inline.clone().unwrap_or_else(|| {
                argv.next().unwrap_or_else(|| {
                    eprintln!("error: {name} needs a value");
                    usage(2);
                })
            })
        };

        match flag.as_str() {
            "-h" | "--help" => usage(0),
            "-V" | "--version" => {
                println!("wiki-search {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            "-f" | "--full" => args.full = true,
            "-w" | "--wpm" => {
                let value = take_value("--wpm");
                args.wpm = value.parse().unwrap_or_else(|_| {
                    eprintln!("error: --wpm must be a number");
                    usage(2);
                });
                if args.wpm == 0 {
                    eprintln!("error: --wpm must be greater than 0");
                    usage(2);
                }
            }
            "-l" | "--lang" => {
                args.lang = take_value("--lang").to_lowercase();
                if args.lang.is_empty() {
                    eprintln!("error: --lang must not be empty");
                    usage(2);
                }
            }
            "--color" => {
                args.color = match take_value("--color").to_lowercase().as_str() {
                    "auto" => ColorMode::Auto,
                    "always" => ColorMode::Always,
                    "never" => ColorMode::Never,
                    _ => {
                        eprintln!("error: --color must be auto, always, or never");
                        usage(2);
                    }
                };
            }
            _ if arg.starts_with('-') && arg != "-" => {
                eprintln!("error: unknown option: {arg}");
                usage(2);
            }
            _ => words.push(arg),
        }
    }

    args.query = words.join(" ");
    if args.query.trim().is_empty() {
        usage(2);
    }
    args
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

fn fetch_article(args: &Args) -> Result<(String, String), String> {
    let url = format!(
        "https://{}.wikipedia.org/w/api.php?action=query&generator=search\
         &gsrsearch={}&gsrlimit=1&prop=extracts{}&explaintext=1&format=json&redirects=1",
        args.lang,
        urlencode(&args.query),
        if args.full { "" } else { "&exintro=1" },
    );

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build();

    let response = agent
        .get(&url)
        .call()
        .map_err(|e| format!("request failed: {e}"))?;

    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_RESPONSE_BYTES)
        .read_to_string(&mut body)
        .map_err(|e| format!("could not read response: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("could not parse response: {e}"))?;

    let pages = json["query"]["pages"]
        .as_object()
        .ok_or_else(|| format!("no wikipedia article found for \"{}\"", args.query))?;

    let page = pages
        .values()
        .next()
        .ok_or_else(|| format!("no wikipedia article found for \"{}\"", args.query))?;

    let title = page["title"].as_str().unwrap_or("untitled").to_string();
    let raw = page["extract"].as_str().unwrap_or("");
    let extract = sanitize(raw).trim().to_string();

    if extract.is_empty() {
        return Err(format!("article \"{title}\" has no readable text"));
    }

    Ok((title, extract))
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character == '\n' {
                '\n'
            } else if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn color_enabled(mode: ColorMode) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            if env::var_os("NO_COLOR").is_some()
                || env::var("TERM").is_ok_and(|term| term.eq_ignore_ascii_case("dumb"))
                || env::var("CLICOLOR").is_ok_and(|value| value == "0")
            {
                return false;
            }
            if env::var("CLICOLOR_FORCE").is_ok_and(|value| value != "0") {
                return true;
            }
            io::stdout().is_terminal()
        }
    }
}

#[derive(Clone, Copy)]
struct Style {
    enabled: bool,
}

impl Style {
    fn title(self, text: &str) -> String {
        self.paint("1;36", text)
    }

    fn heading_start(self) -> &'static str {
        if self.enabled { "\x1b[1;36m" } else { "" }
    }

    fn paint(self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}{RESET}")
        } else {
            text.to_string()
        }
    }
}

fn is_heading(line: &str) -> bool {
    let line = line.trim();
    let level = line
        .chars()
        .take_while(|&character| character == '=')
        .count();
    level >= 2 && line.ends_with('=') && line.chars().count() > level * 2
}

fn extra_pause_ms(word: &str) -> u64 {
    let word = word.trim_end_matches(['"', '\'', ')', ']', '”', '’']);
    if word.ends_with('.') || word.ends_with('!') || word.ends_with('?') {
        220
    } else if word.ends_with(',') || word.ends_with(';') || word.ends_with(':') {
        110
    } else {
        0
    }
}

fn print_slowly(text: &str, wpm: u64, style: Style) {
    let width = env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_WIDTH)
        .max(20);

    let per_word = Duration::from_millis((60_000 / wpm).max(1));
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut column = 0usize;

    for line in text.lines() {
        let line = line.trim();

        if line.is_empty() {
            if writeln!(out).is_err() {
                return;
            }
            let _ = out.flush();
            column = 0;
            sleep(Duration::from_millis(450));
            continue;
        }

        let heading = is_heading(line);
        if heading && write!(out, "{}", style.heading_start()).is_err() {
            return;
        }

        for word in line.split_whitespace() {
            let len = word.chars().count();

            if column > 0 && column + 1 + len > width {
                if writeln!(out).is_err() {
                    return;
                }
                column = 0;
            }

            let separator = if column > 0 { " " } else { "" };
            if write!(out, "{separator}{word}").is_err() || out.flush().is_err() {
                return;
            }
            column += separator.len() + len;

            sleep(per_word + Duration::from_millis(extra_pause_ms(word)));
        }

        if heading && write!(out, "{RESET}").is_err() {
            return;
        }
        if writeln!(out).is_err() || out.flush().is_err() {
            return;
        }
        column = 0;

        if !heading {
            sleep(Duration::from_millis(450));
        }
    }
}

fn main() {
    let args = parse_args();
    let style = Style {
        enabled: color_enabled(args.color),
    };

    match fetch_article(&args) {
        Ok((title, text)) => {
            println!("{}\n", style.title(&title));
            print_slowly(&text, args.wpm, style);
        }
        Err(message) => {
            eprintln!("wiki-search: {message}");
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn sanitize_strips_control_characters_but_keeps_newlines() {
        assert_eq!(sanitize("a\u{1b}[31mred\nnext\ttab"), "a [31mred\nnext tab");
    }

    #[test]
    fn detects_section_headings() {
        assert!(is_heading("== History =="));
        assert!(is_heading("=== Early life ==="));
        assert!(!is_heading("A == B"));
        assert!(!is_heading("==="));
        assert!(!is_heading("plain text"));
    }

    #[test]
    fn style_is_disabled_cleanly() {
        let plain = Style { enabled: false };
        assert_eq!(plain.title("Kedi"), "Kedi");
        assert_eq!(plain.heading_start(), "");

        let colored = Style { enabled: true };
        assert_eq!(colored.title("Kedi"), "\x1b[1;36mKedi\x1b[0m");
        assert_eq!(colored.heading_start(), "\x1b[1;36m");
    }

    #[test]
    fn pauses_after_punctuation() {
        assert_eq!(extra_pause_ms("word"), 0);
        assert_eq!(extra_pause_ms("word,"), 110);
        assert_eq!(extra_pause_ms("word."), 220);
        assert_eq!(extra_pause_ms("word!\""), 220);
        assert_eq!(extra_pause_ms("\"word?\""), 220);
    }
}
