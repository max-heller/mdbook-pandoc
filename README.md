# `mdbook-pandoc` &emsp; [![Latest Version]][crates.io]

[Latest Version]: https://img.shields.io/crates/v/mdbook-pandoc.svg
[crates.io]: https://crates.io/crates/mdbook-pandoc

A [`mdbook`](https://github.com/rust-lang/mdBook) backend that outsources most of the rendering process to [`pandoc`](https://pandoc.org).
By relying on pandoc, many output formats are supported, although this project was mainly developed with LaTeX in mind.

See [Rendered Books](#rendered-books) for samples of rendered books.

## Installation

- [Install `mdbook`](https://rust-lang.github.io/mdBook/guide/installation.html)

- Install `mdbook-pandoc`:

  To install the latest release published to [crates.io](https://crates.io/crates/mdbook-pandoc):

  ```sh
  cargo install mdbook-pandoc
  ```

  The install the latest version committed to GitHub:

  ```sh
  cargo install --git https://github.com/max-heller/mdbook-pandoc.git mdbook-pandoc
  ```

- [Install `pandoc`](https://pandoc.org/installing.html)

> [!NOTE]
> `mdbook-pandoc` works best with Pandoc 2.10.1 or newer.
> Older versions (as old as 2.8) are partially supported, but will result in degraded output.
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
hosted-html = "https://doc.rust-lang.org/book" # URL of a HTML version of the book

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

- [x] CommonMark + [extensions enabled by mdBook](https://rust-lang.github.io/mdBook/format/markdown.html#extensions)
  - [x] [Strikethrough](https://rust-lang.github.io/mdBook/format/markdown.html#strikethrough) (e.g. `~~crossed out~~`)
  - [x] [Footnotes](https://rust-lang.github.io/mdBook/format/markdown.html#footnotes)
  - [x] [Tables](https://rust-lang.github.io/mdBook/format/markdown.html#tables)
  - [x] [Task Lists](https://rust-lang.github.io/mdBook/format/markdown.html#task-lists) (e.g. `- [x] Complete task`)
  - [x] [Heading Attributes](https://rust-lang.github.io/mdBook/format/markdown.html#heading-attributes) (e.g. `# Heading { #custom-heading }`)
- [x] Table of contents
- [x] Take [`[output.html.redirect]`](https://rust-lang.github.io/mdBook/format/configuration/renderers.html#outputhtmlredirect) into account when resolving links
- [x] Font Awesome 4 icons (e.g. `<i class="fa fa-github"></i>`) to LaTeX

### Preprocessing

`mdbook-pandoc` performs a brief preprocessing pass before handing off a book to pandoc:

- In order to make section numbers and the generated table of contents, if applicable, mirror the chapter hierarchy defined in `SUMMARY.md`:
  - Headings in nested chapters are shrunk one level per level of nesting
  - All headings except for H1s are marked as unnumbered and unlisted
- Relative links within chapters are "rebased" to be relative to the source directory so a chapter `src/foo/foo.md` can link to `src/foo/bar.md` with `[bar](bar.md)`
  - Pandoc implements this functionality in the [`rebase_relative_paths`](https://pandoc.org/MANUAL.html#extension-rebase_relative_paths) extension, but only for native markdown links/images, so `mdbook-pandoc` reimplements it to allow for supporting raw HTML links/images in the future

### Known Issues

- Linking to a chapter does not work unless the chapter contains a heading with a non-empty identifier (either auto-generated or explicitly specified).
  See: https://github.com/max-heller/mdbook-pandoc/pull/100

### Comparison to alternatives

#### Rendered books

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

#### Rendering to PDF

- When `mdbook-pandoc` was initially written, existing `mdbook` LaTeX backends ([`mdbook-latex`](https://crates.io/crates/mdbook-latex), [`mdbook-tectonic`](https://crates.io/crates/mdbook-tectonic)) were not mature enough to render much besides the simplest books due to hand-rolling the markdown->LaTeX conversion step.
  `mdbook-pandoc`, on the other hand, outsources this difficult step to pandoc, inheriting its maturity and configurability. 
- "Print to PDF"-based backends like [`mdbook-pdf`](https://crates.io/crates/mdbook-pdf) are more mature, but produce less aesthetically-pleasing PDFs.
  Additionally, `mdbook-pdf` does not support intra-document links or generating a table of contents without using a [forked version of mdbook](https://github.com/rust-lang/mdBook/pull/1738).

#### Rendering to other formats

- By outsourcing most of the rendering process to pandoc, `mdbook-pandoc` in theory supports many different output formats.
  Most of these have not been tested, so feedback on how it performs on non-PDF formats is very welcome!
