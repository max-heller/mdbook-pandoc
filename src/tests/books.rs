use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;

use super::{Config, MDBook};

static BOOKS: Lazy<PathBuf> = Lazy::new(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("books"));

#[test]
#[ignore]
fn mdbook_guide() {
    let logs = MDBook::load(BOOKS.join("mdBook/guide"))
        .config(Config::pdf())
        .site_url("https://rust-lang.github.io/mdBook/")
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
        .config(Config::pdf())
        .site_url("https://doc.rust-lang.org/cargo/")
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rust_book() {
    let logs = MDBook::load(BOOKS.join("rust-book"))
        .config(Config::pdf())
        .site_url("https://doc.rust-lang.org/book/")
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn nomicon() {
    let logs = MDBook::load(BOOKS.join("nomicon"))
        .config(Config::pdf())
        .site_url("https://doc.rust-lang.org/nomicon/")
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rust_by_example() {
    let logs = MDBook::load(BOOKS.join("rust-by-example"))
        .config(Config::pdf())
        .site_url("https://doc.rust-lang.org/rust-by-example/")
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rust_edition_guide() {
    let logs = MDBook::load(BOOKS.join("rust-edition-guide"))
        .config(Config::pdf())
        .site_url("https://doc.rust-lang.org/edition-guide/")
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rust_embedded() {
    let logs = MDBook::load(BOOKS.join("rust-embedded"))
        .config(Config::pdf())
        .site_url("https://docs.rust-embedded.org/book/")
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rust_reference() {
    let logs = MDBook::load(BOOKS.join("rust-reference"))
        .config(Config::pdf())
        .site_url("https://doc.rust-lang.org/reference/")
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}

#[test]
#[ignore]
fn rustc_dev_guide() {
    let logs = MDBook::load(BOOKS.join("rustc-dev-guide"))
        .config(Config::pdf())
        .site_url("https://rustc-dev-guide.rust-lang.org/")
        .build()
        .logs;
    insta::assert_snapshot!(logs);
}
