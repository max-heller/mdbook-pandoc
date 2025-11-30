use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn basic() {
    let diff = |source: &str, mut config: Config| {
        let chapter = Chapter::new("", source, "chapter.md");
        let without = MDBook::init()
            .chapter(chapter.clone())
            .config(config.clone())
            .build();
        let with = MDBook::init()
            .chapter(chapter)
            .config({
                config.common.markdown.extensions.superscript = true;
                config.common.markdown.extensions.subscript = true;
                config
            })
            .build();
        similar::TextDiff::from_lines(&without.to_string(), &with.to_string())
            .unified_diff()
            .to_string()
    };
    let source = indoc! {r"
        ^This is super^ ~This is sub~
    "};
    let latex = diff(source, Config::latex());
    insta::assert_snapshot!(latex, @r#"
    @@ -3,6 +3,6 @@
     │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
     │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
     ├─ latex/output.tex
    -│ \^{}This is super\^{} \st{This is sub}
    +│ \textsuperscript{This is super} \textsubscript{This is sub}
     ├─ latex/src/chapter.md
    -│ [Para [Str "^This is super^ ", Strikeout [Str "This is sub"]]]
    +│ [Para [Superscript [Str "This is super"], Str " ", Subscript [Str "This is sub"]]]
    "#);
}
