mod config;
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
    let text = "A Rust CLI to read Wikipedia

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
        let mut stdout = io::stdout().lock();
        finish_output(write!(stdout, "{text}"));
    } else {
        let mut stderr = io::stderr().lock();
        let _ = write!(stderr, "{text}");
    }
    process::exit(code);
}

fn reject_flag_value(flag: &str, inline: &Option<String>) {
    if inline.is_some() {
        eprintln!("error: {flag} does not take a value");
        usage(2);
    }
}

fn parse_args() -> Args {
    let raw_args = match env::args_os()
        .skip(1)
        .map(|arg| arg.into_string())
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(args) => args,
        Err(_) => {
            eprintln!("error: arguments must be valid UTF-8");
            usage(2);
        }
    };
    if raw_args.is_empty() {
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
    let mut source_set = false;
    let mut lang_set = false;
    let mut wpm_set = false;
    let mut argv = raw_args.into_iter();

    while let Some(arg) = argv.next() {
        if arg == "--" {
            words.extend(argv);
            break;
        }
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
            "-h" | "--help" => {
                reject_flag_value(&flag, &inline);
                usage(0);
            }
            "-V" | "--version" => {
                reject_flag_value(&flag, &inline);
                println!("wiki {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            "-f" | "--full" => {
                reject_flag_value(&flag, &inline);
                args.full = true;
            }
            "--slow" => {
                reject_flag_value(&flag, &inline);
                args.slow = true;
            }
            "--json" => {
                reject_flag_value(&flag, &inline);
                args.json = true;
            }
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
                source_set = true;
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
                wpm_set = true;
            }
            "-l" | "--lang" => {
                args.lang = take_value("--lang").to_lowercase();
                lang_set = true;
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

    let config = config::load().unwrap_or_else(|error| {
        eprintln!("error: {error}");
        process::exit(2);
    });
    if !source_set
        && let Some(source) = config.source().unwrap_or_else(|error| {
            eprintln!("error: {error}");
            process::exit(2);
        })
    {
        args.source = source;
    }
    if !lang_set && let Some(lang) = config.lang {
        args.lang = lang;
    }
    if !wpm_set && let Some(wpm) = config.wpm {
        args.wpm = wpm;
        args.slow = true;
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

fn print_slowly<W: Write>(out: &mut W, text: &str, wpm: u64, slow: bool) -> io::Result<()> {
    let width = env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_WIDTH)
        .max(20);

    let per_word = Duration::from_millis((60_000 / wpm).max(1));
    let mut column = 0usize;

    for line in text.lines() {
        let line = line.trim();

        if line.is_empty() {
            writeln!(out)?;
            out.flush()?;
            column = 0;
            continue;
        }

        for word in line.split_whitespace() {
            let len = word.chars().count();

            if column > 0 && column + 1 + len > width {
                writeln!(out)?;
                column = 0;
            }

            let separator = if column > 0 { " " } else { "" };
            write!(out, "{separator}{word}")?;
            column += separator.len() + len;

            if slow {
                out.flush()?;
                sleep(per_word + Duration::from_millis(extra_pause_ms(word)));
            }
        }

        writeln!(out)?;
        out.flush()?;
        column = 0;

        if slow {
            sleep(Duration::from_millis(450));
        }
    }
    Ok(())
}

fn finish_output(result: io::Result<()>) {
    if let Err(error) = result
        && error.kind() != io::ErrorKind::BrokenPipe
    {
        eprintln!("error: could not write output: {error}");
        process::exit(1);
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
                let stdout = io::stdout();
                let mut out = stdout.lock();
                let result = serde_json::to_writer_pretty(&mut out, &value)
                    .map_err(|error| match error.io_error_kind() {
                        Some(kind) => io::Error::new(kind, error),
                        None => io::Error::other(error),
                    })
                    .and_then(|()| writeln!(out));
                drop(out);
                finish_output(result);
                return;
            }
            let theme = Theme::from_env(args.color);
            let result = {
                let stdout = io::stdout();
                let mut out = stdout.lock();
                writeln!(
                    out,
                    "{}\n",
                    theme.paint(Role::Heading, &sanitize(&article.title))
                )
                .and_then(|()| {
                    print_slowly(&mut out, &sanitize(&article.text), args.wpm, args.slow)
                })
            };
            finish_output(result);
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

    #[test]
    fn prints_paragraphs_and_propagates_success() {
        let mut output = Vec::new();
        print_slowly(&mut output, "one two\n\nthree", DEFAULT_WPM, false).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "one two\n\nthree\n");
    }

    #[test]
    fn broken_pipes_are_not_output_errors() {
        finish_output(Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed")));
    }
}
