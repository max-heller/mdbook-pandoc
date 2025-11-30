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
    │ \chapter{Heading}\label{book__latex__src__chapter.md__custom-heading}
    │ 
    │ \hyperref[book__latex__src__chapter.md__custom-heading]{heading}
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
    │ \chapter{One}\label{book__latex__src__one.md__one}
    │ 
    │ \section{Top}\label{book__latex__src__onepointone.md__top}
    │ 
    │ \subsection*{Another}\label{book__latex__src__onepointone.md__another}
    │ 
    │ \chapter{Two}\label{book__latex__src__two.md__two}
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
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
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
    │ [ Header
    │     1
    │     ( "book__markdown__src__chapter.md__hello" , [] , [] )
    │     [ Str "Hello" ]
    │ , Header
    │     1
    │     ( "book__markdown__src__chapter.md__hello-1"
    │     , [ "unnumbered" , "unlisted" ]
    │     , []
    │     )
    │     [ Str "Hello" ]
    │ , Para
    │     [ Link
    │         ( "" , [] , [] )
    │         [ Str "first" ]
    │         ( "#book__markdown__src__chapter.md__hello" , "" )
    │     , Link
    │         ( "" , [] , [] )
    │         [ Str "second" ]
    │         ( "#book__markdown__src__chapter.md__hello-1" , "" )
    │     ]
    │ , Header
    │     1
    │     ( "book__markdown__src__chapter2.md__hello" , [] , [] )
    │     [ Str "Hello" ]
    │ , Header
    │     1
    │     ( "book__markdown__src__chapter2.md__hello-1"
    │     , [ "unnumbered" , "unlisted" ]
    │     , []
    │     )
    │     [ Str "Hello" ]
    │ , Para
    │     [ Link
    │         ( "" , [] , [] )
    │         [ Str "first" ]
    │         ( "#book__markdown__src__chapter2.md__hello" , "" )
    │     , Link
    │         ( "" , [] , [] )
    │         [ Str "second" ]
    │         ( "#book__markdown__src__chapter2.md__hello-1" , "" )
    │     ]
    │ , Header 1 ( "" , [] , [] ) [ Str "?" ]
    │ , Header
    │     1
    │     ( "book__markdown__src__weird-ids.md__-1"
    │     , [ "unnumbered" , "unlisted" ]
    │     , []
    │     )
    │     [ Str "?" ]
    │ , Para
    │     [ Link
    │         ( "" , [] , [] )
    │         [ Str "second" ]
    │         ( "#book__markdown__src__weird-ids.md__-1" , "" )
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
    │ \chapter{One}\label{book__latex__src__one.md__one}
    │ 
    │ \part{part two}
    │ 
    │ \chapter{Two}\label{book__latex__src__two.md__two}
    ├─ latex/src/one.md
    │ [Header 1 ("one", [], []) [Str "One"]]
    ├─ latex/src/part-1-part-two.md
    │ [Para [RawInline (Format "latex") "\\part{part two}"]]
    ├─ latex/src/two.md
    │ [Header 1 ("two", [], []) [Str "Two"]]
    "#);
}

#[test]
fn internal_headings() {
    let build = |modify_cfg: fn(&mut Config)| {
        MDBook::init()
            .chapter(Chapter::new("One", "# One", "one.md").child(Chapter::new(
                "One.One",
                "# Top\n## Another",
                "onepointone.md",
            )))
            .chapter(Chapter::new("Two", "# Two", "two.md"))
            .config({
                let mut config = Config::latex();
                modify_cfg(&mut config);
                config
            })
            .build()
    };

    let number_internal_headings = |cfg: &mut Config| cfg.common.number_internal_headings = true;
    let list_internal_headings = |cfg: &mut Config| cfg.common.list_internal_headings = true;
    let number_and_list_internal_headings = |cfg: &mut Config| {
        cfg.common.number_internal_headings = true;
        cfg.common.list_internal_headings = true;
    };

    insta::assert_snapshot!(build(number_internal_headings), @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \chapter{One}\label{book__latex__src__one.md__one}
    │ 
    │ \section{Top}\label{book__latex__src__onepointone.md__top}
    │ 
    │ \subsection{Another}\label{book__latex__src__onepointone.md__another}
    │ 
    │ \chapter{Two}\label{book__latex__src__two.md__two}
    ├─ latex/src/one.md
    │ [Header 1 ("one", [], []) [Str "One"]]
    ├─ latex/src/onepointone.md
    │ [Header 2 ("top", [], []) [Str "Top"], Header 3 ("another", ["unlisted"], []) [Str "Another"]]
    ├─ latex/src/two.md
    │ [Header 1 ("two", [], []) [Str "Two"]]
    "#);

    insta::assert_snapshot!(build(list_internal_headings), @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \chapter{One}\label{book__latex__src__one.md__one}
    │ 
    │ \section{Top}\label{book__latex__src__onepointone.md__top}
    │ 
    │ \subsection*{Another}\label{book__latex__src__onepointone.md__another}
    │ \addcontentsline{toc}{subsection}{Another}
    │ 
    │ \chapter{Two}\label{book__latex__src__two.md__two}
    ├─ latex/src/one.md
    │ [Header 1 ("one", [], []) [Str "One"]]
    ├─ latex/src/onepointone.md
    │ [Header 2 ("top", [], []) [Str "Top"], Header 3 ("another", ["unnumbered"], []) [Str "Another"]]
    ├─ latex/src/two.md
    │ [Header 1 ("two", [], []) [Str "Two"]]
    "#);

    insta::assert_snapshot!(build(number_and_list_internal_headings), @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \chapter{One}\label{book__latex__src__one.md__one}
    │ 
    │ \section{Top}\label{book__latex__src__onepointone.md__top}
    │ 
    │ \subsection{Another}\label{book__latex__src__onepointone.md__another}
    │ 
    │ \chapter{Two}\label{book__latex__src__two.md__two}
    ├─ latex/src/one.md
    │ [Header 1 ("one", [], []) [Str "One"]]
    ├─ latex/src/onepointone.md
    │ [Header 2 ("top", [], []) [Str "Top"], Header 3 ("another", [], []) [Str "Another"]]
    ├─ latex/src/two.md
    │ [Header 1 ("two", [], []) [Str "Two"]]
    "#);
}
