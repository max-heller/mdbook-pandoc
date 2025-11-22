use std::{fs, path::PathBuf};

use normpath::PathExt;

pub struct Book<'book> {
    pub book: &'book mdbook_core::book::Book,
    pub root: PathBuf,
    pub source_dir: PathBuf,
    pub destination: PathBuf,
}

impl<'book> Book<'book> {
    pub fn new(ctx: &'book mdbook_renderer::RenderContext) -> anyhow::Result<Self> {
        let root = ctx.root.normalize()?.into_path_buf();
        let source_dir = ctx.source_dir().normalize()?.into_path_buf();

        fs::create_dir_all(&ctx.destination)?;
        let destination = ctx.destination.normalize()?.into_path_buf();

        Ok(Self {
            book: &ctx.book,
            root,
            source_dir,
            destination,
        })
    }
}
