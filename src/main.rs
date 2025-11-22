use std::{
    env,
    io::{self, Write},
    process,
};

use anyhow::Context;
use mdbook_renderer::Renderer;

fn main() {
    if let Err(err) = try_main() {
        eprintln!("{err:#}");
        process::exit(1);
    }
}

fn try_main() -> anyhow::Result<()> {
    init_logger();

    let ctx = mdbook_renderer::RenderContext::from_json(io::stdin().lock())
        .context("unable to parse mdBook context")?;
    mdbook_pandoc::Renderer::new().render(&ctx)
}

// Adapted from mdbook's main.rs for consistency in log format
fn init_logger() {
    let mut builder = env_logger::Builder::new();

    builder.format(|formatter, record| {
        let style = formatter.default_level_style(record.level());
        writeln!(
            formatter,
            "{} [{style}{}{style:#}] ({}): {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            record.level(),
            record.target(),
            record.args()
        )
    });

    if let Ok(var) = env::var("RUST_LOG") {
        builder.parse_filters(&var);
    } else {
        // if no RUST_LOG provided, default to logging at the Info level
        builder.filter(None, log::LevelFilter::Info);
        // Filter extraneous html5ever not-implemented messages
        builder.filter(Some("html5ever"), log::LevelFilter::Error);
    }

    builder.init();
}
