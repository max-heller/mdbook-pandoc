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
                config.markdown.extensions.definition_lists = true;
                config
            })
            .build();
        similar::TextDiff::from_lines(&without.to_string(), &with.to_string())
            .unified_diff()
            .to_string()
    };
    let source = indoc! {"
        title 1
          : definition 1

        title 2
          : definition 2 a
          : definition 2 b
    "};
    let latex = diff(source, Config::latex());
    insta::assert_snapshot!(latex, @r#"
    @@ -3,11 +3,17 @@
     │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
     │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
     ├─ latex/output.tex
    -│ \leavevmode\vadjust pre{\hypertarget{book__latex__src__chapter.md}{}}%
    -│ title 1 : definition 1
    +│ \hypertarget{book__latex__src__chapter.md}{}
    +│ \begin{description}
    +│ \tightlist
    +│ \item[title 1]
    +│ definition 1
    +│ \item[title 2]
    +│ definition 2 a
     │ 
    -│ title 2 : definition 2 a : definition 2 b
    +│ definition 2 b
    +│ \end{description}
     │ 
     │ \hypertarget{book__latex__dummy}{}
     ├─ latex/src/chapter.md
    -│ [Para [Str "title 1", SoftBreak, Str ": definition 1"], Para [Str "title 2", SoftBreak, Str ": definition 2 a", SoftBreak, Str ": definition 2 b"]]
    +│ [DefinitionList [([Str "title 1"], [[Plain [Str "definition 1"]]]), ([Str "title 2"], [[Plain [Str "definition 2 a"]], [Plain [Str "definition 2 b"]]])]]
    "#);
}

#[test]
fn dt_attributes() {
    let source = indoc! {r#"
        <dl>
        <dt id="term1">term 1</dt>
        <dd>definition 1</dd>
        </dl>

        [link to term 1](#term1)
    "#};
    let latex = MDBook::init()
        .chapter(Chapter::new("", source, "chapter.md"))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(latex, @r##"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \hypertarget{book__latex__src__chapter.md}{}
    │ \begin{description}
    │ \tightlist
    │ \item[\protect\hypertarget{book__latex__src__chapter.md__term1}{}{term 1}]
    │ definition 1
    │ \end{description}
    │ 
    │ \protect\hyperlink{book__latex__src__chapter.md__term1}{link to term 1}
    │ 
    │ \hypertarget{book__latex__dummy}{}
    ├─ latex/src/chapter.md
    │ [DefinitionList [([Span ("term1", [], []) [Str "term 1"]], [[Plain [Str "definition 1"]]])], Para [Link ("", [], []) [Str "link to term 1"] ("#term1", "")]]
    "##);
}
