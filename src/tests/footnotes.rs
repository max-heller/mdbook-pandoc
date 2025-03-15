use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn footnotes() {
    // Pandoc doesn't support nested footnotes (it won't output anything meaningful for them)
    // but we output the AST for them anyway. See https://github.com/jgm/pandoc/issues/2053
    let book = MDBook::init()
        .chapter(Chapter::new(
            "",
            indoc! {"
                    hello[^1] world

                    [^1]: a footnote containing another footnote[^2]
                    [^2]: second footnote
                "},
            "chapter.md",
        ))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
    ├─ latex/output.tex
    │ hello\footnote{a footnote containing another footnote\footnotemark{}} world
    ├─ latex/src/chapter.md
    │ [Para [Str "hello", Note [Para [Str "a footnote containing another footnote", Note [Para [Str "second footnote"]]]], Str " world"]]
    "#);
}

#[test]
fn footnote_cycle() {
    let output = MDBook::init()
        .chapter(Chapter::new(
            "",
            "[^1]\n\n[^1]: [^2]\n\n[^2]: [^1]",
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(output, @r"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  WARN mdbook_pandoc::preprocess::tree: Cycle in footnote definitions: 1 => 2 => 1    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
    ├─ markdown/book.md
    │ [^1]
    │ 
    │ [^1]: [^2]
    ");
}

#[test]
fn footnotes_get_preprocessed() {
    let book = MDBook::init()
        .config(Config::pandoc())
        .chapter(Chapter::new(
            "",
            indoc! {r#"
                hello[^1]

                [^1]: a footnote containing another footnote[^2]
                [^2]: <a href="example.com"></a>
            "#},
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir    
    ├─ markdown/pandoc-ir
    │ [ Para
    │     [ Str "hello"
    │     , Note
    │         [ Para
    │             [ Str "a footnote containing another footnote"
    │             , Note
    │                 [ Para
    │                     [ Link
    │                         ( "" , [] , [ ( "href" , "example.com" ) ] )
    │                         []
    │                         ( "example.com" , "" )
    │                     ]
    │                 ]
    │             ]
    │         ]
    │     ]
    │ ]
    "#);
}
