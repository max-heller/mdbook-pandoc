# `mdbook-pandoc`

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
