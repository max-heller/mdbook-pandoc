use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn images() {
    let book = MDBook::init()
        .config(Config::latex())
        .file_in_src("img/image.png", "")
        .chapter(Chapter::new(
            "",
            indoc!{r#"
                ![alt text](img/image.png "a title")
                <img src="img/image.png" alt="alt text" title = "a title" width=50 height=100 class="foo bar">
                <img src="img/image.png" alt="alt text" title = "a title" style="width:50; height: 100" class="foo bar">
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
    │ \pandocbounded{\includegraphics[keepaspectratio]{book/latex/src/img/image.png}}
    │ \includegraphics[width=0.52083in,height=1.04167in]{book/latex/src/img/image.png}
    │ \includegraphics[width=0.52083in,height=1.04167in]{book/latex/src/img/image.png}
    ├─ latex/src/chapter.md
    │ [Para [Image ("", [], []) [Str "alt text"] ("book/latex/src/img/image.png", "a title"), SoftBreak, Image ("", ["foo", "bar"], [("height", "100"), ("width", "50")]) [Str "alt text"] ("book/latex/src/img/image.png", "a title"), SoftBreak, Image ("", ["foo", "bar"], [("width", "50"), ("height", "100")]) [Str "alt text"] ("book/latex/src/img/image.png", "a title")]]
    ├─ latex/src/img/image.png
    "#);
}

#[test]
#[ignore]
fn remote_images() {
    let book = MDBook::init()
        .config(Config::pdf())
        .chapter(Chapter::new(
            "",
            indoc!{r#"
                [![Build](https://github.com/rust-lang/mdBook/workflows/CI/badge.svg?event=push)](https://github.com/rust-lang/mdBook/actions?query=workflow%3ACI+branch%3Amaster)
                [![Build](https://img.shields.io/github/actions/workflow/status/rust-lang/mdBook/main.yml?style=flat-square)](https://github.com/rust-lang/mdBook/actions/workflows/main.yml?query=branch%3Amaster)
                [![crates.io](https://img.shields.io/crates/v/mdbook.svg)](https://crates.io/crates/mdbook)
                [![GitHub contributors](https://img.shields.io/github/contributors/rust-lang/mdBook?style=flat-square)](https://github.com/rust-lang/mdBook/graphs/contributors)
                [![GitHub stars](https://img.shields.io/github/stars/rust-lang/mdBook?style=flat-square)](https://github.com/rust-lang/mdBook/stargazers)
            "#},
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/pdf/book.pdf    
    ├─ pdf/book.pdf
    │ <INVALID UTF8>
    ");
}

#[test]
fn unresolvable_remote_images() {
    let book = MDBook::init()
        .config(Config::markdown())
        .chapter(Chapter::new(
            "Some Chapter",
            "prefix ![test image](https://doesnotexist.fake/main.yml?style=flat-square) suffix",
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  WARN mdbook_pandoc::preprocess: Failed to resolve image link 'https://doesnotexist.fake/main.yml?style=flat-square' in chapter 'Some Chapter': could not fetch remote image: Dns Failed    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
    ├─ markdown/book.md
    │ prefix test image suffix
    ");
}
