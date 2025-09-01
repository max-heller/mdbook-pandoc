use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn font_awesome_icons() {
    let book = MDBook::init()
        .config(Config::latex())
        .chapter(Chapter::new(
            "",
            indoc! {r#"
                <i class="fa fa-print"></i>
                <i class = "fa fa-print"/></i>
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
    │ \faicon{print} \faicon{print}
    ├─ latex/src/chapter.md
    │ [Para [RawInline (Format "latex") "\\faicon{print}", SoftBreak, RawInline (Format "latex") "\\faicon{print}"]]
    "#);

    let book = MDBook::init()
        .chapter(Chapter::new(
            "",
            r#"<i class="fa fa-print"></i>"#,
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md
    ├─ markdown/book.md
    │ `<i class="fa fa-print">`{=html}`</i>`{=html}
    "#);
}

#[test]
#[ignore]
fn right_to_left_fonts_lualatex() {
    let cfg = indoc! {r#"
        [book]
        language = "fa"

        [output.pandoc.profile.pdf]
        output-file = "book.pdf"
        pdf-engine = "lualatex"

        [output.pandoc.profile.pdf.variables]
        mainfont = "Noto Naskh Arabic"
        mainfontfallback = [
          "NotoSerif:",
        ]
    "#};
    let output = MDBook::init()
        .mdbook_config(cfg.parse().unwrap())
        .chapter(Chapter::new("", "<span dir=ltr>C++</span>", "chapter.md"))
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/pdf/book.pdf
    ├─ pdf/book.pdf
    │ <INVALID UTF8>
    ├─ pdf/src/chapter.md
    │ [Para [Span ("", [], [("dir", "ltr")]) [Str "C++"]]]
    "#);
}
