# wiki

A Wikipedia browser in the terminal

![wiki in a terminal](preview.png)

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
5. The text prints immediately by default. `--slow` switches to word-by-word
   reading: each word waits `60000 / wpm` milliseconds, commas add 110 ms,
   sentence ends add 220 ms, and paragraph breaks add 450 ms. `--wpm` implies
   `--slow`.
6. In slow mode, every word is flushed as it is printed, so the pace is visible
   even when the output is redirected.
7. Lines wrap to `COLUMNS` (80 when unset), and each source line ends with a
   newline, so paragraphs stay separate. Anything wider than the terminal is
   the terminal's problem, not the formatter's.
8. TLS is handled by `rustls`. Requests have a 10-second connect timeout, a
   60-second overall timeout, and a 16 MiB response cap.

## Usage

### Commands

| Command | Action |
| --- | --- |
| `wiki <query>` | Print the intro of the top Wikipedia result. |
| `wiki --source itsfoss <query>` | Print the top It's FOSS result. |
| `wiki --source wikihow <query>` | Print the top wikiHow result. |
| `wiki --slow <query>` | Print at reading speed. |
| `wiki --wpm 300 <query>` | Read faster or slower (implies `--slow`). |
| `wiki --full <query>` | Print the whole Wikipedia article. |
| `wiki --lang tr <query>` | Use another Wikipedia edition. |
| `wiki --help` | Show the help text. |
| `wiki --version` | Show the version. |

### Examples

```bash
wiki rust programming language
wiki --slow alan turing
wiki --wpm 300 alan turing
wiki --full history of the bicycle
wiki --lang tr kedi
wiki --source itsfoss linux mint
wiki --source wikihow boil water
COLUMNS=100 wiki serendipity
```

A query with no matching article exits with status `1` and prints the reason on
stderr.

### Sources

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
