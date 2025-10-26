use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn broken_links() {
    let book = MDBook::init()
        .chapter(Chapter::new(
            "Getting Started",
            "[broken link](foobarbaz)",
            "getting-started.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  WARN mdbook_pandoc::preprocess: Unable to normalize link 'foobarbaz' in chapter 'Getting Started': Unable to normalize path: $ROOT/src/foobarbaz: No such file or directory (os error 2)
    │  WARN mdbook_pandoc: Failed to resolve one or more relative links within the book; consider setting the `site-url` option in `[output.html]`
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md
    ├─ markdown/book.md
    │ [broken link](foobarbaz)
    ");

    let book = MDBook::init()
        .chapter(Chapter::new(
            "Getting Started",
            "[broken link](foobarbaz)",
            "getting-started.md",
        ))
        .site_url("example.com/book")
        .build();
    insta::assert_snapshot!(book, @r"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  INFO mdbook_pandoc::preprocess: Failed to resolve link 'foobarbaz' in chapter 'getting-started.md', linking to hosted HTML book at 'example.com/book/foobarbaz'
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md
    ├─ markdown/book.md
    │ [broken link](example.com/book/foobarbaz)
    ");
}

#[test]
fn link_title_containing_quotes() {
    let book = MDBook::init()
        .config(Config::latex())
        .chapter(Chapter::new(
            "",
            indoc! {r#"
                # Chapter Foo

                [link][link-with-description]

                [link-with-description]: chapter.md '"foo" (bar)'
            "#},
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \chapter{Chapter Foo}\label{book__latex__src__chapter.md__chapter-foo}
    │ 
    │ \hyperref[book__latex__src__chapter.md__chapter-foo]{link}
    ├─ latex/src/chapter.md
    │ [Header 1 ("chapter-foo", [], []) [Str "Chapter Foo"], Para [Link ("", [], []) [Str "link"] ("book/latex/src/chapter.md#chapter-foo", "\"foo\" (bar)")]]
    "#);
}

#[test]
fn single_chapter_with_explicit_self_link() {
    let book = MDBook::init()
        .config(Config::latex())
        .chapter(Chapter::new(
            "Chapter One",
            "# Chapter One\n[link](chapter.md)",
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \chapter{Chapter One}\label{book__latex__src__chapter.md__chapter-one}
    │ 
    │ \hyperref[book__latex__src__chapter.md__chapter-one]{link}
    ├─ latex/src/chapter.md
    │ [Header 1 ("chapter-one", [], []) [Str "Chapter One"], Para [Link ("", [], []) [Str "link"] ("book/latex/src/chapter.md#chapter-one", "")]]
    "#);
}

#[test]
fn inter_chapter_links() {
    let book = MDBook::init()
        .chapter(Chapter::new(
            "One",
            "# One\n[Two](../two/two.md)",
            "one/one.md",
        ))
        .chapter(Chapter::new(
            "Two",
            "# Two\n[One](../one/one.md)\n[also one](/one/one.md)\n[Three](../three.md)",
            "two/two.md",
        ))
        .chapter(Chapter::new("Three", "", "three.md"))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  WARN mdbook_pandoc::preprocess: Failed to determine suitable anchor for beginning of chapter 'Three'--does it contain any headings?
    │  WARN mdbook_pandoc::preprocess: Unable to normalize link '../three.md' in chapter 'Two': failed to link to beginning of chapter
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \chapter{One}\label{book__latex__src__one__one.md__one}
    │ 
    │ \hyperref[book__latex__src__two__two.md__two]{Two}
    │ 
    │ \chapter{Two}\label{book__latex__src__two__two.md__two}
    │ 
    │ \hyperref[book__latex__src__one__one.md__one]{One}
    │ \hyperref[book__latex__src__one__one.md__one]{also one}
    │ \href{../three.md}{Three}
    ├─ latex/src/one/one.md
    │ [Header 1 ("one", [], []) [Str "One"], Para [Link ("", [], []) [Str "Two"] ("book/latex/src/two/two.md#two", "")]]
    ├─ latex/src/three.md
    │ []
    ├─ latex/src/two/two.md
    │ [Header 1 ("two", [], []) [Str "Two"], Para [Link ("", [], []) [Str "One"] ("book/latex/src/one/one.md#one", ""), SoftBreak, Link ("", [], []) [Str "also one"] ("book/latex/src/one/one.md#one", ""), SoftBreak, Link ("", [], []) [Str "Three"] ("../three.md", "")]]
    "#);
}

#[test]
fn percent_encoding() {
    let book = MDBook::init()
        .chapter(Chapter::new(
            "One",
            "# One\n[Two](../two/chapter%20two.md)",
            "one/one.md",
        ))
        .chapter(Chapter::new("Two", "# Two", "two/chapter two.md"))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \chapter{One}\label{book__latex__src__one__one.md__one}
    │ 
    │ \hyperref[book__latex__src__two__chapter-two.md__two]{Two}
    │ 
    │ \chapter{Two}\label{book__latex__src__two__chapter-two.md__two}
    ├─ latex/src/one/one.md
    │ [Header 1 ("one", [], []) [Str "One"], Para [Link ("", [], []) [Str "Two"] ("book/latex/src/two/chapter%20two.md#two", "")]]
    ├─ latex/src/two/chapter two.md
    │ [Header 1 ("two", [], []) [Str "Two"]]
    "#);
}
