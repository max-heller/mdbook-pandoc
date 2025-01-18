use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;

use super::{Config, MDBook};

static BOOKS: Lazy<PathBuf> = Lazy::new(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("books"));

#[test]
#[ignore]
fn mdbook_guide() {
    let logs = MDBook::load(BOOKS.join("mdBook/guide"))
        .config(Config {
            hosted_html: Some("https://rust-lang.github.io/mdBook/".into()),
            ..Config::pdf()
        })
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn cargo_book() {
    let logs = MDBook::options()
        .max_log_level(tracing::Level::DEBUG)
        .load(BOOKS.join("cargo/src/doc"))
        .config(Config {
            hosted_html: Some("https://doc.rust-lang.org/cargo/".into()),
            ..Config::pdf()
        })
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rust_book() {
    let logs = MDBook::load(BOOKS.join("rust-book"))
        .config(Config {
            hosted_html: Some("https://doc.rust-lang.org/book/".into()),
            ..Config::pdf()
        })
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn nomicon() {
    let logs = MDBook::load(BOOKS.join("nomicon"))
        .config(Config {
            hosted_html: Some("https://doc.rust-lang.org/nomicon/".into()),
            ..Config::pdf()
        })
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rust_by_example() {
    let logs = MDBook::load(BOOKS.join("rust-by-example"))
        .config(Config {
            hosted_html: Some("https://doc.rust-lang.org/rust-by-example/".into()),
            ..Config::pdf()
        })
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rust_edition_guide() {
    let logs = MDBook::load(BOOKS.join("rust-edition-guide"))
        .config(Config {
            hosted_html: Some("https://doc.rust-lang.org/edition-guide/".into()),
            ..Config::pdf()
        })
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rust_embedded() {
    let logs = MDBook::load(BOOKS.join("rust-embedded"))
        .config(Config {
            hosted_html: Some("https://docs.rust-embedded.org/book/".into()),
            ..Config::pdf()
        })
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rust_reference() {
    let logs = MDBook::load(BOOKS.join("rust-reference"))
        .config(Config {
            hosted_html: Some("https://doc.rust-lang.org/reference/".into()),
            ..Config::pdf()
        })
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rustc_dev_guide() {
    let logs = MDBook::load(BOOKS.join("rustc-dev-guide"))
        .config(Config {
            hosted_html: Some("https://rustc-dev-guide.rust-lang.org/".into()),
            ..Config::pdf()
        })
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}
