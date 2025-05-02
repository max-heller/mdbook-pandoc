use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn html_comments() {
    let output = MDBook::init()
        .config(Config::markdown())
        .chapter(Chapter::new("", "<!-- Comment -->", "chapter.md"))
        .build();
    insta::assert_snapshot!(output, @r"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
    ├─ markdown/book.md
    │ <!-- Comment -->
    ");
}

#[test]
fn noncontiguous_html() {
    // HTML comment is noncontiguous in the source because it is nested in a block quote.
    // Parsing should handle this sanely.
    let s = indoc! {"
        > <!-- hello
        >
        > world -->
    "};
    let output = MDBook::init()
        .config(Config::pandoc())
        .chapter(Chapter::new("", s, "chapter.md"))
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir    
    ├─ markdown/pandoc-ir
    │ [ BlockQuote
    │     [ RawBlock (Format "html") "<!-- hello\n\nworld -->"
    │     , Plain [ Str "\n" ]
    │     ]
    │ ]
    "#);
}

#[test]
fn matched_html_tags() {
    let ast = MDBook::init()
        .chapter(Chapter::new(
            "Chapter",
            indoc! {"
                <details>
                <summary>

                ## Heading

                text

                </summary>

                more **markdown**

                </details>

                outside divs
            "},
            "chapter.md",
        ))
        .config(Config::pandoc())
        .build();
    insta::assert_snapshot!(ast, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir    
    ├─ markdown/pandoc-ir
    │ [ RawBlock (Format "html") "<details>"
    │ , Div
    │     ( "" , [] , [] )
    │     [ Plain [ Str "\n" , RawInline (Format "html") "<summary>" ]
    │     , Div
    │         ( "" , [] , [] )
    │         [ Plain [ Str "\n" ]
    │         , Header
    │             2
    │             ( "book__markdown__src__chapter.md__heading" , [] , [] )
    │             [ Str "Heading" ]
    │         , Para [ Str "text" ]
    │         ]
    │     , RawBlock (Format "html") "</summary>"
    │     , Plain [ Str "\n" ]
    │     , Para [ Str "more " , Strong [ Str "markdown" ] ]
    │     ]
    │ , RawBlock (Format "html") "</details>"
    │ , Plain [ Str "\n" ]
    │ , Para [ Str "outside divs" ]
    │ ]
    "#);

    // Make sure logic doesn't trigger on inline html since inserting divs
    // introduces newlines and breaks the original structure
    let output = MDBook::init()
        .config(Config::markdown())
        .chapter(Chapter::new(
            "Chapter",
            "he is <del>four</del><ins>five</ins> years old",
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(output, @r"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
    ├─ markdown/book.md
    │ he is `<del>`{=html}four`</del>`{=html}`<ins>`{=html}five`</ins>`{=html} years old
    ");
}

#[test]
fn implicitly_closed_tags() {
    let book = MDBook::init()
        .config(Config::latex())
        .chapter(Chapter::new(
            "",
            indoc! {r#"
                - before
                - [Box<T>](#foo)
                - after

                # Foo
            "#},
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r##"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
    ├─ latex/output.tex
    │ \begin{itemize}
    │ \tightlist
    │ \item
    │   before
    │ \item
    │   \hyperref[book__latex__src__chapter.md__foo]{Box}
    │ \item
    │   after
    │ \end{itemize}
    │ 
    │ \chapter{Foo}\label{book__latex__src__chapter.md__foo}
    ├─ latex/src/chapter.md
    │ [BulletList [[Plain [Str "before"]], [Plain [Link ("", [], []) [Str "Box", RawInline (Format "html") "<t>", RawInline (Format "html") "</t>"] ("#foo", "")]], [Plain [Str "after"]]], Header 1 ("foo", [], []) [Str "Foo"]]
    "##);
}

#[test]
fn link_to_element_by_id() {
    let book = MDBook::init()
        .config(Config::latex())
        .chapter(Chapter::new(
            "",
            indoc! {r#"
                <a id="test">some text here</a>
                <span id="test2">some text here</span>

                <div id="test3">
                some text here
                </div>

                <div id="test4">some text here</div>

                [test link](#test)
                [test2 link](#test2)
                [test3 link](#test3)
                [test4 link](#test4)
            "#},
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r##"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
    ├─ latex/output.tex
    │ \phantomsection\label{book__latex__src__chapter.md__test}{some text here}
    │ \phantomsection\label{book__latex__src__chapter.md__test2}{some text here}
    │ 
    │ \phantomsection\label{book__latex__src__chapter.md__test3}
    │ 
    │ some text here
    │ 
    │ \phantomsection\label{book__latex__src__chapter.md__test4}
    │ some text here
    │ 
    │ \hyperref[book__latex__src__chapter.md__test]{test link}
    │ \hyperref[book__latex__src__chapter.md__test2]{test2 link}
    │ \hyperref[book__latex__src__chapter.md__test3]{test3 link}
    │ \hyperref[book__latex__src__chapter.md__test4]{test4 link}
    ├─ latex/src/chapter.md
    │ [Para [Span ("test", [], []) [Str "some text here"], SoftBreak, Span ("test2", [], []) [Str "some text here"]], Div ("test3", [], []) [Plain [Str "
    │ some text here
    │ "]], Plain [Str "
    │ "], Div ("test4", [], []) [Plain [Str "some text here"]], Plain [Str "
    │ "], Para [Link ("", [], []) [Str "test link"] ("#test", ""), SoftBreak, Link ("", [], []) [Str "test2 link"] ("#test2", ""), SoftBreak, Link ("", [], []) [Str "test3 link"] ("#test3", ""), SoftBreak, Link ("", [], []) [Str "test4 link"] ("#test4", "")]]
    "##);
}

#[test]
fn rust_reference_regression_nested_elements() {
    let book = MDBook::init()
        .config(Config::latex())
        .chapter(Chapter::new(
            "",
            indoc! {r##"
                <div id="my-div">
                <a id="my-a" href="#my-div">[some text here]</a>
                </div>

                [div](#my-div)
                [a](#my-a)
            "##},
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r##"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
    ├─ latex/output.tex
    │ \phantomsection\label{book__latex__src__chapter.md__my-div}
    │ 
    │ \phantomsection\label{book__latex__src__chapter.md__my-a}\hyperref[book__latex__src__chapter.md__my-div]{{[}some text here{]}}
    │ 
    │ \hyperref[book__latex__src__chapter.md__my-div]{div}
    │ \hyperref[book__latex__src__chapter.md__my-a]{a}
    ├─ latex/src/chapter.md
    │ [Div ("my-div", [], []) [Plain [Str "
    │ ", Link ("my-a", [], [("href", "#my-div")]) [Str "[some text here]"] ("#my-div", ""), Str "
    │ "]], Plain [Str "
    │ "], Para [Link ("", [], []) [Str "div"] ("#my-div", ""), SoftBreak, Link ("", [], []) [Str "a"] ("#my-a", "")]]
    "##);
}

#[test]
fn regression_malformed_html() {
    let output = MDBook::init()
        .config(Config::latex())
        .chapter(Chapter::new(
            "",
            // These tags are mismatched (the second should be </del> to close the first)
            // but we should be able to handle this in a reasonable way
            "**<del>foo<del>**",
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
    ├─ latex/output.tex
    │ \textbf{{foo}}
    ├─ latex/src/chapter.md
    │ [Para [Strong [RawInline (Format "html") "<del>", Span ("", [], []) [Str "foo", RawInline (Format "html") "<del>", RawInline (Format "html") "</del>"], RawInline (Format "html") "</del>"]]]
    "#);
}

#[test]
fn regression_inline_html_newlines() {
    let output = MDBook::init()
        .config(Config::latex())
        .chapter(Chapter::new(
            "",
            // This should stay on a single line; the presence of inline HTML should not result in a line break
            "- <kbd>Arrow-Left</kbd>: Navigate to the previous page.",
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
    ├─ latex/output.tex
    │ \begin{itemize}
    │ \item
    │   Arrow-Left: Navigate to the previous page.
    │ \end{itemize}
    ├─ latex/src/chapter.md
    │ [BulletList [[RawBlock (Format "html") "<kbd>", Plain [Str "Arrow-Left", RawInline (Format "html") "</kbd>", Str ": Navigate to the previous page."]]]]
    "#);
}

#[test]
fn attach_id_to_div_of_stripped_html_elements() {
    let book = MDBook::init()
        .config(Config::latex())
        .chapter(Chapter::new(
            "",
            r##"<dt id="foo=bar"><a href="#foo=bar"></a>something here</dt>"##,
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r##"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
    ├─ latex/output.tex
    │ \phantomsection\label{book__latex__src__chapter.md__foo=bar}
    │ \hyperref[book__latex__src__chapter.md__foo=bar]{}something here
    ├─ latex/src/chapter.md
    │ [RawBlock (Format "html") "<dt id=\"foo=bar\">", Div ("foo=bar", [], []) [Plain [Link ("", [], [("href", "#foo=bar")]) [] ("#foo=bar", ""), Str "something here"]], RawBlock (Format "html") "</dt>"]
    "##);
}

#[test]
fn noscript_element() {
    let output = MDBook::init()
        .config(Config::pandoc())
        .chapter(Chapter::new(
            "",
            "<noscript>\n\n## No scripting enabled\n\n</noscript>",
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir    
    ├─ markdown/pandoc-ir
    │ [ RawBlock (Format "html") "<noscript>"
    │ , Plain [ Str "\n" ]
    │ , Header
    │     2
    │     ( "book__markdown__src__chapter.md__no-scripting-enabled"
    │     , []
    │     , []
    │     )
    │     [ Str "No scripting enabled" ]
    │ , RawBlock (Format "html") "</noscript>"
    │ ]
    "#);
}
