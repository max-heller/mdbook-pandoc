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

                before <i class="fas fa-print"></i> after
            "#},
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \leavevmode\vadjust pre{\hypertarget{book__latex__src__chapter.md}{}}%
    │ \faicon{print} \faicon{print}
    │ 
    │ before  after
    │ 
    │ \hypertarget{book__latex__dummy}{}
    ├─ latex/src/chapter.md
    │ [Para [RawInline (Format "latex") "\\faicon{print}", SoftBreak, RawInline (Format "latex") "\\faicon{print}"], Para [Str "before ", Image ("", [], [("height", "1em")]) [] ("data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCA1MTIgNTEyIj48IS0tISBGb250IEF3ZXNvbWUgRnJlZSA2LjIuMCBieSBAZm9udGF3ZXNvbWUgLSBodHRwczovL2ZvbnRhd2Vzb21lLmNvbSBMaWNlbnNlIC0gaHR0cHM6Ly9mb250YXdlc29tZS5jb20vbGljZW5zZS9mcmVlIChJY29uczogQ0MgQlkgNC4wLCBGb250czogU0lMIE9GTCAxLjEsIENvZGU6IE1JVCBMaWNlbnNlKSBDb3B5cmlnaHQgMjAyMiBGb250aWNvbnMsIEluYy4gLS0+PHBhdGggZD0iTTEyOCAwQzkyLjcgMCA2NCAyOC43IDY0IDY0djk2aDY0VjY0SDM1NC43TDM4NCA5My4zVjE2MGg2NFY5My4zYzAtMTctNi43LTMzLjMtMTguNy00NS4zTDQwMCAxOC43QzM4OCA2LjcgMzcxLjcgMCAzNTQuNyAwSDEyOHpNMzg0IDM1MnYzMiA2NEgxMjhWMzg0IDM2OCAzNTJIMzg0em02NCAzMmgzMmMxNy43IDAgMzItMTQuMyAzMi0zMlYyNTZjMC0zNS4zLTI4LjctNjQtNjQtNjRINjRjLTM1LjMgMC02NCAyOC43LTY0IDY0djk2YzAgMTcuNyAxNC4zIDMyIDMyIDMySDY0djY0YzAgMzUuMyAyOC43IDY0IDY0IDY0SDM4NGMzNS4zIDAgNjQtMjguNyA2NC02NFYzODR6bS0xNi04OGMtMTMuMyAwLTI0LTEwLjctMjQtMjRzMTAuNy0yNCAyNC0yNHMyNCAxMC43IDI0IDI0cy0xMC43IDI0LTI0IDI0eiIvPjwvc3ZnPg==", ""), Str " after"]]
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
