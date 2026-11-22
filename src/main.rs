mod sources;
mod theme;

use std::env;
use std::io::{self, Write};
use std::process;
use std::thread::sleep;
use std::time::Duration;

use sources::{Source, fetch};
use theme::{ColorMode, Role, Theme};

const DEFAULT_WPM: u64 = 238;
const DEFAULT_WIDTH: usize = 80;

struct Args {
    query: String,
    wpm: u64,
    full: bool,
    lang: String,
    source: Source,
    slow: bool,
    json: bool,
    color: ColorMode,
}

fn usage(code: i32) -> ! {
    let text = "A Rust CLI to read Wikipedia and other sources

Usage: wiki [OPTIONS] <QUERY>

Options:
  -s, --source <NAME>  Source: wikipedia, itsfoss, or wikihow (default: wikipedia)
  -w, --wpm <N>        Reading speed in words per minute (implies --slow)
      --slow           Print at reading speed
  -f, --full           Print the whole Wikipedia article, not just the intro
  -l, --lang <CODE>    Wikipedia language code (default: en)
      --json           Print the article as JSON
      --color <WHEN>   Color: auto, always, or never (default: auto)
  -h, --help           Print help
  -V, --version        Print version
";

    if code == 0 {
        print!("{text}");
    } else {
        eprint!("{text}");
    }
    process::exit(code);
}

fn parse_args() -> Args {
    if env::args_os().count() == 1 {
        usage(0);
    }
    let mut args = Args {
        query: String::new(),
        wpm: DEFAULT_WPM,
        full: false,
        lang: "en".to_string(),
        source: Source::Wikipedia,
        slow: false,
        json: false,
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
                println!("wiki {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            "-f" | "--full" => args.full = true,
            "--slow" => args.slow = true,
            "--json" => args.json = true,
            "--color" => {
                let value = take_value("--color");
                args.color = ColorMode::parse(&value).unwrap_or_else(|| {
                    eprintln!("error: unknown color mode: {value}");
                    eprintln!("available modes: auto, always, never");
                    usage(2);
                });
            }
            "-s" | "--source" => {
                let value = take_value("--source");
                args.source = Source::parse(&value).unwrap_or_else(|| {
                    eprintln!("error: unknown source: {value}");
                    eprintln!("available sources: {}", Source::NAMES);
                    usage(2);
                });
            }
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
                args.slow = true;
            }
            "-l" | "--lang" => {
                args.lang = take_value("--lang").to_lowercase();
                if args.lang.is_empty() {
                    eprintln!("error: --lang must not be empty");
                    usage(2);
                }
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

fn print_slowly(text: &str, wpm: u64, slow: bool) {
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
            if slow {
                sleep(Duration::from_millis(450));
            }
            continue;
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
            if write!(out, "{separator}{word}").is_err() {
                return;
            }
            column += separator.len() + len;

            if slow {
                if out.flush().is_err() {
                    return;
                }
                sleep(per_word + Duration::from_millis(extra_pause_ms(word)));
            }
        }

        if writeln!(out).is_err() || out.flush().is_err() {
            return;
        }
        column = 0;

        if slow {
            sleep(Duration::from_millis(450));
        }
    }
}

fn main() {
    let args = parse_args();

    match fetch(args.source, &args.query, &args.lang, args.full) {
        Ok(article) => {
            if args.json {
                let value = serde_json::json!({
                    "source": args.source.name(),
                    "query": args.query,
                    "language": if args.source == Source::Wikipedia { Some(&args.lang) } else { None },
                    "title": article.title,
                    "text": article.text,
                });
                match serde_json::to_writer_pretty(io::stdout(), &value) {
                    Ok(()) => println!(),
                    Err(error) => {
                        eprintln!("error: {error}");
                        process::exit(1);
                    }
                }
                return;
            }
            let theme = Theme::from_env(args.color);
            println!(
                "{}\n",
                theme.paint(Role::Heading, &sanitize(&article.title))
            );
            print_slowly(&sanitize(&article.text), args.wpm, args.slow);
        }
        Err(message) => {
            eprintln!("error: {message}");
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_control_characters_but_keeps_newlines() {
        assert_eq!(sanitize("a\u{1b}[31mred\nnext\ttab"), "a [31mred\nnext tab");
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
