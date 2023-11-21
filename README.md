# `mdbook-pandoc`

A [`mdbook`](https://github.com/rust-lang/mdBook) backend supporting many output formats by relying on [`pandoc`](https://pandoc.org).

## Installation

- [Install `mdbook`](https://rust-lang.github.io/mdBook/guide/installation.html)

- Install `mdbook-pandoc`:

  ```sh
  cargo install --git https://github.com/max-heller/mdbook-pandoc.git mdbook-pandoc
  ```

- [Install `pandoc`](https://pandoc.org/installing.html)

## Getting Started

The following example demonstrates configuring `mdbook-pandoc` to generate a PDF version of a book with LaTeX (which must be [installed](https://www.latex-project.org/get)).
To generate other output formats, see [Configuration](#configuration).

```diff
[book]
title = "My First Book"

+ [output.pandoc.profile.pdf]
+ output = "output.pdf"
+ to = "latex"
```

Running `mdbook build` will write the rendered book to `pdf/output.pdf` in `mdbook-pandoc`'s [build directory](https://rust-lang.github.io/mdBook/format/configuration/renderers.html#output-tables) (`book/pandoc` if multiple renderers are configured; `book` otherwise).

## Configuration

Since `mdbook-pandoc` supports many different output formats through `pandoc`, it must be configured to render to one or more formats through the `[output.pandoc]` table in a book's `book.toml` file.
Configuration is centered around *output profiles*, sets of arguments that `mdbook-pandoc` passed to `pandoc` to render a book in a particular format.

The available settings are described below:

```toml
[output.pandoc]

[output.pandoc.profile.<name>] # arguments to pass to `pandoc` (see https://pandoc.org/MANUAL.html)
output = "output.pdf" # output file
pdf-engine = "pdflatex" # engine to use to produce PDF output
to = "latex" # output format

# The following settings have sane defaults and should usually not need to be overridden

columns = 72 # line length in characters
file-scope = true # parse each file individually before combining
number-sections = true # number sections headings
standalone = true # produce output with an appropriate header and footer
table-of-contents = true # include an automatically generated table of contents
toc-depth = 3 # number of section levels to include in table of contents

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
- [x] Font Awesome 4 icons (e.g. `<i class="fa fa-github"></i>`)

### Known Issues

- When rendering to PDF through LaTeX, links to chapters sometimes link to slightly before the beginning of the chapter.
  See: https://github.com/jgm/pandoc/issues/9200
- When rendering to PDF through LaTeX, tables with long rows will overflow the width of the page.
  This is due to usage of pandoc's CommonMark parser, which doesn't yet support the functionality
  needed to wrap cell contents that is implemented in the Pandoc Markdown parser.
  See: https://github.com/jgm/commonmark-hs/issues/128
