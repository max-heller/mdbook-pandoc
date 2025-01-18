use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn alerts() {
    let alert = indoc! {"
        > [!NOTE]  
        > Highlights information that users should take into account, even when skimming.
    "};
    let latex = MDBook::init()
        .chapter(Chapter::new("", alert, "chapter.md"))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(latex, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
    ├─ latex/output.tex
    │ Note
    │ 
    │ Highlights information that users should take into account, even when skimming.
    ├─ latex/src/chapter.md
    │ [Div ("", ["note"], []) [Div ("", ["title"], []) [Para [Str "Note"]], Para [Str "Highlights information that users should take into account, even when skimming."]]]
    "#);
    let markdown = MDBook::init()
        .chapter(Chapter::new("", alert, "chapter.md"))
        .config(Config::markdown())
        .build();
    insta::assert_snapshot!(markdown, @r"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
    ├─ markdown/book.md
    │ > [!NOTE]
    │ > Highlights information that users should take into account, even when skimming.
    ");
}
