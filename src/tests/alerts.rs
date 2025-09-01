use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn alerts() {
    let diff = |source: &str, mut config: Config| {
        let chapter = Chapter::new("", source, "chapter.md");
        let without = MDBook::init()
            .chapter(chapter.clone())
            .config(config.clone())
            .build();
        let with = MDBook::init()
            .chapter(chapter)
            .config({
                config.markdown.extensions.gfm = true;
                config
            })
            .build();
        similar::TextDiff::from_lines(&without.to_string(), &with.to_string())
            .unified_diff()
            .to_string()
    };
    let alert = indoc! {"
        > [!NOTE]  
        > Highlights information that users should take into account, even when skimming.
    "};
    let latex = diff(alert, Config::latex());
    insta::assert_snapshot!(latex, @r#"
    @@ -3,9 +3,8 @@
     │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
     │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
     ├─ latex/output.tex
    -│ \begin{quote}
    -│ {[}!NOTE{]}\\
    +│ Note
    +│ 
     │ Highlights information that users should take into account, even when skimming.
    -│ \end{quote}
     ├─ latex/src/chapter.md
    -│ [BlockQuote [Para [Str "[", Str "!NOTE", Str "]", LineBreak, Str "Highlights information that users should take into account, even when skimming."]]]
    +│ [Div ("", ["note"], []) [Div ("", ["title"], []) [Para [Str "Note"]], Para [Str "Highlights information that users should take into account, even when skimming."]]]
    "#);
    let markdown = diff(alert, Config::markdown());
    insta::assert_snapshot!(markdown, @r"
    @@ -3,5 +3,5 @@
     │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
     │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md
     ├─ markdown/book.md
    -│ > \[!NOTE\]  
    +│ > [!NOTE]
     │ > Highlights information that users should take into account, even when skimming.
    ");
}
