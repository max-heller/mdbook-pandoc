use super::{Chapter, Config, MDBook};

#[test]
fn preserve_escapes() {
    let output = MDBook::init()
        .config(Config::pandoc())
        .chapter(Chapter::new("", "[Prefix @fig:1] [-@fig:1]", "chapter.md"))
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir
    ├─ markdown/pandoc-ir
    │ [ Div
    │     ( "book__markdown__src__chapter.md" , [] , [] )
    │     [ Para
    │         [ Str "["
    │         , Str "Prefix @fig:1"
    │         , Str "]"
    │         , Str " "
    │         , Str "["
    │         , Str "-@fig:1"
    │         , Str "]"
    │         ]
    │     ]
    │ , Div ( "book__markdown__dummy" , [] , [] ) []
    │ ]
    "#);
}
