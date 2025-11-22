use std::str::FromStr;

use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn css() {
    let cfg = indoc! {r#"
        [output.html]
        additional-css = ["custom.css"]
    "#};
    let book = MDBook::init()
        .mdbook_config(mdbook_core::config::Config::from_str(cfg).unwrap())
        .config(Config::latex())
        .file_in_src("img/image.png", "")
        .file_in_root(
            "custom.css",
            indoc! {"
                .ferris-explain {
                  width: 100px;
                  height: 50;
                }

                .foo-hidden {
                    display: none;
                }
            "},
        )
        .chapter(Chapter::new(
            "",
            indoc! {r#"
                <img class="ferris-explain" src="img/image.png" alt="alt text" title = "a title">
                <div class="foo-hidden">should be hidden</div>
                <div style="display:none">should also be hidden</div>
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
    │ \hypertarget{book__latex__src__chapter.md}{}
    │ \includegraphics[width=1.04167in,height=0.52083in]{book/latex/src/img/image.png}
    │ 
    │ \hypertarget{book__latex__dummy}{}
    ├─ latex/src/chapter.md
    │ [Plain [Image ("", ["ferris-explain"], [("height", "50"), ("width", "100px")]) [Str "alt text"] ("book/latex/src/img/image.png", "a title")]]
    ├─ latex/src/img/image.png
    "#);
}
