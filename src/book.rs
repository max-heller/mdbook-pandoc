use std::{fs, path::PathBuf};

pub struct Book<'book> {
    pub book: &'book mdbook::book::Book,
    pub root: PathBuf,
    pub source_dir: PathBuf,
    pub destination: PathBuf,
}

impl<'book> Book<'book> {
    pub fn new(ctx: &'book mdbook::renderer::RenderContext) -> anyhow::Result<Self> {
        let root = ctx.root.canonicalize()?;
        let source_dir = ctx.source_dir().canonicalize()?;

        fs::create_dir_all(&ctx.destination)?;
        let destination = ctx.destination.canonicalize()?;

        Ok(Self {
            book: &ctx.book,
            root,
            source_dir,
            destination,
        })
    }
}
