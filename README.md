# wiki

A Wikipedia browser in the terminal

![wiki in a terminal](preview.png)

## What it is

A single Rust binary that searches Wikipedia, It's FOSS, or wikiHow and prints
the article one word at a time. The default pace is 238 words per minute,
roughly the average adult reading speed, with short pauses at commas and
sentence ends. Choose the source with `--source`.

## Why this exists

Reference sites are reading tools that terminals usually use as link dispensers:
you get a URL, open a browser, and lose the thread. Printing the text in place
keeps it a single step.

It is also deliberately slow. Dumping a wall of text at terminal speed is
scanning, not reading. Pacing the output turns a lookup into a reading session.
The output is plain text with no colour, TUI, pager, or config file.

## How it works

1. The query is joined from the arguments, URL-encoded, and sent to the selected
   source.
2. Wikipedia is queried through its API (`generator=search`, `prop=extracts`,
   `explaintext=1`). The intro is requested with `exintro=1`; `--full` asks for
   the complete article.
3. It's FOSS is searched through its Ghost content API. The search index is
   downloaded, scored locally, and the best post is fetched as plain text.
4. wikiHow is loaded in a local headless Chromium/Chrome because the site
   blocks plain automated requests. The rendered page is reduced to its intro,
   steps, tips, and warnings.
5. The text is printed word by word. Each word waits `60000 / wpm`
   milliseconds; commas add 110 ms, sentence ends add 220 ms, and paragraph
   breaks add 450 ms.
6. Every word is flushed as it is printed, so the pace is visible even when the
   output is redirected.
7. Lines wrap to `COLUMNS` (80 when unset), and each source line ends with a
   newline, so paragraphs stay separate. Anything wider than the terminal is
   the terminal's problem, not the formatter's.
8. TLS is handled by `rustls`. Requests have a 10-second connect timeout, a
   60-second overall timeout, and a 16 MiB response cap.

## Usage

### Commands

| Command | Action |
| --- | --- |
| `wiki <query>` | Read the intro of the top Wikipedia result. |
| `wiki --source itsfoss <query>` | Read the top It's FOSS result. |
| `wiki --source wikihow <query>` | Read the top wikiHow result. |
| `wiki --full <query>` | Read the whole Wikipedia article. |
| `wiki --wpm 300 <query>` | Read faster or slower. |
| `wiki --lang tr <query>` | Use another Wikipedia edition. |
| `wiki --help` | Show the help text. |
| `wiki --version` | Show the version. |

### Examples

```bash
wiki rust programming language
wiki --wpm 300 alan turing
wiki --full history of the bicycle
wiki --lang tr kedi
wiki --source itsfoss linux mint
wiki --source wikihow boil water
COLUMNS=100 wiki serendipity
```

A query with no matching article exits with status `1` and prints the reason on
stderr.

## Sources

| Source | Name | Notes |
| --- | --- | --- |
| Wikipedia | `wikipedia` (default) | Any language edition through `--lang`. |
| It's FOSS | `itsfoss` | Top result from the site's Ghost search index. |
| wikiHow | `wikihow` | Top result, rendered with headless Chromium/Chrome. |

## Install

**Requirements:** Rust 1.88+, Git, and Chromium or Chrome for `--source wikihow`.

```bash
cargo install --git https://github.com/batusaribay/wiki.git --locked
```

## Uninstall

```bash
cargo uninstall wiki
```

## Security

- TLS certificates are verified by `rustls`.
- Only the selected source is contacted, with read-only `GET` requests.
- wikiHow pages are loaded in a local headless browser with a temporary profile;
  no third-party reader proxy is used.
- No credentials, cookies, configuration, or telemetry are read or sent.
- Connect and request timeouts are bounded, and responses are capped at 16 MiB.
- Control characters are stripped from article text before anything is
  printed.
- Unsafe code is forbidden by the package lint configuration.

## License

This project is licensed under the [MIT License](LICENSE).
