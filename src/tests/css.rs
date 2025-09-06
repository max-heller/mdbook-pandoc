use std::str::FromStr;

use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn css() {
    let cfg = indoc! {r#"
        [output.html]
        additional-css = ["ferris.css"]
    "#};
    let book = MDBook::init()
        .mdbook_config(mdbook::Config::from_str(cfg).unwrap())
        .config(Config::latex())
        .file_in_src("img/image.png", "")
        .file_in_root(
            "ferris.css",
            indoc! {"
                .ferris-explain {
                  width: 100px;
                  height: 50;
                }
            "},
        )
        .chapter(Chapter::new(
            "",
            r#"<img class="ferris-explain" src="img/image.png" alt="alt text" title = "a title">"#,
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \includegraphics[width=1.04167in,height=0.52083in,alt={alt text}]{book/latex/src/img/image.png}
    ├─ latex/src/chapter.md
    │ [Plain [Image ("", ["ferris-explain"], [("height", "50"), ("width", "100px")]) [Str "alt text"] ("book/latex/src/img/image.png", "a title")]]
    ├─ latex/src/img/image.png
    "#);
}
