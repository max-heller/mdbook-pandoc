use super::{Chapter, Config, MDBook};

#[test]
fn heading_attributes() {
    let book = MDBook::init()
        .chapter(Chapter::new(
            "",
            "# Heading { #custom-heading }\n[heading](#custom-heading)",
            "chapter.md",
        ))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(book, @r##"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \hypertarget{book__latex__src__chapter.md}{}
    │ \hypertarget{book__latex__src__chapter.md__custom-heading}{%
    │ \chapter{Heading}\label{book__latex__src__chapter.md__custom-heading}}
    │ 
    │ \protect\hyperlink{book__latex__src__chapter.md__custom-heading}{heading}
    │ 
    │ \hypertarget{book__latex__dummy}{}
    ├─ latex/src/chapter.md
    │ [Header 1 ("custom-heading", [], []) [Str "Heading"], Para [Link ("", [], []) [Str "heading"] ("#custom-heading", "")]]
    "##);
}

#[test]
fn nested_chapters() {
    let book = MDBook::init()
        .chapter(Chapter::new("One", "# One", "one.md").child(Chapter::new(
            "One.One",
            "# Top\n## Another",
            "onepointone.md",
        )))
        .chapter(Chapter::new("Two", "# Two", "two.md"))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \hypertarget{book__latex__src__one.md}{}
    │ \hypertarget{book__latex__src__one.md__one}{%
    │ \chapter{One}\label{book__latex__src__one.md__one}}
    │ 
    │ \hypertarget{book__latex__src__onepointone.md}{}
    │ \hypertarget{book__latex__src__onepointone.md__top}{%
    │ \section{Top}\label{book__latex__src__onepointone.md__top}}
    │ 
    │ \hypertarget{book__latex__src__onepointone.md__another}{%
    │ \subsection*{Another}\label{book__latex__src__onepointone.md__another}}
    │ 
    │ \hypertarget{book__latex__src__two.md}{}
    │ \hypertarget{book__latex__src__two.md__two}{%
    │ \chapter{Two}\label{book__latex__src__two.md__two}}
    ├─ latex/src/one.md
    │ [Header 1 ("one", [], []) [Str "One"]]
    ├─ latex/src/onepointone.md
    │ [Header 2 ("top", [], []) [Str "Top"], Header 3 ("another", ["unnumbered", "unlisted"], []) [Str "Another"]]
    ├─ latex/src/two.md
    │ [Header 1 ("two", [], []) [Str "Two"]]
    "#);

    let book =
        MDBook::init()
            .chapter(Chapter::new("One", "# One", "one.md").child(
                Chapter::new("One.One", "## Top\n### Another", "onepointone.md").child(
                    Chapter::new("One.One.One", "### Top\n#### Another", "onepointoneone.md"),
                ),
            ))
            .chapter(Chapter::new("Two", "# Two", "two.md"))
            .config(Config::latex())
            .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \chapter{One}\label{book__latex__src__one.md__one}
    │ 
    │ \section{Top}\label{book__latex__src__onepointone.md__top}
    │ 
    │ \subsection*{Another}\label{book__latex__src__onepointone.md__another}
    │ 
    │ \subsection{Top}\label{book__latex__src__onepointoneone.md__top}
    │ 
    │ \subsubsection*{Another}\label{book__latex__src__onepointoneone.md__another}
    │ 
    │ \chapter{Two}\label{book__latex__src__two.md__two}
    ├─ latex/src/one.md
    │ [Header 1 ("one", [], []) [Str "One"]]
    ├─ latex/src/onepointone.md
    │ [Header 2 ("top", [], []) [Str "Top"], Header 3 ("another", ["unnumbered", "unlisted"], []) [Str "Another"]]
    ├─ latex/src/onepointoneone.md
    │ [Header 3 ("top", [], []) [Str "Top"], Header 4 ("another", ["unnumbered", "unlisted"], []) [Str "Another"]]
    ├─ latex/src/two.md
    │ [Header 1 ("two", [], []) [Str "Two"]]
    "#);
}

#[test]
fn repeated_identifiers() {
    let book = MDBook::init()
        .config(Config::pandoc())
        .chapter(Chapter::new(
            "",
            "# Hello\n# Hello\n[first](#hello)[second](#hello-1)",
            "chapter.md",
        ))
        .chapter(Chapter::new(
            "",
            "# Hello\n# Hello\n[first](#hello)[second](#hello-1)",
            "chapter2.md",
        ))
        .chapter(Chapter::new("", "# ?\n# ?\n[second](#-1)", "weird-ids.md"))
        .build();
    insta::assert_snapshot!(book, @r##"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir
    ├─ markdown/pandoc-ir
    │ [ Div
    │     ( "book__markdown__src__chapter.md" , [] , [] )
    │     [ Header
    │         1
    │         ( "book__markdown__src__chapter.md__hello" , [] , [] )
    │         [ Str "Hello" ]
    │     , Header
    │         1
    │         ( "book__markdown__src__chapter.md__hello-1"
    │         , [ "unnumbered" , "unlisted" ]
    │         , []
    │         )
    │         [ Str "Hello" ]
    │     , Para
    │         [ Link
    │             ( "" , [] , [] )
    │             [ Str "first" ]
    │             ( "#book__markdown__src__chapter.md__hello" , "" )
    │         , Link
    │             ( "" , [] , [] )
    │             [ Str "second" ]
    │             ( "#book__markdown__src__chapter.md__hello-1" , "" )
    │         ]
    │     ]
    │ , Div
    │     ( "book__markdown__src__chapter2.md" , [] , [] )
    │     [ Header
    │         1
    │         ( "book__markdown__src__chapter2.md__hello" , [] , [] )
    │         [ Str "Hello" ]
    │     , Header
    │         1
    │         ( "book__markdown__src__chapter2.md__hello-1"
    │         , [ "unnumbered" , "unlisted" ]
    │         , []
    │         )
    │         [ Str "Hello" ]
    │     , Para
    │         [ Link
    │             ( "" , [] , [] )
    │             [ Str "first" ]
    │             ( "#book__markdown__src__chapter2.md__hello" , "" )
    │         , Link
    │             ( "" , [] , [] )
    │             [ Str "second" ]
    │             ( "#book__markdown__src__chapter2.md__hello-1" , "" )
    │         ]
    │     ]
    │ , Div
    │     ( "book__markdown__src__weird-ids.md" , [] , [] )
    │     [ Header 1 ( "" , [] , [] ) [ Str "?" ]
    │     , Header
    │         1
    │         ( "book__markdown__src__weird-ids.md__-1"
    │         , [ "unnumbered" , "unlisted" ]
    │         , []
    │         )
    │         [ Str "?" ]
    │     , Para
    │         [ Link
    │             ( "" , [] , [] )
    │             [ Str "second" ]
    │             ( "#book__markdown__src__weird-ids.md__-1" , "" )
    │         ]
    │     ]
    │ ]
    "##);
}

#[test]
fn parts() {
    let book = MDBook::init()
        .chapter(Chapter::new("", "# One", "one.md"))
        .part("part two")
        .chapter(Chapter::new("", "# Two", "two.md"))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \hypertarget{book__latex__src__one.md}{}
    │ \hypertarget{book__latex__src__one.md__one}{%
    │ \chapter{One}\label{book__latex__src__one.md__one}}
    │ 
    │ \leavevmode\vadjust pre{\hypertarget{book__latex__src__part-1-part-two.md}{}}%
    │ \part{part two}
    │ 
    │ \hypertarget{book__latex__src__two.md}{}
    │ \hypertarget{book__latex__src__two.md__two}{%
    │ \chapter{Two}\label{book__latex__src__two.md__two}}
    ├─ latex/src/one.md
    │ [Header 1 ("one", [], []) [Str "One"]]
    ├─ latex/src/part-1-part-two.md
    │ [Para [RawInline (Format "latex") "\\part{part two}"]]
    ├─ latex/src/two.md
    │ [Header 1 ("two", [], []) [Str "Two"]]
    "#);
}
