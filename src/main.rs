use std::{io, process};

use anyhow::Context;
use mdbook::Renderer;

fn main() {
    if let Err(err) = try_main() {
        eprintln!("{err:#}");
        process::exit(1);
    }
}

fn try_main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let ctx = mdbook::renderer::RenderContext::from_json(io::stdin().lock())
        .context("unable to parse mdBook context")?;
    mdbook_pandoc::Renderer::new().render(&ctx)
}
