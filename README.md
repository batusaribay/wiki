# wiki

A Rust CLI to read Wikipedia and other sources

![wiki in a terminal](preview.png)

## Why this exists

Reading an article should not cost a browser tab and a page of layout. This
tool fetches a source, reduces it to text, and prints it with the terminal width
as the only formatting decision.

## How it works

The query is joined from the arguments, URL-encoded, and sent to the selected
source. Wikipedia is queried through its API with `generator=search`,
`prop=extracts`, and `explaintext=1`. The intro is requested with `exintro=1`,
and `--full` asks for the complete article. It's FOSS is searched through its
Ghost content API: the index is downloaded, scored locally, and the best post is
fetched as plain text. wikiHow is loaded in a local headless Chromium or Chrome
because the site blocks plain automated requests, then reduced to its intro,
steps, tips, and warnings.

Text prints immediately by default. `--slow` switches to word-by-word reading:
each word waits `60000 / wpm` milliseconds, commas add 110 ms, sentence ends add
220 ms, and paragraph breaks add 450 ms. Lines wrap to `COLUMNS`, or 80 when it
is unset. `--json` returns the source, query, title, and extracted text for
scripts. TLS is handled by `rustls`, with a 10-second connect timeout, a
60-second overall timeout, and a 16 MiB response cap.

## Usage

```text
wiki [OPTIONS] <QUERY>

Options:
  -s, --source <NAME>  Source: wikipedia, itsfoss, or wikihow (default: wikipedia)
  -w, --wpm <N>        Reading speed in words per minute (implies --slow)
      --slow           Print at reading speed
  -f, --full           Print the whole Wikipedia article, not just the intro
  -l, --lang <CODE>    Wikipedia language code (default: en)
      --json           Print the article as JSON
      --color <WHEN>   Color output: auto, always, or never [default: auto]
  -h, --help           Print help
  -V, --version        Print version
```

Typical calls:

```bash
wiki rust programming language
wiki --slow alan turing
wiki --wpm 300 alan turing
wiki --full history of the bicycle
wiki --lang tr kedi
wiki --source itsfoss linux mint
wiki --json --full history of the bicycle
COLUMNS=100 wiki serendipity
```

## Notes

Sources are `wikipedia` with any language through `--lang`, `itsfoss` with the
top result from the Ghost search index, and `wikihow` with the top result
rendered through a local headless browser.

A query with no matching article exits `1` and prints the reason on stderr.
`--wpm` implies `--slow`, and in slow mode every word is flushed as it prints,
so the pace is visible even when output is redirected.

Only the selected source is contacted, with read-only `GET` requests. The
wikiHow browser uses a temporary local profile and no third-party reader proxy.
No credentials, cookies, configuration, or telemetry are read or sent. Control
characters are stripped from article text before printing. Unsafe code is
forbidden by the package lint configuration.

Color uses the terminal palette and ANSI attributes only. The article title is
the only styled text; body text inherits the terminal foreground. `--color
never`, `NO_COLOR`, `CLICOLOR=0`, `TERM=dumb`, and non-terminal output all
disable it. JSON is never colored.

## Install

**Requirements:** Rust 1.88+, `git`, and Chromium or Chrome for
`--source wikihow`.

```bash
cargo install --git https://github.com/batusaribay/wiki.git --locked
```

## License

This project is licensed under the [MIT License](LICENSE).
