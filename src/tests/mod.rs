use std::{
    env,
    fmt::{self, Write},
    fs::{self, File},
    io::{self, Read, Seek},
    path::{Path, PathBuf},
};

use anyhow::Context;
use mdbook::{book::BookItem, Renderer as _};
use normpath::PathExt;
use regex::Regex;
use tempfile::{tempfile, TempDir};
use toml::toml;
use tracing_subscriber::layer::SubscriberExt;

use crate::{Config, Renderer};

pub struct MDBook {
    book: mdbook_driver::MDBook,
    _root: Option<TempDir>,
    _logger: tracing::subscriber::DefaultGuard,
    logfile: File,
}

#[derive(Clone, Copy)]
pub struct Options {
    max_log_level: tracing::level_filters::LevelFilter,
}

#[derive(Clone)]
pub struct Chapter {
    chapter: mdbook::book::Chapter,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_log_level: tracing::Level::INFO.into(),
        }
    }
}

impl Options {
    pub fn init(self) -> MDBook {
        // Initialize a book directory
        let root = TempDir::new().unwrap();
        let mut book = mdbook_driver::init::BookBuilder::new(root.path())
            .build()
            .unwrap();

        // Clear out the stub files
        let src = book.source_dir();
        fs::remove_file(src.join("SUMMARY.md")).unwrap();
        for item in book.book.items.drain(..) {
            match item {
                BookItem::Chapter(chap) => {
                    if let Some(path) = chap.source_path {
                        fs::remove_file(src.join(path)).unwrap();
                    }
                }
                BookItem::Separator | BookItem::PartTitle(_) => {}
            }
        }

        MDBook::new(book, Some(root), self)
    }

    pub fn load(self, path: impl Into<PathBuf>) -> MDBook {
        MDBook::new(mdbook_driver::MDBook::load(path).unwrap(), None, self)
    }

    pub fn max_log_level(
        mut self,
        max_level: impl Into<tracing::level_filters::LevelFilter>,
    ) -> Self {
        self.max_log_level = max_level.into();
        self
    }
}

impl MDBook {
    pub fn init() -> Self {
        Options::default().init()
    }

    pub fn load(path: impl Into<PathBuf>) -> Self {
        Options::default().load(path)
    }

    pub fn options() -> Options {
        Options::default()
    }

    fn new(mut book: mdbook_driver::MDBook, tempdir: Option<TempDir>, options: Options) -> Self {
        // Initialize logger to captures `log` output and redirect it to a tempfile
        let logfile = tempfile().unwrap();
        let _logger = tracing::subscriber::set_default(
            tracing_subscriber::registry()
                .with(
                    tracing_subscriber::fmt::layer()
                        .compact()
                        .without_time()
                        .with_ansi(false)
                        .with_writer({
                            let logfile = logfile.try_clone().unwrap();
                            move || logfile.try_clone().unwrap()
                        }),
                )
                .with(
                    tracing_subscriber::filter::Targets::new()
                        .with_default(options.max_log_level)
                        .with_target("html5ever", tracing::Level::INFO),
                ),
        );

        // Configure renderer to only preprocess
        book.config
            .set(Renderer::CONFIG_KEY, Config::markdown())
            .unwrap();

        Self {
            book,
            _root: tempdir,
            _logger,
            logfile,
        }
    }

    pub fn mdbook_config(mut self, config: mdbook::config::Config) -> Self {
        self.book.config = config;
        self
    }

    pub fn config(mut self, config: Config) -> Self {
        self.book.config.set(Renderer::CONFIG_KEY, config).unwrap();
        self
    }

    pub fn site_url(mut self, url: &str) -> Self {
        self.book.config.set("output.html.site-url", url).unwrap();
        self
    }

    pub fn chapter(mut self, Chapter { mut chapter }: Chapter) -> Self {
        use mdbook::book::SectionNumber;
        let number = (self.book.book.chapters())
            .filter(|chapter| chapter.number.is_some())
            .count();
        chapter.number = Some(SectionNumber::new(vec![number as u32]));
        let mut chapters = vec![&mut chapter];
        while let Some(chapter) = chapters.pop() {
            let number = &chapter.number;
            for (idx, chapter) in chapter
                .sub_items
                .iter_mut()
                .filter_map(|item| match item {
                    BookItem::Chapter(chapter) => Some(chapter),
                    _ => None,
                })
                .enumerate()
            {
                if let Some(number) = number {
                    let mut number = number.clone();
                    number.push(idx as u32 + 1);
                    chapter.number = Some(number);
                }
                chapters.push(chapter);
            }
            if let Some(path) = &chapter.path {
                let path = self.book.source_dir().join(path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                File::create(path).unwrap();
            }
        }
        self.book.book.push_item(BookItem::Chapter(chapter));
        self
    }

    pub fn part(mut self, name: impl Into<String>) -> Self {
        self.book.book.push_item(BookItem::PartTitle(name.into()));
        self
    }

    pub fn file_in_src(self, path: impl AsRef<Path>, contents: &str) -> Self {
        let path = self.book.source_dir().join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
        self
    }

    pub fn file_in_root(self, path: impl AsRef<Path>, contents: &str) -> Self {
        let path = self.book.root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
        self
    }

    pub fn build(mut self) -> BuildOutput {
        let mut renderer = Renderer::new();
        renderer.logfile = Some(self.logfile.try_clone().unwrap());
        env::set_current_dir(&self.book.root).unwrap();
        let res = self.book.execute_build_process(&renderer);
        self.logfile.seek(io::SeekFrom::Start(0)).unwrap();
        let mut logs = String::new();
        self.logfile.read_to_string(&mut logs).unwrap();
        if let Err(err) = res {
            writeln!(&mut logs, "{err:#}").unwrap()
        }

        let root = self.book.root.normalize().unwrap().into_path_buf();
        let re = Regex::new(&format!(
            r"(?P<root>{})|(?P<line>line\s+\d+)|(?P<page>page\s+\d+)",
            root.display()
        ))
        .unwrap();
        let logs = re.replace_all(&logs, |caps: &regex::Captures| {
            (caps.name("root").map(|_| "$ROOT"))
                .or_else(|| caps.name("line").map(|_| "$LINE"))
                .or_else(|| caps.name("page").map(|_| "$PAGE"))
                .unwrap()
        });
        BuildOutput {
            logs: logs.into(),
            dir: self.book.build_dir_for(renderer.name()),
            _root: self._root,
        }
    }
}

impl Chapter {
    pub fn new(
        name: impl Into<String>,
        content: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Self {
        let path = path.into();
        Self {
            chapter: mdbook::book::Chapter {
                name: name.into(),
                content: content.into(),
                path: Some(path.clone()),
                source_path: Some(path),
                ..Default::default()
            },
        }
    }

    /// Adds `chapter` as a child of `self` in the hierarchy.
    pub fn child(mut self, mut chapter: Self) -> Self {
        chapter.chapter.parent_names.push(self.chapter.name.clone());
        self.chapter
            .sub_items
            .push(BookItem::Chapter(chapter.chapter));
        self
    }
}

fn visualize_directory(dir: impl AsRef<Path>, mut writer: impl fmt::Write) -> anyhow::Result<()> {
    fn visualize_directory(
        root: &Path,
        dir: &Path,
        writer: &mut dyn fmt::Write,
    ) -> anyhow::Result<()> {
        let mut entries = fs::read_dir(dir)
            .with_context(|| format!("Unable to read directory: {}", dir.display()))?
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            match entry.file_type()? {
                ty if ty.is_dir() => visualize_directory(root, path.as_ref(), writer)?,
                ty if ty.is_file() => {
                    writeln!(writer, "├─ {}", path.strip_prefix(root).unwrap().display())?;
                    match fs::read_to_string(path) {
                        Ok(contents) => {
                            for line in contents.lines() {
                                writeln!(writer, "│ {line}")?;
                            }
                        }
                        Err(err) if err.kind() == io::ErrorKind::InvalidData => {
                            writeln!(writer, "│ <INVALID UTF8>")?;
                        }
                        Err(err) => return Err(err.into()),
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    visualize_directory(dir.as_ref(), dir.as_ref(), &mut writer)
}

pub struct BuildOutput {
    logs: String,
    dir: PathBuf,
    _root: Option<TempDir>,
}

impl fmt::Display for BuildOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.logs.is_empty() {
            writeln!(f, "├─ log output")?;
            for line in self.logs.lines() {
                writeln!(f, "│ {line}")?;
            }
        }
        visualize_directory(&self.dir, f).expect("`visualize_directory` should succeed");
        Ok(())
    }
}

impl Config {
    fn latex() -> Self {
        toml! {
            [profile.latex]
            output-file = "output.tex"
            standalone = false

            [profile.latex.variables]
            documentclass = "report"
        }
        .try_into()
        .unwrap()
    }

    fn pdf() -> Self {
        toml! {
            keep-preprocessed = false

            [profile.pdf]
            output-file = "book.pdf"
            to = "latex"
            pdf-engine = "lualatex"

            [profile.pdf.variables]
            documentclass = "report"
            mainfont = "Noto Serif"
            sansfont = "Noto Sans"
            monofont = "Noto Sans Mono"
            mainfontfallback = [
              "NotoColorEmoji:mode=harf",
              "NotoSansMath:",
              "NotoSerifCJKSC:",
            ]
            monofontfallback = [
              "NotoColorEmoji:mode=harf",
              "NotoSansMath:",
              "NotoSansMonoCJKSC:",
              "NotoSansArabic:",
            ]
            geometry = ["margin=1.25in"]
        }
        .try_into()
        .unwrap()
    }

    fn pdf_and_latex() -> Self {
        let mut config = Self::pdf();
        config.profiles.extend(Self::latex().profiles);
        config
    }

    fn markdown() -> Self {
        toml! {
            keep-preprocessed = false

            [profile.markdown]
            output-file = "book.md"
            to = "commonmark_x"
            standalone = false
        }
        .try_into()
        .unwrap()
    }

    fn html() -> Self {
        toml! {
            keep-preprocessed = false

            [profile.html]
            output-file = "book.html"
            standalone = false
        }
        .try_into()
        .unwrap()
    }

    fn pandoc() -> Self {
        toml! {
            keep-preprocessed = false

            [profile.markdown]
            output-file = "pandoc-ir"
            to = "native"
            standalone = false
        }
        .try_into()
        .unwrap()
    }
}

mod basic;
mod config;
mod escaping;

mod alerts;
mod code;
mod css;
mod definition_lists;
mod fonts;
mod footnotes;
mod headings;
mod html;
mod images;
mod links;
mod math;
mod redirects;
mod super_sub;
mod tables;

mod books;
