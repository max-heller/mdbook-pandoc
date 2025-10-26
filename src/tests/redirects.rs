use std::str::FromStr;

use indoc::indoc;

use super::{Chapter, MDBook};

#[test]
fn redirects() {
    let cfg = indoc! {r#"
        [output.pandoc.profile.test]
        output-file = "/dev/null"
        to = "markdown"

        [output.html.redirect]
        "/appendices/bibliography.html" = "https://rustc-dev-guide.rust-lang.org/appendix/bibliography.html"
        "/foo/bar.html" = "../new bar.html"
        "/new bar.html" = "new new-bar.html"
    "#};
    let output = MDBook::options()
        .max_log_level(tracing::Level::DEBUG)
        .init()
        .mdbook_config(mdbook::Config::from_str(cfg).unwrap())
        .chapter(Chapter::new(
            "",
            "[bar](foo/bar.md)\n[bib](appendices/bibliography.html)",
            "index.md",
        ))
        .chapter(Chapter::new("", "# New New Bar", "new new-bar.md"))
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │ DEBUG mdbook::book: Running the index preprocessor.
    │ DEBUG mdbook::book: Running the links preprocessor.
    │  INFO mdbook::book: Running the pandoc backend
    │ DEBUG mdbook_pandoc: Processing redirects in [output.html.redirect]
    │ DEBUG mdbook_pandoc::preprocess: Processing redirect: /appendices/bibliography.html => https://rustc-dev-guide.rust-lang.org/appendix/bibliography.html
    │ DEBUG mdbook_pandoc::preprocess: Processing redirect: /foo/bar.html => ../new bar.html
    │ DEBUG mdbook_pandoc::preprocess: Processing redirect: /new bar.html => new new-bar.html
    │ DEBUG mdbook_pandoc::preprocess: Registered redirect: book/test/src/appendices/bibliography.html => https://rustc-dev-guide.rust-lang.org/appendix/bibliography.html
    │ DEBUG mdbook_pandoc::preprocess: Registered redirect: book/test/src/foo/bar.html => book/test/src/new bar.html
    │ DEBUG mdbook_pandoc::preprocess: Registered redirect: book/test/src/new bar.html => book/test/src/new new-bar.md#new-new-bar
    │ DEBUG mdbook_pandoc::preprocess: Preprocessing ''
    │ DEBUG mdbook_pandoc::preprocess: Preprocessing ''
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to /dev/null
    ├─ test/src/appendices/bibliography.html
    ├─ test/src/foo/bar.html
    ├─ test/src/index.md
    │ [Para [Link ("", [], []) [Str "bar"] ("book/test/src/new%20new-bar.md#new-new-bar", ""), SoftBreak, Link ("", [], []) [Str "bib"] ("https://rustc-dev-guide.rust-lang.org/appendix/bibliography.html", "")]]
    ├─ test/src/new bar.html
    ├─ test/src/new new-bar.md
    │ [Header 1 ("new-new-bar", [], []) [Str "New New Bar"]]
    "#)
}
