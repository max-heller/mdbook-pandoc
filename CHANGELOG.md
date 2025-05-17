# Changelog

All notable changes to this project will be documented in this file.

## [0.10.4] - 2025-05-17

### Changes

- Increased Minimum Supported Rust Version (MSRV) to 1.81

### Bug Fixes

- Correctly parse multiline $$-delimited MathJax math (#186)

### Features

- Support using TeX macros across math inlines/blocks (#188)


## [0.10.3] - 2025-05-02

### Bug Fixes

- Correctly generate TOC when nested chapters don't contain H1s (#182)

### Features

- Add code block attributes as additional classes (#185)


## [0.10.2] - 2025-04-27

### Features

- Emulate HTML renderer's mathjax support (#177)

  When `output.html.mathjax-support = true`, the following patterns are parsed
  as mathematical expressions and rendered using Pandoc's math nodes:
  - `InlineMath`:  `\\(...\\)`
  - `DisplayMath`: `\\[...\\]` and `$$...$$`

  **Note:** the `pulldown-cmark`-based math support (`$...$` and `$$...$$`)
  enabled by `output.pandoc.markdown.extensions.math = true` takes precedence
  over MathJax emulation. Parsing behavior is slightly different between the
  two--in particular, a line break is `\\` in the `pulldown-cmark` variant,
  whereas it must be written as `\\\\` in the MathJax variant.


## [0.10.1] - 2025-03-15

### Bug Fixes

- Correctly parse text after footnote references (#174)


## [0.10.0] - 2025-02-16

### Changes

- Increase Minimum Supported Rust Version (MSRV) to 1.77 (#163)
- Upgrade `pulldown-cmark` (Markdown parser) to 0.13.0 (#165)
- [**breaking**] Removed a workaround for remote images (#161)

  Removed a workaround for remote images served from URIs without standard image extensions (e.g., https://img.shields.io/github/actions/workflow/status/rust-lang/mdBook/main.yml hosts an SVG image despite its YAML extension).
  `mdbook-pandoc` previously would download such images before handing off to Pandoc because Pandoc was not able to handle them as remote images, but Pandoc implemented a fix in jgm/pandoc#10557 (released in 3.6.3) so this workaround is no longer necessary.

  Removing this workaround allows removing the dependency on an HTTP client, shrinking `mdbook-pandoc`'s dependency tree.

  If your book no longer renders without this workaround, upgrade to Pandoc 3.6.3+ or find a URI for the image with an image-like extension.

- Match mdbook's log format (#169)

### Documentation

- Update README to more accurately describe `mdbook-pandoc`'s rendering pipeline and features (#172)

### Features

- Support block quote tags e.g. [!NOTE] (#138)
- Support mathematical expressions (#166)
- Support definition lists (#168)
- Support superscripted and subscripted text (#171)


## [0.9.3] - 2025-01-18

### Documentation

- Suggest `cargo install --locked` and fix formatting in README (#157)

### Miscellaneous Tasks

- Unexport accidentally-exported macro (#153)

### Testing

- Clean up multiline strings in tests using `indoc` (#155)
- Split test suite into sections (#156)


## [0.9.2] - 2025-01-18

### Bug Fixes

- Parse `<noscript>` elements without panicking (#150)
- Detect cycles in footnote definitions (#151)


## [0.9.1] - 2025-01-17

### Bug Fixes

- Correctly hide lines in Rust code blocks (#147)


## [0.9.0] - 2025-01-12

### Changes

- Raise minimum supported Rust version to 1.75 (#135)
- [**breaking**] Overhaul HTML parsing and Pandoc communication (#137, #140)

  This release completely changes how `mdbook-pandoc` parses Markdown files and how it interacts with Pandoc:
  - Raw HTML: Instead of processing a stream of HTML tokens, `mdbook-pandoc` now uses a browser-grade HTML parser (`html5ever`) to parse raw HTML and construct a combined Markdown+HTML tree. This lets `mdbook-pandoc` avoid having to think too much about the semantics of HTML (which elements are void vs need closing tags, when elements are implicitly closed, etc.).
  - Pandoc: Instead of generating preprocessed Markdown (Commonmark + many extensions), generates Pandoc's native format. This avoids ambiguity in parsing preprocessed markdown (previously, the meaning of Markdown in the original source may have changed based on the Pandoc extensions `mdbook-pandoc` enabled) and the difficulty of round-tripping Markdown (`pulldown-cmark-to-cmark` often had subtle differences).

  This may result in worse compatibility with old versions of Pandoc.


### Bug Fixes

- Recognize `.latex` and `.ltx` output files as LaTeX (#143)

### Miscellaneous Tasks

- Improve logging when preprocessing a chapter fails (#139)


## [0.8.1] - 2025-01-01

### Bug Fixes

- Support more HTML link patterns (#134)


## [0.8.0] - 2024-12-31

### Changes

- [**breaking**] Raw HTML is now parsed and processed in a best-effort manner instead of being passed through verbatim.
  For the most part, this should be neutral at worst (since raw HTML is stripped for most output formats) and improve
  rendered output in the presence of `<img>`, `<a>`, and `<span>` elements (see [**Features**](#features)).
- Tweak log levels to better convey where time is spent (#129)

### Features

- Support `<img>` elements (#124)
- Link to `<a>` elements by `id` (#128)
- Link to `<span>` elements by `id` (#130)

### Bug Fixes

- Set document metadata for all output formats (#125)

### Miscellaneous Tasks

- Upgrade `pandoc` used for tests to version 3.6.1 (#122)
- Update example books (#127, #131)


## [0.7.3] - 2024-10-01

### Bug Fixes

- Prevent infinite recursion when source directory contains destination directory ([#116](https://github.com/max-heller/mdbook-pandoc/pull/116))


## [0.7.2] - 2024-09-14

### Bug Fixes

- Keep inline HTML inline ([#112](https://github.com/max-heller/mdbook-pandoc/pull/112))

### Miscellaneous Tasks

- Update example books ([#114](https://github.com/max-heller/mdbook-pandoc/pull/114))


## [0.7.1] - 2024-07-22

### Bug Fixes

- Check for `disabled = true` before invoking `pandoc` ([#108](https://github.com/max-heller/mdbook-pandoc/pull/108))


## [0.7.0] - 2024-07-10

### Bug Fixes

- Minimum `pandoc` version is actually 2.8, not 1.14 ([#90](https://github.com/max-heller/mdbook-pandoc/pull/90))
- Resolve links correctly when book contains exactly one chapter ([#101](https://github.com/max-heller/mdbook-pandoc/pull/101))
- Correctly resolve absolute-path links to be relative to book root ([#103](https://github.com/max-heller/mdbook-pandoc/pull/103))
- [**breaking**] Work around Pandoc 3.2+ breaking links to chapters ([#100](https://github.com/max-heller/mdbook-pandoc/pull/100))
- Replace unresolvable remote images with their descriptions ([#105](https://github.com/max-heller/mdbook-pandoc/pull/105))
- Fix EPUB conversion with HTML elements spanning multiple blocks ([#106](https://github.com/max-heller/mdbook-pandoc/pull/106))

### Changes

- Raise minimum supported Rust version to 1.74 ([#104](https://github.com/max-heller/mdbook-pandoc/pull/104))

### Features

- Preserve escape characters (or lack thereof) from Markdown source ([#95](https://github.com/max-heller/mdbook-pandoc/pull/95))
- Allow overriding source format and extensions through Pandoc's `from` option ([#98](https://github.com/max-heller/mdbook-pandoc/pull/98))
- `disabled` flag to disable rendering even if `mdbook-pandoc` is available ([#93](https://github.com/max-heller/mdbook-pandoc/pull/93))

### Miscellaneous Tasks

- Fix date in CHANGELOG ([#86](https://github.com/max-heller/mdbook-pandoc/pull/86))
- Update `cargo-dist` ([#92](https://github.com/max-heller/mdbook-pandoc/pull/92))
- Update example rendered books ([#107](https://github.com/max-heller/mdbook-pandoc/pull/107))


## [0.6.4] - 2024-04-07

### Bug Fixes

- Fix `withBinaryFile` errors on Windows by normalizing paths with `normpath` instead of `std::fs::canonicalize()` ([#84](https://github.com/max-heller/mdbook-pandoc/pull/84))


## [0.6.3] - 2024-04-06

### Bug Fixes

- Correctly parse `pandoc` versions with fewer than three components ([#82](https://github.com/max-heller/mdbook-pandoc/pull/82))

## [0.6.2] - 2024-03-21

### Changes

- Upgrade `pulldown-cmark-to-cmark` to 13.0 ([#77](https://github.com/max-heller/mdbook-pandoc/pull/77))

### Documentation

- Improve rendering of example books ([#79](https://github.com/max-heller/mdbook-pandoc/pull/79))

## [0.6.1] - 2024-03-19

### Features

- Hide/show hidden lines in code blocks ([#76](https://github.com/max-heller/mdbook-pandoc/pull/76))

## [0.6.0] - 2024-03-16

### Bug Fixes

- Replace redirects that can't be resolved with links to hosted HTML ([#67](https://github.com/max-heller/mdbook-pandoc/pull/67))

### Features

- [**breaking**] Upgrade `pulldown-cmark` to 0.10 and `pulldown-cmark-to-cmark` to 12.0 ([#70](https://github.com/max-heller/mdbook-pandoc/pull/70))

  This is not an API-breaking change but involves significant changes to the Commonmark parser and renderer and may therefore result in changes to rendered books.

- Wrap long lines in code blocks ([#60](https://github.com/max-heller/mdbook-pandoc/pull/60))

## [0.5.0] - 2024-02-10

### Changes

- Bump minimum supported Rust version (MSRV) to 1.71

### Features

- Emulate [Pandoc's cell-wrapping behavior for tables](https://pandoc.org/MANUAL.html#extension-pipe_tables) to prevent wide tables from overflowing the page ([#63](https://github.com/max-heller/mdbook-pandoc/pull/63))
- Added a `hosted-html` option to specify the URL of a hosted HTML version of the book. If set, relative links that can't be resolved within the book will be translated to links to the hosted version of the book ([#66](https://github.com/max-heller/mdbook-pandoc/pull/66))

## [0.4.2] - 2024-01-23

### Miscellaneous Tasks

- Update example book submodules ([#61](https://github.com/max-heller/mdbook-pandoc/pull/61))

## [0.4.1] - 2024-01-14

### Bug Fixes

- Correctly number chapters in the presence of prefix/suffix chapters and multiple top-level headings per chapter ([#58](https://github.com/max-heller/mdbook-pandoc/pull/58))
- Don't nest suffix chapters under most recent book part in PDF bookmarks ([#59](https://github.com/max-heller/mdbook-pandoc/pull/59))

### Documentation

- Correct `mdbook-pdf` link in README ([#54](https://github.com/max-heller/mdbook-pandoc/pull/54))

### Miscellaneous Tasks

- Update `cargo-dist` to v0.7.1 ([#56](https://github.com/max-heller/mdbook-pandoc/pull/56))

## [0.4.0] - 2024-01-07

### Changes

- [**breaking**] Options are now passed to Pandoc as a [defaults file](https://pandoc.org/MANUAL.html#defaults-files) instead of as command-line arguments ([#50](https://github.com/max-heller/mdbook-pandoc/pull/50))

  As a result, some options must be specified with different names--in particular, the output file should now be specified as `output-file` instead of `output`.

### Features

- Pass metadata from `[book]` table to Pandoc ([#53](https://github.com/max-heller/mdbook-pandoc/pull/53))

### Miscellaneous Tasks

- Use Noto fonts in LaTeX tests ([#48](https://github.com/max-heller/mdbook-pandoc/pull/48))

## [0.3.2] - 2024-01-03

### Bug Fixes

- Correctly check mdBook version compatibility ([#45](https://github.com/max-heller/mdbook-pandoc/pull/45))

## [0.3.1] - 2023-12-27

### Bug Fixes

- Support lists nested more than four levels deep when rendering to LaTeX ([#40](https://github.com/max-heller/mdbook-pandoc/pull/40))

## [0.3.0] - 2023-12-25

### Changes

- [**breaking**] Run `pandoc` with mdBook root as working directory ([#34](https://github.com/max-heller/mdbook-pandoc/pull/34))

### Features

- Support older versions of Pandoc (with possibly degraded output) ([#37](https://github.com/max-heller/mdbook-pandoc/pull/37))

### Miscellaneous Tasks

- Update pandoc version used for testing from v3.1.9 -> v3.1.11 ([#31](https://github.com/max-heller/mdbook-pandoc/pull/31))

## [0.2.1] - 2023-12-22

### Documentation

- List support for `[output.html.redirect]` under features section of README ([#28](https://github.com/max-heller/mdbook-pandoc/pull/28))
- Link to sample rendered books in README ([#30](https://github.com/max-heller/mdbook-pandoc/pull/30))

### Miscellaneous Tasks

- Specify `pandoc` and `rsvg-convert` as dependencies in `cargo-dist` config ([#29](https://github.com/max-heller/mdbook-pandoc/pull/29))
- Upload example rendered books to releases ([#27](https://github.com/max-heller/mdbook-pandoc/pull/27))

## [0.2.0] - 2023-12-08

### Bug Fixes

- Download remote images that pandoc doesn't handle on its own ([#24](https://github.com/max-heller/mdbook-pandoc/pull/24))

### Features

- Allow configuring logging with `RUST_LOG` environment variable ([#21](https://github.com/max-heller/mdbook-pandoc/pull/21))
- Take `[output.html.redirect]` into account when resolving links ([#20](https://github.com/max-heller/mdbook-pandoc/pull/20))

## [0.1.3] - 2023-12-05

### Bug Fixes

- Correctly identify profiles as LaTeX when output file is PDF and no PDF engine or output format is specified ([#14](https://github.com/max-heller/mdbook-pandoc/pull/14))
- Syntax highlighting for Rust code blocks with mdBook attributes ([#16](https://github.com/max-heller/mdbook-pandoc/pull/16))

### Documentation

- Document known issue with images located at URLs that have `.yml` path extensions ([#17](https://github.com/max-heller/mdbook-pandoc/pull/17))

## [0.1.2] - 2023-12-01

### Bug Fixes

- Escape quotes within link titles ([#6](https://github.com/max-heller/mdbook-pandoc/pull/6))

### Performance

- Eliminate redundant processing of tags during start and end events ([#10](https://github.com/max-heller/mdbook-pandoc/pull/10))

### Miscellaneous Tasks

- Only include necessary files in released packages ([#8](https://github.com/max-heller/mdbook-pandoc/pull/8))
- Run CI clippy workflow on Windows and MacOS in addition to Ubuntu ([#7](https://github.com/max-heller/mdbook-pandoc/pull/7))
- Update version of `cargo-dist` used to generate releases ([#12](https://github.com/max-heller/mdbook-pandoc/pull/12))
- Fix CI release workflow ([#13](https://github.com/max-heller/mdbook-pandoc/pull/13))

## [0.1.1] - 2023-11-23

### Documentation

- Include command to install latest version published to crates.io ([#3](https://github.com/max-heller/mdbook-pandoc/pull/3))
- Add crates.io badge ([#5](https://github.com/max-heller/mdbook-pandoc/pull/5))

### Miscellaneous Tasks

- Run normal workflows on release PRs ([#4](https://github.com/max-heller/mdbook-pandoc/pull/4))

## [0.1.0] - 2023-11-23

Initial release
