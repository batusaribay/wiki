# wiki-search

A Wikipedia searcher in the terminal

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
The output is plain text with no colour, TUI, pager, or config file.

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
7. TLS is handled by `rustls`. The request has a 10-second connect timeout, a
   30-second overall timeout, and a 16 MiB response cap.

## Usage

### Commands

| Command | Action |
| --- | --- |
| `wiki-search <query>` | Read the intro of the top search result. |
| `wiki-search --full <query>` | Read the whole article. |
| `wiki-search --wpm 300 <query>` | Read faster or slower. |
| `wiki-search --lang tr <query>` | Use another Wikipedia edition. |
| `wiki-search --help` | Show the help text. |
| `wiki-search --version` | Show the version. |

### Examples

```bash
wiki-search rust programming language
wiki-search --wpm 300 alan turing
wiki-search --full history of the bicycle
wiki-search --lang tr kedi
COLUMNS=100 wiki-search serendipity
```

A query with no matching article exits with status `1` and prints the reason on
stderr.

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
