use std::str::FromStr;

use indoc::indoc;

use super::MDBook;

#[test]
fn disabled() {
    let cfg = indoc! {r#"
        [output.pandoc]
        disabled = true
    "#};
    let output = MDBook::init()
        .mdbook_config(mdbook_core::config::Config::from_str(cfg).unwrap())
        .build();
    insta::assert_snapshot!(output, @r"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc: Skipping rendering since `disabled` is set
    ")
}

#[test]
fn pandoc_working_dir_is_root() {
    let cfg = indoc! {r#"
        [output.pandoc.profile.foo]
        output-file = "foo.md"
        include-in-header = ["file-in-root"]
    "#};
    let book = MDBook::init()
        .mdbook_config(cfg.parse().unwrap())
        .file_in_root("file-in-root", "some text")
        .build();
    insta::assert_snapshot!(book, @r"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/foo/foo.md
    ├─ foo/foo.md
    │ some text
    ");
}

#[test]
fn raw_opts() {
    let cfg = indoc! {r#"
        [book]
        title = "Example book"
        authors = ["John Doe", "Jane Doe"]
        description = "The example book covers examples."
        language = "en"
        text-direction = "ltr"

        [output.pandoc.profile.test]
        output-file = "/dev/null"
        to = "markdown"
        verbosity = "INFO"
        fail-if-warnings = false

        resource-path = [
            "really-long-path",
            "really-long-path2",
        ]

        [output.pandoc.profile.test.variables]
        header-includes = [
            "text1",
            "text2",
        ]
        indent = true
        colorlinks = false
    "#};
    let output = MDBook::options()
        .max_log_level(tracing::Level::TRACE)
        .init()
        .mdbook_config(mdbook_core::config::Config::from_str(cfg).unwrap())
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │ DEBUG mdbook_driver::mdbook: Running the index preprocessor.
    │ DEBUG mdbook_driver::mdbook: Running the links preprocessor.
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │ TRACE mdbook_pandoc::pandoc::renderer: Running pandoc with profile: Profile {
    │     columns: 72,
    │     file_scope: true,
    │     number_sections: true,
    │     output_file: "/dev/null",
    │     pdf_engine: None,
    │     standalone: true,
    │     to: Some(
    │         "markdown",
    │     ),
    │     table_of_contents: true,
    │     variables: {
    │         "colorlinks": Boolean(
    │             false,
    │         ),
    │         "dir": String(
    │             "ltr",
    │         ),
    │         "header-includes": Array(
    │             [
    │                 String(
    │                     "text1",
    │                 ),
    │                 String(
    │                     "text2",
    │                 ),
    │             ],
    │         ),
    │         "indent": Boolean(
    │             true,
    │         ),
    │         "lang": String(
    │             "en",
    │         ),
    │     },
    │     metadata: {
    │         "author": Array(
    │             [
    │                 String(
    │                     "John Doe",
    │                 ),
    │                 String(
    │                     "Jane Doe",
    │                 ),
    │             ],
    │         ),
    │         "description": String(
    │             "The example book covers examples.",
    │         ),
    │         "title": String(
    │             "Example book",
    │         ),
    │     },
    │     rest: {
    │         "fail-if-warnings": Boolean(
    │             false,
    │         ),
    │         "resource-path": Array(
    │             [
    │                 String(
    │                     "really-long-path",
    │                 ),
    │                 String(
    │                     "really-long-path2",
    │                 ),
    │             ],
    │         ),
    │         "verbosity": String(
    │             "INFO",
    │         ),
    │     },
    │ }
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to /dev/null
    "#)
}
