# wiki

A Rust CLI to read Wikipedia

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

Text prints immediately by default. `--slow` switches to character-by-character
reading: each character waits `60000 / (wpm * 5)` milliseconds, commas add
110 ms, sentence ends add 220 ms, and paragraph breaks add 450 ms. Lines wrap to
`COLUMNS`, or 80 when it is unset. `--json` returns the source, query, title, and extracted text for
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

`--full` applies only to Wikipedia. It's FOSS and wikiHow return their best
matching article, and wikiHow requires Chromium or Chrome.

## Config

Reads `~/.config/wiki/config.toml` if present. Flags override the file;
defaults apply otherwise.

## Install

**Requirements:** Rust 1.88+, `git`, and Chromium or Chrome for
`--source wikihow`.

```bash
cargo install --git https://github.com/batusaribay/wiki.git --locked
```

## License

This project is licensed under the [MIT License](LICENSE).
