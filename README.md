# wiki-search

Search Wikipedia from the terminal.

![wiki-search in a terminal](preview.png)

## What it is

A single Rust binary that searches Wikipedia and prints the article one word at
a time. The default pace is 238 words per minute, roughly the average adult
reading speed, with short pauses at commas and sentence ends.

## Why this exists

Wikipedia is a reading tool that terminals usually use as a link dispenser: you
get a URL, open a browser, and lose the thread. Printing the text in place keeps
it a single step.

It is also deliberately slow. Dumping a wall of text at terminal speed is
scanning, not reading. Pacing the output turns a lookup into a reading session.
The output is plain text with no TUI, pager, or config file. Only the title and
section headings are coloured, and they use the terminal's own palette.

## How it works

1. The query is joined from the arguments and URL-encoded.
2. One request to the Wikipedia API (`generator=search`, `prop=extracts`,
   `explaintext=1`) returns the top result and its plain-text extract.
3. The intro is requested with `exintro=1`. `--full` drops that filter and asks
   for the complete article.
4. The text is printed word by word. Each word waits `60000 / wpm`
   milliseconds; commas add 110 ms, sentence ends add 220 ms, and paragraph
   breaks add 450 ms.
5. Every word is flushed as it is printed, so the pace is visible even when the
   output is redirected.
6. Lines wrap to `COLUMNS` (80 when unset), and each source line ends with a
   newline, so paragraphs stay separate. Anything wider than the terminal is
   the terminal's problem, not the formatter's.
7. The title and section headings are painted with ANSI colours from the
   terminal's palette. The article body stays plain.
8. TLS is handled by `rustls`. The request has a 10-second connect timeout, a
   30-second overall timeout, and a 16 MiB response cap.

## Usage

### Commands

| Command | Action |
| --- | --- |
| `wiki-search <query>` | Read the intro of the top search result. |
| `wiki-search --full <query>` | Read the whole article. |
| `wiki-search --wpm 300 <query>` | Read faster or slower. |
| `wiki-search --lang tr <query>` | Use another Wikipedia edition. |
| `wiki-search --color never <query>` | Force plain text output. |
| `wiki-search --help` | Show the help text. |
| `wiki-search --version` | Show the version. |

### Examples

```bash
wiki-search rust programming language
wiki-search --wpm 300 alan turing
wiki-search --full history of the bicycle
wiki-search --lang tr kedi
wiki-search --color never serendipity
COLUMNS=100 wiki-search serendipity
NO_COLOR=1 wiki-search serendipity
```

A query with no matching article exits with status `1` and prints the reason on
stderr.

## Theme

Colour comes from the terminal's ANSI palette, never from hard-coded RGB
values. The terminal maps palette slots to the active theme, so light, dark, and
custom themes are followed automatically. Only the title and section headings
are painted; the article body is always plain.

Colour is enabled only when stdout is a terminal, `NO_COLOR` is unset, `TERM` is
not `dumb`, and `CLICOLOR` is not `0`. `CLICOLOR_FORCE=1` or `--color always`
forces it on, and `--color never` turns it off.

## Installation

**Requirements:** Rust 1.88+ and Git.

```bash
cargo install --git https://github.com/batusaribay/wiki-search.git --locked
```

## Uninstall

```bash
cargo uninstall wiki-search
```

## Security

- TLS certificates are verified by `rustls`.
- Only the public Wikipedia API is contacted, with a read-only `GET` request.
- No credentials, cookies, configuration, or telemetry are read or sent.
- Connect and request timeouts are bounded, and responses are capped at 16 MiB.
- Control characters are stripped from article text before anything is
  printed.
- Unsafe code is forbidden by the package lint configuration.

## License

This project is licensed under the [MIT License](LICENSE).
