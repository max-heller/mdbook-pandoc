# `mdbook-pandoc` &emsp; [![Latest Version]][crates.io]

[Latest Version]: https://img.shields.io/crates/v/mdbook-pandoc.svg
[crates.io]: https://crates.io/crates/mdbook-pandoc

A [`pandoc`](https://pandoc.org)-powered [`mdbook`](https://github.com/rust-lang/mdBook) backend.
By relying on pandoc, many output formats are supported, although this project was mainly developed with LaTeX in mind.

See [Rendered Books](#rendered-books) for samples of rendered books.

## Installation

- [Install `mdbook`](https://rust-lang.github.io/mdBook/guide/installation.html)

- Install `mdbook-pandoc`:

  To install the latest release published to [crates.io](https://crates.io/crates/mdbook-pandoc):

  ```sh
  cargo install mdbook-pandoc --locked
  ```

  The install the latest version committed to GitHub:

  ```sh
  cargo install mdbook-pandoc --git https://github.com/max-heller/mdbook-pandoc.git --locked
  ```

- [Install `pandoc`](https://pandoc.org/installing.html)

  > **Note**: `mdbook-pandoc` works best with Pandoc 2.10.1 (released July 2020) or newer -- ideally, the newest version you have access to.
  > Older versions (as old as 2.8, released Nov 2019) are partially supported, but will result in degraded output.
  >
  > If you have an old version of Pandoc installed (in particular, Ubuntu releases before 23.04 have older-than-recommended Pandoc versions in their package repositories), consider downloading a newer version from Pandoc's installation page.

## Getting Started

Instruct `mdbook` to use `mdbook-pandoc` by updating your `book.toml` file.
The following example configures `mdbook-pandoc` to generate a PDF version of the book with LaTeX (which must be [installed](https://www.latex-project.org/get)).
To generate other output formats, see [Configuration](#configuration).

```diff
[book]
title = "My First Book"

+ [output.pandoc.profile.pdf]
+ output-file = "output.pdf"
+ to = "latex"
```

Running `mdbook build` will write the rendered book to `pdf/output.pdf` in `mdbook-pandoc`'s [build directory](https://rust-lang.github.io/mdBook/format/configuration/renderers.html#output-tables) (`book/pandoc` if multiple renderers are configured; `book` otherwise).

## Configuration

Since `mdbook-pandoc` supports many different output formats through `pandoc`, it must be configured to render to one or more formats through the `[output.pandoc]` table in a book's `book.toml` file.

Configuration is centered around *output profiles*, named packages of options that `mdbook-pandoc` passes to `pandoc` as a [*defaults file*](https://pandoc.org/MANUAL.html#defaults-files) to render a book in a particular format.
The output for each profile is written to a subdirectory with the same name as the profile under `mdbook-pandoc`'s top-level [build directory](https://rust-lang.github.io/mdBook/format/configuration/renderers.html#output-tables) (`book/pandoc` if multiple renderers are configured; `book` otherwise).

A subset of the available options are described below:

> **Note:** Pandoc is run from the book's root directory (the directory containing `book.toml`).
> Therefore, relative paths in the configuration (e.g. values for `include-in-header`, `reference-doc`) should be written relative to the book's root directory.

```toml
[output.pandoc]
number-internal-headings = false # number headings inside of chapters
list-internal-headings = false # list internal headings in the table of contents

[output.pandoc.markdown.extensions] # enable additional Markdown extensions
math = false # parse inline ($a^b$) and display ($$a^b$$) math
superscript = false # parse superscripted text (^this is superscripted^)
subscript = false # parse subscripted text (~this is subscripted~)

[output.pandoc.code]
# Display hidden lines in code blocks (e.g., lines in Rust blocks prefixed by '#').
# See https://rust-lang.github.io/mdBook/format/mdbook.html?highlight=hidden#hiding-code-lines
show-hidden-lines = false

[output.pandoc.profile.<name>] # options to pass to Pandoc (see https://pandoc.org/MANUAL.html#defaults-files)
output-file = "output.pdf" # output file (within the profile's build directory)
to = "latex" # output format

# PDF-specific settings
pdf-engine = "pdflatex" # engine to use to produce PDF output

# `mdbook-pandoc` overrides Pandoc's defaults for the following options to better support mdBooks
file-scope = true # parse each file individually before combining
number-sections = true # number sections headings
standalone = true # produce output with an appropriate header and footer
table-of-contents = true # include an automatically generated table of contents

# Arbitrary other Pandoc options can be specified as they would be in a Pandoc defaults file
# (see https://pandoc.org/MANUAL.html#defaults-files) but written in TOML instead of YAML...

# For example, to pass variables (https://pandoc.org/MANUAL.html#variables):
[output.pandoc.profile.<name>.variables]
# Set the pandoc variable named 'variable-name' to 'value'
variable-name = "value"
```

## Features

- [Markdown extensions supported by mdBook](https://rust-lang.github.io/mdBook/format/markdown.html#extensions)
  - [Strikethrough](https://rust-lang.github.io/mdBook/format/markdown.html#strikethrough) (e.g. `~~crossed out~~`)
  - [Footnotes](https://rust-lang.github.io/mdBook/format/markdown.html#footnotes)
  - [Tables](https://rust-lang.github.io/mdBook/format/markdown.html#tables)
  - [Task Lists](https://rust-lang.github.io/mdBook/format/markdown.html#task-lists) (e.g. `- [x] Complete task`)
  - [Heading Attributes](https://rust-lang.github.io/mdBook/format/markdown.html#heading-attributes) (e.g. `# Heading { #custom-heading }`)
  - [Definition Lists](https://rust-lang.github.io/mdBook/format/markdown.html#definition-lists)
  - [Admonitions](https://rust-lang.github.io/mdBook/format/markdown.html#admonitions)
- Markdown extensions not yet supported by mdBook

  These extensions are disabled by default for consistency with mdBook and must be explicitly enabled.
  - [Math](https://github.com/pulldown-cmark/pulldown-cmark/blob/v0.13.0/pulldown-cmark/specs/math.txt)
    (Enabled by `output.pandoc.markdown.extensions.math`)
  - [Superscript](https://github.com/pulldown-cmark/pulldown-cmark/blob/v0.13.0/pulldown-cmark/specs/super_sub.txt)
    (Enabled by `output.pandoc.markdown.extensions.superscript`)
  - [Subscript](https://github.com/pulldown-cmark/pulldown-cmark/blob/v0.13.0/pulldown-cmark/specs/super_sub.txt)
    (Enabled by `output.pandoc.markdown.extensions.subscript`)
- Raw HTML (best effort, almost always lossy)
  - Linking to HTML elements by `id`
  - Strikthrough (`<s>`), superscript (`<sup>`), subscript (`<sub>`)
  - Definition lists (`<dl>`, `<dt>`, `<dd>`)
  - Images (`<img>`) with `width` and `height` attributes
    - Class-based CSS styling (`width`/`height`)
  - `<span>`s, `<div>`s, and `<figure>`s
  - Anchors (`<a>`)
- Table of contents
- MathJax emulation (TeX only)

  When `output.html.mathjax-support = true`, the following patterns are parsed
  as mathematical expressions and rendered using Pandoc's math nodes:
  - `InlineMath`:  `\\(...\\)`
  - `DisplayMath`: `\\[...\\]` and `$$...$$`

  **Note:** the `pulldown-cmark`-based math support (`$...$` and `$$...$$`)
  enabled by `output.pandoc.markdown.extensions.math = true` takes precedence
  over MathJax emulation. Parsing behavior is slightly different between the
  two--in particular, a line break is `\\` in the `pulldown-cmark` variant,
  whereas it must be written as `\\\\` in the MathJax variant.
- Redirects ([`[output.html.redirect]`](https://rust-lang.github.io/mdBook/format/configuration/renderers.html#outputhtmlredirect))
- Font Awesome 6.2.0 icons (e.g. `<i class="fas fa-github"></i>`)
  - Best effort support for Font Awesome 4 icons (e.g. `<i class="fa fa-github"></i>`)

## Rendering Pipeline

To render a book, `mdbook-pandoc` parses the book's source ([Parsing](#parsing)), transforms it into Pandoc's native representation ([Preprocessing](#preprocessing)), then runs `pandoc` to render the book in the desired output format.

### Parsing

#### HTML

`mdbook-pandoc` does its best to support raw HTML embedded in Markdown documents, transformating it into relevant Pandoc AST elements where possible.
Each chapter is parsed into a hybrid Markdown+HTML tree using [`pulldown-cmark`](https://crates.io/crates/pulldown-cmark) and the browser-grade [`html5ever`](https://crates.io/crates/html5ever) HTML parser.
This approach captures the full structure of the document -- including implicitly closed elements and other HTML quirks -- and makes it possible to accurately render HTML elements containing Markdown elements containing HTML elements...

This approach *should* also make `mdbook-pandoc` better able to handle malformed HTML, since `html5ever` performs the same HTML sanitization magic that browsers do.
However, the standard principle applies: garbage in, garbage out; for best results, write simple and obviously correct HTML.

### Preprocessing

#### Structural Changes

- In order to make section numbers and the generated table of contents, if applicable, mirror the chapter hierarchy defined in `SUMMARY.md`:
  - Heading levels are adjusted such that the largest heading in a top-level chapter is a H1, the largest heading in a singly-nested chapter is an H2, the largest heading in a doubly-nested chapter is an H3, etc.
  - All headings except for the first in each chapter are excluded from numbering and the table of contents
- Relative links within chapters are "rebased" to be relative to the source directory so a chapter `src/foo/foo.md` can link to `src/foo/bar.md` with `[bar](bar.md)`

## Known Issues

- Linking to a chapter does not work unless the chapter contains a heading with a non-empty identifier (either auto-generated or explicitly specified).
  See: https://github.com/max-heller/mdbook-pandoc/pull/100

## Comparison to alternatives

### Rendered books

The following table links to sample books rendered with `mdbook-pandoc`.
PDFs are rendered with LaTeX ([LuaTeX](https://en.wikipedia.org/wiki/LuaTeX)).

| Book | Rendered |
| ---- | -------- |
| [Cargo Book](https://doc.rust-lang.org/stable/cargo/) | [PDF](https://github.com/max-heller/mdbook-pandoc/releases/latest/download/rendered-cargo-book.pdf) |
| [mdBook Guide](https://rust-lang.github.io/mdBook/) | [PDF](https://github.com/max-heller/mdbook-pandoc/releases/latest/download/rendered-mdBook-guide.pdf) |
| [Rustonomicon](https://doc.rust-lang.org/nomicon/) | [PDF](https://github.com/max-heller/mdbook-pandoc/releases/latest/download/rendered-nomicon.pdf) |
| [Rust Book](https://doc.rust-lang.org/book/) | [PDF](https://github.com/max-heller/mdbook-pandoc/releases/latest/download/rendered-rust-book.pdf) |
| [Rust by Example](https://doc.rust-lang.org/rust-by-example/) | [PDF](https://github.com/max-heller/mdbook-pandoc/releases/latest/download/rendered-rust-by-example.pdf) |
| [Rust Edition Guide](https://doc.rust-lang.org/edition-guide/) | [PDF](https://github.com/max-heller/mdbook-pandoc/releases/latest/download/rendered-rust-edition-guide.pdf) |
| [Embedded Rust Book](https://docs.rust-embedded.org/book/) | [PDF](https://github.com/max-heller/mdbook-pandoc/releases/latest/download/rendered-rust-embedded.pdf) |
| [Rust Reference](https://doc.rust-lang.org/reference/) | [PDF](https://github.com/max-heller/mdbook-pandoc/releases/latest/download/rendered-rust-reference.pdf) |
| [Rust Compiler Development Guide](https://rustc-dev-guide.rust-lang.org/) | [PDF](https://github.com/max-heller/mdbook-pandoc/releases/latest/download/rendered-rustc-dev-guide.pdf) |

### Rendering to PDF

- When `mdbook-pandoc` was initially written, existing `mdbook` LaTeX backends ([`mdbook-latex`](https://crates.io/crates/mdbook-latex), [`mdbook-tectonic`](https://crates.io/crates/mdbook-tectonic)) were not mature enough to render much besides the simplest books due to hand-rolling the markdown->LaTeX conversion step.
  `mdbook-pandoc`, on the other hand, delegates this difficult step to pandoc, inheriting its maturity and configurability.
- "Print to PDF"-based backends like [`mdbook-pdf`](https://crates.io/crates/mdbook-pdf) are more mature, but produce less aesthetically-pleasing PDFs.
  Additionally, `mdbook-pdf` does not support intra-document links or generating a table of contents without using a [forked version of mdbook](https://github.com/rust-lang/mdBook/pull/1738).

### Rendering to other formats

- By delegating most of the difficult rendering work to pandoc, `mdbook-pandoc` supports [numerous output formats](https://pandoc.org/MANUAL.html#option--to).
  Most of these have not been tested, so feedback on how it performs on non-PDF formats is very welcome!
