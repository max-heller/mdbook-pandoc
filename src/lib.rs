use std::{
    collections::HashMap,
    fs::{self, File},
};

use anyhow::{anyhow, Context as _};
use mdbook::config::HtmlConfig;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

mod book;
use book::Book;

mod css;
mod html;
mod latex;
mod pandoc;

mod preprocess;
use preprocess::Preprocessor;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Config {
    #[serde(rename = "profile", default = "Default::default")]
    pub profiles: HashMap<String, pandoc::Profile>,
    #[serde(default = "defaults::enabled")]
    pub keep_preprocessed: bool,
    pub hosted_html: Option<String>,
    /// Code block related configuration.
    #[serde(default = "Default::default")]
    pub code: CodeConfig,
    /// Skip running the renderer.
    #[serde(default = "Default::default")]
    pub disabled: bool,
}

/// Configuration for tweaking how code blocks are rendered.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CodeConfig {
    pub show_hidden_lines: bool,
}

mod defaults {
    pub fn enabled() -> bool {
        true
    }
}

/// A [`mdbook`] backend supporting many output formats by relying on [`pandoc`](https://pandoc.org).
#[derive(Default)]
pub struct Renderer {
    logfile: Option<File>,
}

impl Renderer {
    pub fn new() -> Self {
        Self { logfile: None }
    }

    const NAME: &'static str = "pandoc";
    const CONFIG_KEY: &'static str = "output.pandoc";
}

impl mdbook::Renderer for Renderer {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn render(&self, ctx: &mdbook::renderer::RenderContext) -> anyhow::Result<()> {
        // If we're compiled against mdbook version I.J.K, require ^I.J
        // This allows using a version of mdbook with an earlier patch version as a server
        static MDBOOK_VERSION_REQ: Lazy<semver::VersionReq> = Lazy::new(|| {
            let compiled_mdbook_version = semver::Version::parse(mdbook::MDBOOK_VERSION).unwrap();
            semver::VersionReq {
                comparators: vec![semver::Comparator {
                    op: semver::Op::Caret,
                    major: compiled_mdbook_version.major,
                    minor: Some(compiled_mdbook_version.minor),
                    patch: None,
                    pre: Default::default(),
                }],
            }
        });
        let mdbook_server_version = semver::Version::parse(&ctx.version).unwrap();
        if !MDBOOK_VERSION_REQ.matches(&mdbook_server_version) {
            log::warn!(
                "{} is semver-incompatible with mdbook {} (requires {})",
                env!("CARGO_PKG_NAME"),
                mdbook_server_version,
                *MDBOOK_VERSION_REQ,
            );
        }

        let cfg: Config = ctx
            .config
            .get_deserialized_opt(Self::CONFIG_KEY)
            .with_context(|| format!("Unable to deserialize {}", Self::CONFIG_KEY))?
            .ok_or(anyhow!("No {} table found", Self::CONFIG_KEY))?;

        if cfg.disabled {
            log::info!("Skipping rendering since `disabled` is set");
            return Ok(());
        }

        pandoc::check_compatibility()?;

        let html_cfg: Option<HtmlConfig> = ctx
            .config
            .get_deserialized_opt("output.html")
            .unwrap_or_default();

        let book = Book::new(ctx)?;

        let stylesheets;
        let mut css = css::Css::default();
        if let Some(cfg) = &html_cfg {
            stylesheets = css::read_stylesheets(cfg, &book).collect::<Vec<_>>();
            for (stylesheet, stylesheet_css) in &stylesheets {
                css.load(stylesheet, stylesheet_css);
            }
        }

        for (name, profile) in cfg.profiles {
            let ctx = pandoc::RenderContext {
                book: &book,
                mdbook_cfg: &ctx.config,
                destination: book.destination.join(name),
                output: profile.output_format(),
                columns: profile.columns,
                cur_list_depth: 0,
                max_list_depth: 0,
                code: &cfg.code,
                html: html_cfg.as_ref(),
                css: &css,
            };

            // Preprocess book
            let mut preprocessor = Preprocessor::new(ctx)?;

            if let Some(uri) = cfg.hosted_html.as_deref() {
                preprocessor.hosted_html(uri);
            }

            if let Some(redirects) = html_cfg.as_ref().map(|cfg| &cfg.redirect) {
                if !redirects.is_empty() {
                    log::debug!("Processing redirects in [output.html.redirect]");
                    let redirects = redirects
                        .iter()
                        .map(|(src, dst)| (src.as_str(), dst.as_str()));
                    // In tests, sort redirect map to ensure stable log output
                    #[cfg(test)]
                    let redirects = redirects
                        .collect::<std::collections::BTreeMap<_, _>>()
                        .into_iter();
                    preprocessor.add_redirects(redirects);
                }
            }

            let mut preprocessed = preprocessor.preprocess();

            // Initialize renderer
            let mut renderer = pandoc::Renderer::new();

            // Add preprocessed book chapters to renderer
            renderer.current_dir(&book.root);
            for input in &mut preprocessed {
                renderer.input(input?);
            }

            if preprocessed.unresolved_links() {
                log::warn!(
                    "Unable to resolve one or more relative links within the book, \
                    consider setting the `hosted-html` option in `[output.pandoc]`"
                );
            }

            if let Some(logfile) = &self.logfile {
                renderer.stderr(logfile.try_clone()?);
            }

            // Render final output
            renderer.render(profile, preprocessed.render_context())?;

            if !cfg.keep_preprocessed {
                fs::remove_dir_all(preprocessed.output_dir())?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        fmt::{self, Write},
        fs,
        io::{self, Read, Seek},
        path::{Path, PathBuf},
        str::FromStr,
    };

    use mdbook::{BookItem, Renderer as _};
    use normpath::PathExt;
    use once_cell::sync::Lazy;
    use regex::Regex;
    use tempfile::{tempfile, TempDir};
    use toml::toml;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    pub struct MDBook {
        book: mdbook::MDBook,
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
            let mut book = mdbook::book::BookBuilder::new(root.path()).build().unwrap();

            // Clear out the stub files
            let src = book.source_dir();
            fs::remove_file(src.join("SUMMARY.md")).unwrap();
            for item in book.book.sections.drain(..) {
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
            MDBook::new(mdbook::MDBook::load(path).unwrap(), None, self)
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

        fn new(mut book: mdbook::MDBook, tempdir: Option<TempDir>, options: Options) -> Self {
            // Initialize logger to captures `log` output and redirect it to a tempfile
            let logfile = tempfile().unwrap();
            let _logger = tracing::subscriber::set_default(
                tracing_subscriber::registry()
                    .with(
                        tracing_subscriber::fmt::layer()
                            .compact()
                            .without_time()
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
            {
                let logger = tracing_log::LogTracer::new();
                let _ = log::set_boxed_logger(Box::new(logger));
                log::set_max_level(log::LevelFilter::Trace);
            }

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

        pub fn mdbook_config(mut self, config: mdbook::Config) -> Self {
            self.book.config = config;
            self
        }

        pub fn config(mut self, config: Config) -> Self {
            self.book.config.set(Renderer::CONFIG_KEY, config).unwrap();
            self
        }

        pub fn chapter(mut self, Chapter { mut chapter }: Chapter) -> Self {
            use mdbook::book::SectionNumber;
            let number = (self.book.book.sections.iter())
                .filter(
                    |item| matches!(item, BookItem::Chapter(chapter) if chapter.number.is_some()),
                )
                .count();
            chapter.number = Some(SectionNumber(vec![number as u32]));
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

    fn visualize_directory(
        dir: impl AsRef<Path>,
        mut writer: impl fmt::Write,
    ) -> anyhow::Result<()> {
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
                ]
                geometry = ["margin=1.25in"]
            }
            .try_into()
            .unwrap()
        }

        fn markdown() -> Self {
            toml! {
                keep-preprocessed = false

                [profile.markdown]
                output-file = "book.md"
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

    #[test]
    fn basic() {
        let book = MDBook::init()
            .chapter(Chapter::new(
                "Getting Started",
                "# Getting Started",
                "getting-started.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ # Getting Started {#book__markdown__src__getting-started.md__getting-started}
        ");
    }

    #[test]
    fn broken_links() {
        let book = MDBook::init()
            .chapter(Chapter::new(
                "Getting Started",
                "[broken link](foobarbaz)",
                "getting-started.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  WARN mdbook_pandoc::preprocess: Unable to normalize link 'foobarbaz' in chapter 'Getting Started': Unable to normalize path: $ROOT/src/foobarbaz: No such file or directory (os error 2)    
        │  WARN mdbook_pandoc: Unable to resolve one or more relative links within the book, consider setting the `hosted-html` option in `[output.pandoc]`    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ [broken link](foobarbaz)
        ");
    }

    #[test]
    fn strikethrough() {
        let book = MDBook::init()
            .chapter(Chapter::new("", "~test1~ ~~test2~~", "chapter.md"))
            .config(Config::latex())
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \st{test1} \st{test2}
        ├─ latex/src/chapter.md
        │ [Para [Strikeout [Str "test1"], Str " ", Strikeout [Str "test2"]]]
        "#);
    }

    #[test]
    fn task_lists() {
        let book = MDBook::init()
            .chapter(Chapter::new(
                "",
                "- [x] Complete task\n- [ ] Incomplete task",
                "chapter.md",
            ))
            .config(Config::latex())
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \begin{itemize}
        │ \tightlist
        │ \item[$\boxtimes$]
        │   Complete task
        │ \item[$\square$]
        │   Incomplete task
        │ \end{itemize}
        ├─ latex/src/chapter.md
        │ [BulletList [[Plain [Str "\9746", Space, Str "Complete task"]], [Plain [Str "\9744", Space, Str "Incomplete task"]]]]
        "#);
    }

    #[test]
    fn heading_attributes() {
        let book = MDBook::init()
            .chapter(Chapter::new(
                "",
                "# Heading { #custom-heading }\n[heading](#custom-heading)",
                "chapter.md",
            ))
            .config(Config::latex())
            .build();
        insta::assert_snapshot!(book, @r##"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \chapter{Heading}\label{book__latex__src__chapter.md__custom-heading}
        │ 
        │ \hyperref[book__latex__src__chapter.md__custom-heading]{heading}
        ├─ latex/src/chapter.md
        │ [Header 1 ("custom-heading", [], []) [Str "Heading"], Para [Link ("", [], []) [Str "heading"] ("#custom-heading", "")]]
        "##);
    }

    #[test]
    fn footnotes() {
        // Pandoc doesn't support nested footnotes (it won't output anything meaningful for them)
        // but we output the AST for them anyway. See https://github.com/jgm/pandoc/issues/2053
        let book = MDBook::init()
            .chapter(Chapter::new(
                "",
                "
hello[^1]

[^1]: a footnote containing another footnote[^2]
[^2]: second footnote
                ",
                "chapter.md",
            ))
            .config(Config::latex())
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ hello\footnote{a footnote containing another footnote\footnotemark{}}
        ├─ latex/src/chapter.md
        │ [Para [Str "hello", Note [Para [Str "a footnote containing another footnote", Note [Para [Str "second footnote"]]]]]]
        "#);
    }

    #[test]
    fn tables() {
        let book = MDBook::init()
            .chapter(Chapter::new(
                "",
                "
| Header1 | Header2 |
|---------|---------|
| abc     | def     |
                ",
                "chapter.md",
            ))
            .config(Config::latex())
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \begin{longtable}[]{@{}ll@{}}
        │ \toprule\noalign{}
        │ Header1 & Header2 \\
        │ \midrule\noalign{}
        │ \endhead
        │ \bottomrule\noalign{}
        │ \endlastfoot
        │ abc & def \\
        │ \end{longtable}
        ├─ latex/src/chapter.md
        │ [Table ("", [], []) (Caption Nothing []) [(AlignDefault, ColWidthDefault), (AlignDefault, ColWidthDefault)] (TableHead ("", [], []) [Row ("", [], []) [Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "Header1"]], Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "Header2"]]]]) [(TableBody ("", [], []) (RowHeadColumns 0) [] [Row ("", [], []) [Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "abc"]], Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "def"]]]])] (TableFoot ("", [], []) [])]
        "#);
    }

    #[test]
    fn wide_table() {
        let book = MDBook::init()
            .chapter(Chapter::new(
                "",
                "
| Header1 | Header2 |
| ------- | :--------------------------------------------------------------- |
| abc     | long long long long long long long long long long long long long |
                ",
                "chapter.md",
            ))
            .config(Config::latex())
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \begin{longtable}[]{@{}
        │   >{\raggedright\arraybackslash}p{(\linewidth - 2\tabcolsep) * \real{0.0986}}
        │   >{\raggedright\arraybackslash}p{(\linewidth - 2\tabcolsep) * \real{0.9014}}@{}}
        │ \toprule\noalign{}
        │ \begin{minipage}[b]{\linewidth}\raggedright
        │ Header1
        │ \end{minipage} & \begin{minipage}[b]{\linewidth}\raggedright
        │ Header2
        │ \end{minipage} \\
        │ \midrule\noalign{}
        │ \endhead
        │ \bottomrule\noalign{}
        │ \endlastfoot
        │ abc &
        │ long long long long long long long long long long long long long \\
        │ \end{longtable}
        ├─ latex/src/chapter.md
        │ [Table ("", [], []) (Caption Nothing []) [(AlignDefault, (ColWidth 0.09859154929577464)), (AlignLeft, (ColWidth 0.9014084507042254))] (TableHead ("", [], []) [Row ("", [], []) [Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "Header1"]], Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "Header2"]]]]) [(TableBody ("", [], []) (RowHeadColumns 0) [] [Row ("", [], []) [Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "abc"]], Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "long long long long long long long long long long long long long"]]]])] (TableFoot ("", [], []) [])]
        "#);
    }

    #[test]
    fn parts() {
        let book = MDBook::init()
            .chapter(Chapter::new("", "# One", "one.md"))
            .part("part two")
            .chapter(Chapter::new("", "# Two", "two.md"))
            .config(Config::latex())
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \chapter{One}\label{book__latex__src__one.md__one}
        │ 
        │ \part{part two}
        │ 
        │ \chapter{Two}\label{book__latex__src__two.md__two}
        ├─ latex/src/one.md
        │ [Header 1 ("one", [], []) [Str "One"]]
        ├─ latex/src/part-1-part-two.md
        │ [Para [RawInline (Format "latex") "\\part{part two}"]]
        ├─ latex/src/two.md
        │ [Header 1 ("two", [], []) [Str "Two"]]
        "#);
    }

    #[test]
    fn inter_chapter_links() {
        let book = MDBook::init()
            .chapter(Chapter::new(
                "One",
                "# One\n[Two](../two/two.md)",
                "one/one.md",
            ))
            .chapter(Chapter::new(
                "Two",
                "# Two\n[One](../one/one.md)\n[also one](/one/one.md)\n[Three](../three.md)",
                "two/two.md",
            ))
            .chapter(Chapter::new("Three", "", "three.md"))
            .config(Config::latex())
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  WARN mdbook_pandoc::preprocess: Failed to determine suitable anchor for beginning of chapter 'Three'--does it contain any headings?    
        │  WARN mdbook_pandoc::preprocess: Unable to normalize link '../three.md' in chapter 'Two': failed to link to beginning of chapter    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \chapter{One}\label{book__latex__src__one__one.md__one}
        │ 
        │ \hyperref[book__latex__src__two__two.md__two]{Two}
        │ 
        │ \chapter{Two}\label{book__latex__src__two__two.md__two}
        │ 
        │ \hyperref[book__latex__src__one__one.md__one]{One}
        │ \hyperref[book__latex__src__one__one.md__one]{also one}
        │ \href{../three.md}{Three}
        ├─ latex/src/one/one.md
        │ [Header 1 ("one", [], []) [Str "One"], Para [Link ("", [], []) [Str "Two"] ("book/latex/src/two/two.md#two", "")]]
        ├─ latex/src/three.md
        │ []
        ├─ latex/src/two/two.md
        │ [Header 1 ("two", [], []) [Str "Two"], Para [Link ("", [], []) [Str "One"] ("book/latex/src/one/one.md#one", ""), SoftBreak, Link ("", [], []) [Str "also one"] ("book/latex/src/one/one.md#one", ""), SoftBreak, Link ("", [], []) [Str "Three"] ("../three.md", "")]]
        "#);
    }

    #[test]
    fn nested_chapters() {
        let book = MDBook::init()
            .chapter(Chapter::new("One", "# One", "one.md").child(Chapter::new(
                "One.One",
                "# Top\n## Another",
                "onepointone.md",
            )))
            .chapter(Chapter::new("Two", "# Two", "two.md"))
            .config(Config::latex())
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \chapter{One}\label{book__latex__src__one.md__one}
        │ 
        │ \section{Top}\label{book__latex__src__onepointone.md__top}
        │ 
        │ \subsection*{Another}\label{book__latex__src__onepointone.md__another}
        │ 
        │ \chapter{Two}\label{book__latex__src__two.md__two}
        ├─ latex/src/one.md
        │ [Header 1 ("one", [], []) [Str "One"]]
        ├─ latex/src/onepointone.md
        │ [Header 2 ("top", [], []) [Str "Top"], Header 3 ("another", ["unnumbered", "unlisted"], []) [Str "Another"]]
        ├─ latex/src/two.md
        │ [Header 1 ("two", [], []) [Str "Two"]]
        "#);
    }

    #[test]
    fn font_awesome_icons() {
        let book = MDBook::init()
            .config(Config::latex())
            .chapter(Chapter::new(
                "",
                r#"
<i class="fa fa-print"></i>
<i class = "fa fa-print"/></i>
                "#,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \faicon{print} \faicon{print}
        ├─ latex/src/chapter.md
        │ [Para [RawInline (Format "latex") "\\faicon{print}", SoftBreak, RawInline (Format "latex") "\\faicon{print}"]]
        "#);

        let book = MDBook::init()
            .chapter(Chapter::new(
                "",
                r#"<i class="fa fa-print"></i>"#,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ `<i class="fa fa-print">`{=html}`</i>`{=html}
        "#);
    }

    #[test]
    fn code_block_with_hidden_lines() {
        let content = r#"
```rust
# fn main() {
    # // another hidden line
println!("Hello, world!");
# }
```
        "#;
        let book = MDBook::init()
            .config(Config::markdown())
            .chapter(Chapter::new("", content, "chapter.md"))
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ ``` rust
        │ println!("Hello, world!");
        │ ```
        "#);
        let book = MDBook::init()
            .config(Config {
                code: CodeConfig {
                    show_hidden_lines: true,
                },
                ..Config::markdown()
            })
            .chapter(Chapter::new("", content, "chapter.md"))
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ ``` rust
        │ # fn main() {
        │     # // another hidden line
        │ println!("Hello, world!");
        │ # }
        │ ```
        "#);
    }

    #[test]
    fn non_rust_code_block_with_hidden_lines() {
        let content = r#"
```python
~hidden()
nothidden():
~    hidden()
    ~hidden()
    nothidden()
```
        "#;
        let cfg = r#"
[output.html.code.hidelines]
python = "~"
        "#;
        let book = MDBook::init()
            .mdbook_config(cfg.parse().unwrap())
            .config(Config::markdown())
            .chapter(Chapter::new("", content, "chapter.md"))
            .build();
        insta::assert_snapshot!(book, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ ``` python
        │ nothidden():
        │     nothidden()
        │ ```
        ");
        let book = MDBook::init()
            .config(Config {
                code: CodeConfig {
                    show_hidden_lines: true,
                },
                ..Config::markdown()
            })
            .chapter(Chapter::new("", content, "chapter.md"))
            .build();
        insta::assert_snapshot!(book, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ ``` python
        │ ~hidden()
        │ nothidden():
        │ ~    hidden()
        │     ~hidden()
        │     nothidden()
        │ ```
        ");
    }

    #[test]
    fn code_block_hidelines_override() {
        let content = r#"
```python,hidelines=!!!
!!!hidden()
nothidden():
!!!    hidden()
    !!!hidden()
    nothidden()
```
        "#;
        let book = MDBook::init()
            .config(Config::markdown())
            .chapter(Chapter::new("", content, "chapter.md"))
            .build();
        insta::assert_snapshot!(book, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ ``` python
        │ nothidden():
        │     nothidden()
        │ ```
        ");
    }

    #[test]
    #[ignore]
    fn code_block_with_very_long_line() {
        let long_line = str::repeat("long ", 1000);
        let content = format!(
            "
```java
{long_line}
```
            "
        );
        let book = MDBook::init()
            .config(Config::pdf())
            .chapter(Chapter::new("", content, "chapter.md"))
            .build();
        insta::assert_snapshot!(book, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/pdf/book.pdf    
        ├─ pdf/book.pdf
        │ <INVALID UTF8>
        ");
    }

    #[test]
    #[ignore]
    fn code_block_with_very_long_line_with_special_characters() {
        let content = r#"""
```console
$ rustc json_error_demo.rs --error-format json
{"message":"cannot add `&str` to `{integer}`","code":{"code":"E0277","explanation":"\nYou tried to use a type which doesn't implement some trait in a place which\nexpected that trait. Erroneous code example:\n\n```compile_fail,E0277\n// here we declare the Foo trait with a bar method\ntrait Foo {\n    fn bar(&self);\n}\n\n// we now declare a function which takes an object implementing the Foo trait\nfn some_func<T: Foo>(foo: T) {\n    foo.bar();\n}\n\nfn main() {\n    // we now call the method with the i32 type, which doesn't implement\n    // the Foo trait\n    some_func(5i32); // error: the trait bound `i32 : Foo` is not satisfied\n}\n```\n\nIn order to fix this error, verify that the type you're using does implement\nthe trait. Example:\n\n```\ntrait Foo {\n    fn bar(&self);\n}\n\nfn some_func<T: Foo>(foo: T) {\n    foo.bar(); // we can now use this method since i32 implements the\n               // Foo trait\n}\n\n// we implement the trait on the i32 type\nimpl Foo for i32 {\n    fn bar(&self) {}\n}\n\nfn main() {\n    some_func(5i32); // ok!\n}\n```\n\nOr in a generic context, an erroneous code example would look like:\n\n```compile_fail,E0277\nfn some_func<T>(foo: T) {\n    println!(\"{:?}\", foo); // error: the trait `core::fmt::Debug` is not\n                           //        implemented for the type `T`\n}\n\nfn main() {\n    // We now call the method with the i32 type,\n    // which *does* implement the Debug trait.\n    some_func(5i32);\n}\n```\n\nNote that the error here is in the definition of the generic function: Although\nwe only call it with a parameter that does implement `Debug`, the compiler\nstill rejects the function: It must work with all possible input types. In\norder to make this example compile, we need to restrict the generic type we're\naccepting:\n\n```\nuse std::fmt;\n\n// Restrict the input type to types that implement Debug.\nfn some_func<T: fmt::Debug>(foo: T) {\n    println!(\"{:?}\", foo);\n}\n\nfn main() {\n    // Calling the method is still fine, as i32 implements Debug.\n    some_func(5i32);\n\n    // This would fail to compile now:\n    // struct WithoutDebug;\n    // some_func(WithoutDebug);\n}\n```\n\nRust only looks at the signature of the called function, as such it must\nalready specify all requirements that will be used for every type parameter.\n"},"level":"error","spans":[{"file_name":"json_error_demo.rs","byte_start":50,"byte_end":51,"line_start":4,"line_end":4,"column_start":7,"column_end":8,"is_primary":true,"text":[{"text":"    a + b","highlight_start":7,"highlight_end":8}],"label":"no implementation for `{integer} + &str`","suggested_replacement":null,"suggestion_applicability":null,"expansion":null}],"children":[{"message":"the trait `std::ops::Add<&str>` is not implemented for `{integer}`","code":null,"level":"help","spans":[],"children":[],"rendered":null}],"rendered":"error[E0277]: cannot add `&str` to `{integer}`\n --> json_error_demo.rs:4:7\n  |\n4 |     a + b\n  |       ^ no implementation for `{integer} + &str`\n  |\n  = help: the trait `std::ops::Add<&str>` is not implemented for `{integer}`\n\n"}
```
            """#;
        let book = MDBook::init()
            .config(Config::pdf())
            .chapter(Chapter::new("", content, "chapter.md"))
            .build();
        insta::assert_snapshot!(book, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/pdf/book.pdf    
        ├─ pdf/book.pdf
        │ <INVALID UTF8>
        ");
    }

    #[test]
    fn mdbook_rust_code_block_attributes() {
        let book = MDBook::init()
            .config(Config::latex())
            .chapter(Chapter::new(
                "",
                r#"
```rust
fn main() {}
```
```rust,ignore
fn main() {}
```
                "#,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \begin{Shaded}
        │ \begin{Highlighting}[]
        │ \KeywordTok{fn}\NormalTok{ main() }\OperatorTok{\{\}}
        │ \end{Highlighting}
        │ \end{Shaded}
        │ 
        │ \begin{Shaded}
        │ \begin{Highlighting}[]
        │ \KeywordTok{fn}\NormalTok{ main() }\OperatorTok{\{\}}
        │ \end{Highlighting}
        │ \end{Shaded}
        ├─ latex/src/chapter.md
        │ [CodeBlock ("", ["rust"], []) "fn main() {}
        │ ", CodeBlock ("", ["rust"], []) "fn main() {}
        │ "]
        "#);
    }

    #[test]
    fn link_to_element_by_id() {
        let book = MDBook::init()
            .config(Config::latex())
            .chapter(Chapter::new(
                "",
                r#"
<a id="test">some text here</a>
<span id="test2">some text here</span>

<div id="test3">
some text here
</div>

<div id="test4">some text here</div>

[test link](#test)
[test2 link](#test2)
[test3 link](#test3)
[test4 link](#test4)
                "#,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r##"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \phantomsection\label{book__latex__src__chapter.md__test}{some text here}
        │ \phantomsection\label{book__latex__src__chapter.md__test2}{some text here}
        │ 
        │ \phantomsection\label{book__latex__src__chapter.md__test3}
        │ 
        │ some text here
        │ 
        │ \phantomsection\label{book__latex__src__chapter.md__test4}
        │ some text here
        │ 
        │ \hyperref[book__latex__src__chapter.md__test]{test link}
        │ \hyperref[book__latex__src__chapter.md__test2]{test2 link}
        │ \hyperref[book__latex__src__chapter.md__test3]{test3 link}
        │ \hyperref[book__latex__src__chapter.md__test4]{test4 link}
        ├─ latex/src/chapter.md
        │ [Para [Span ("test", [], []) [Str "some text here"], SoftBreak, Span ("test2", [], []) [Str "some text here"]], Div ("test3", [], []) [Plain [Str "
        │ some text here
        │ "]], Plain [Str "
        │ "], Div ("test4", [], []) [Plain [Str "some text here"]], Plain [Str "
        │ "], Para [Link ("", [], []) [Str "test link"] ("#test", ""), SoftBreak, Link ("", [], []) [Str "test2 link"] ("#test2", ""), SoftBreak, Link ("", [], []) [Str "test3 link"] ("#test3", ""), SoftBreak, Link ("", [], []) [Str "test4 link"] ("#test4", "")]]
        "##);
    }

    #[test]
    fn attach_id_to_div_of_stripped_html_elements() {
        let book = MDBook::init()
            .config(Config::latex())
            .chapter(Chapter::new(
                "",
                r##"<dt id="foo=bar"><a href="#foo=bar"></a>something here</dt>"##,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r##"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \phantomsection\label{book__latex__src__chapter.md__foo=bar}
        │ \hyperref[book__latex__src__chapter.md__foo=bar]{}something here
        ├─ latex/src/chapter.md
        │ [RawBlock (Format "html") "<dt id=\"foo=bar\">", Div ("foo=bar", [], []) [Plain [Link ("", [], [("href", "#foo=bar")]) [] ("#foo=bar", ""), Str "something here"]], RawBlock (Format "html") "</dt>"]
        "##);
    }

    #[test]
    fn implicitly_closed_tags() {
        let book = MDBook::init()
            .config(Config::latex())
            .chapter(Chapter::new(
                "",
                r#"
- before
- [Box<T>](#foo)
- after

# Foo
                "#,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r##"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \begin{itemize}
        │ \tightlist
        │ \item
        │   before
        │ \item
        │   \hyperref[book__latex__src__chapter.md__foo]{Box}
        │ \item
        │   after
        │ \end{itemize}
        │ 
        │ \chapter{Foo}\label{book__latex__src__chapter.md__foo}
        ├─ latex/src/chapter.md
        │ [BulletList [[Plain [Str "before"]], [Plain [Link ("", [], []) [Str "Box", RawInline (Format "html") "<t>", RawInline (Format "html") "</t>"] ("#foo", "")]], [Plain [Str "after"]]], Header 1 ("foo", [], []) [Str "Foo"]]
        "##);
    }

    #[test]
    fn rust_reference_regression_nested_elements() {
        let book = MDBook::init()
            .config(Config::latex())
            .chapter(Chapter::new(
                "",
                r##"
<div id="my-div">
<a id="my-a" href="#my-div">[some text here]</a>
</div>

[div](#my-div)
[a](#my-a)
                "##,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r##"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \phantomsection\label{book__latex__src__chapter.md__my-div}
        │ 
        │ \phantomsection\label{book__latex__src__chapter.md__my-a}\hyperref[book__latex__src__chapter.md__my-div]{{[}some text here{]}}
        │ 
        │ \hyperref[book__latex__src__chapter.md__my-div]{div}
        │ \hyperref[book__latex__src__chapter.md__my-a]{a}
        ├─ latex/src/chapter.md
        │ [Div ("my-div", [], []) [Plain [Str "
        │ ", Link ("my-a", [], [("href", "#my-div")]) [Str "[some text here]"] ("#my-div", ""), Str "
        │ "]], Plain [Str "
        │ "], Para [Link ("", [], []) [Str "div"] ("#my-div", ""), SoftBreak, Link ("", [], []) [Str "a"] ("#my-a", "")]]
        "##);
    }

    #[test]
    fn link_title_containing_quotes() {
        let book = MDBook::init()
            .config(Config::latex())
            .chapter(Chapter::new(
                "",
                r#"
# Chapter Foo

[link][link-with-description]

[link-with-description]: chapter.md '"foo" (bar)'
                "#,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \chapter{Chapter Foo}\label{book__latex__src__chapter.md__chapter-foo}
        │ 
        │ \hyperref[book__latex__src__chapter.md__chapter-foo]{link}
        ├─ latex/src/chapter.md
        │ [Header 1 ("chapter-foo", [], []) [Str "Chapter Foo"], Para [Link ("", [], []) [Str "link"] ("book/latex/src/chapter.md#chapter-foo", "\"foo\" (bar)")], Para []]
        "#);
    }

    #[test]
    fn single_chapter_with_explicit_self_link() {
        let book = MDBook::init()
            .config(Config::latex())
            .chapter(Chapter::new(
                "Chapter One",
                "# Chapter One\n[link](chapter.md)",
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \chapter{Chapter One}\label{book__latex__src__chapter.md__chapter-one}
        │ 
        │ \hyperref[book__latex__src__chapter.md__chapter-one]{link}
        ├─ latex/src/chapter.md
        │ [Header 1 ("chapter-one", [], []) [Str "Chapter One"], Para [Link ("", [], []) [Str "link"] ("book/latex/src/chapter.md#chapter-one", "")]]
        "#);
    }

    #[test]
    fn preserve_escapes() {
        let output = MDBook::init()
            .config(Config::pandoc())
            .chapter(Chapter::new("", "[Prefix @fig:1] [-@fig:1]", "chapter.md"))
            .build();
        insta::assert_snapshot!(output, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir    
        ├─ markdown/pandoc-ir
        │ [ Para
        │     [ Str "["
        │     , Str "Prefix @fig:1"
        │     , Str "]"
        │     , Str " "
        │     , Str "["
        │     , Str "-@fig:1"
        │     , Str "]"
        │     ]
        │ ]
        "#);
    }

    #[test]
    fn html_comments() {
        let output = MDBook::init()
            .config(Config::markdown())
            .chapter(Chapter::new("", "<!-- Comment -->", "chapter.md"))
            .build();
        insta::assert_snapshot!(output, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ <!-- Comment -->
        ");
    }

    #[test]
    fn nested_html_block() {
        let s = "
> <!-- hello
>
> world -->
                ";
        let output = MDBook::init()
            .config(Config::markdown())
            .chapter(Chapter::new("", s, "chapter.md"))
            .build();
        insta::assert_snapshot!(output, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ > <!-- hello
        │ >
        │ > world -->
        ");
    }

    #[test]
    fn matched_html_tags() {
        let ast = MDBook::init()
            .chapter(Chapter::new(
                "Chapter",
                "
<details>
<summary>

## Heading

text

</summary>

more **markdown**

</details>

outside divs
                ",
                "chapter.md",
            ))
            .config(Config::pandoc())
            .build();
        insta::assert_snapshot!(ast, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir    
        ├─ markdown/pandoc-ir
        │ [ RawBlock (Format "html") "<details>"
        │ , Div
        │     ( "" , [] , [] )
        │     [ Plain [ Str "\n" , RawInline (Format "html") "<summary>" ]
        │     , Div
        │         ( "" , [] , [] )
        │         [ Plain [ Str "\n" ]
        │         , Header
        │             2
        │             ( "book__markdown__src__chapter.md__heading"
        │             , [ "unnumbered" , "unlisted" ]
        │             , []
        │             )
        │             [ Str "Heading" ]
        │         , Para [ Str "text" ]
        │         ]
        │     , RawBlock (Format "html") "</summary>"
        │     , Plain [ Str "\n" ]
        │     , Para [ Str "more " , Strong [ Str "markdown" ] ]
        │     ]
        │ , RawBlock (Format "html") "</details>"
        │ , Plain [ Str "\n" ]
        │ , Para [ Str "outside divs" ]
        │ ]
        "#);

        // Make sure logic doesn't trigger on inline html since inserting divs
        // introduces newlines and breaks the original structure
        let output = MDBook::init()
            .config(Config::markdown())
            .chapter(Chapter::new("Chapter", "2<sup>n - 1</sup>", "chapter.md"))
            .build();
        insta::assert_snapshot!(output, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ 2`<sup>`{=html}n - 1`</sup>`{=html}
        ");
    }

    #[test]
    fn raw_opts() {
        let cfg = r#"
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
        "#;
        let output = MDBook::options()
            .max_log_level(tracing::Level::TRACE)
            .init()
            .mdbook_config(mdbook::Config::from_str(cfg).unwrap())
            .build();
        insta::assert_snapshot!(output, @r#"
        ├─ log output
        │ DEBUG mdbook::book: Running the index preprocessor.    
        │ DEBUG mdbook::book: Running the links preprocessor.    
        │  INFO mdbook::book: Running the pandoc backend    
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

    #[test]
    fn disabled() {
        let cfg = r#"
[output.pandoc]
disabled = true
        "#;
        let output = MDBook::init()
            .mdbook_config(mdbook::Config::from_str(cfg).unwrap())
            .build();
        insta::assert_snapshot!(output, @r###"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc: Skipping rendering since `disabled` is set    
        "###)
    }

    #[test]
    fn redirects() {
        let cfg = r#"
[output.pandoc.profile.test]
output-file = "/dev/null"
to = "markdown"

[output.html.redirect]
"/appendices/bibliography.html" = "https://rustc-dev-guide.rust-lang.org/appendix/bibliography.html"
"/foo/bar.html" = "../new-bar.html"
"/new-bar.html" = "new-new-bar.html"
        "#;
        let output = MDBook::options()
            .max_log_level(tracing::Level::DEBUG)
            .init()
            .mdbook_config(mdbook::Config::from_str(cfg).unwrap())
            .chapter(Chapter::new(
                "",
                "[bar](foo/bar.md)\n[bib](appendices/bibliography.html)",
                "index.md",
            ))
            .chapter(Chapter::new("", "# New New Bar", "new-new-bar.md"))
            .build();
        insta::assert_snapshot!(output, @r#"
        ├─ log output
        │ DEBUG mdbook::book: Running the index preprocessor.    
        │ DEBUG mdbook::book: Running the links preprocessor.    
        │  INFO mdbook::book: Running the pandoc backend    
        │ DEBUG mdbook_pandoc: Processing redirects in [output.html.redirect]    
        │ DEBUG mdbook_pandoc::preprocess: Processing redirect: /appendices/bibliography.html => https://rustc-dev-guide.rust-lang.org/appendix/bibliography.html    
        │ DEBUG mdbook_pandoc::preprocess: Processing redirect: /foo/bar.html => ../new-bar.html    
        │ DEBUG mdbook_pandoc::preprocess: Processing redirect: /new-bar.html => new-new-bar.html    
        │ DEBUG mdbook_pandoc::preprocess: Registered redirect: book/test/src/appendices/bibliography.html => https://rustc-dev-guide.rust-lang.org/appendix/bibliography.html    
        │ DEBUG mdbook_pandoc::preprocess: Registered redirect: book/test/src/foo/bar.html => book/test/src/new-bar.html    
        │ DEBUG mdbook_pandoc::preprocess: Registered redirect: book/test/src/new-bar.html => book/test/src/new-new-bar.md#new-new-bar    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to /dev/null    
        ├─ test/src/appendices/bibliography.html
        ├─ test/src/foo/bar.html
        ├─ test/src/index.md
        │ [Para [Link ("", [], []) [Str "bar"] ("book/test/src/new-new-bar.md#new-new-bar", ""), SoftBreak, Link ("", [], []) [Str "bib"] ("https://rustc-dev-guide.rust-lang.org/appendix/bibliography.html", "")]]
        ├─ test/src/new-bar.html
        ├─ test/src/new-new-bar.md
        │ [Header 1 ("new-new-bar", [], []) [Str "New New Bar"]]
        "#)
    }

    #[test]
    fn images() {
        let book = MDBook::init()
            .config(Config::latex())
            .file_in_src("img/image.png", "")
            .chapter(Chapter::new(
                "",
                r#"
![alt text](img/image.png "a title")
<img src="img/image.png" alt="alt text" title = "a title" width=50 height=100 class="foo bar">
<img src="img/image.png" alt="alt text" title = "a title" style="width:50; height: 100" class="foo bar">
                "#,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \pandocbounded{\includegraphics[keepaspectratio]{book/latex/src/img/image.png}}
        │ \includegraphics[width=0.52083in,height=1.04167in]{book/latex/src/img/image.png}
        │ \includegraphics[width=0.52083in,height=1.04167in]{book/latex/src/img/image.png}
        ├─ latex/src/chapter.md
        │ [Para [Image ("", [], []) [Str "alt text"] ("book/latex/src/img/image.png", "a title"), SoftBreak, Image ("", ["foo", "bar"], [("height", "100"), ("width", "50")]) [Str "alt text"] ("book/latex/src/img/image.png", "a title"), SoftBreak, Image ("", ["foo", "bar"], [("width", "50"), ("height", "100")]) [Str "alt text"] ("book/latex/src/img/image.png", "a title")]]
        ├─ latex/src/img/image.png
        "#);
    }

    #[test]
    #[ignore]
    fn remote_images() {
        let book = MDBook::init()
            .config(Config::pdf())
            .chapter(Chapter::new(
                "",
                r#"
[![Build](https://github.com/rust-lang/mdBook/workflows/CI/badge.svg?event=push)](https://github.com/rust-lang/mdBook/actions?query=workflow%3ACI+branch%3Amaster)
[![Build](https://img.shields.io/github/actions/workflow/status/rust-lang/mdBook/main.yml?style=flat-square)](https://github.com/rust-lang/mdBook/actions/workflows/main.yml?query=branch%3Amaster)
[![crates.io](https://img.shields.io/crates/v/mdbook.svg)](https://crates.io/crates/mdbook)
[![GitHub contributors](https://img.shields.io/github/contributors/rust-lang/mdBook?style=flat-square)](https://github.com/rust-lang/mdBook/graphs/contributors)
[![GitHub stars](https://img.shields.io/github/stars/rust-lang/mdBook?style=flat-square)](https://github.com/rust-lang/mdBook/stargazers)
                "#,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/pdf/book.pdf    
        ├─ pdf/book.pdf
        │ <INVALID UTF8>
        ");
    }

    #[test]
    fn unresolvable_remote_images() {
        let book = MDBook::init()
            .config(Config::markdown())
            .chapter(Chapter::new(
                "Some Chapter",
                "prefix ![test image](https://doesnotexist.fake/main.yml?style=flat-square) suffix",
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  WARN mdbook_pandoc::preprocess: Failed to resolve image link 'https://doesnotexist.fake/main.yml?style=flat-square' in chapter 'Some Chapter': could not fetch remote image: Dns Failed    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
        ├─ markdown/book.md
        │ prefix test image suffix
        ");
    }

    #[test]
    fn css() {
        let cfg = r#"
[output.html]
additional-css = ["ferris.css"]
        "#;
        let book = MDBook::init()
            .mdbook_config(mdbook::Config::from_str(cfg).unwrap())
            .config(Config::latex())
            .file_in_src("img/image.png", "")
            .file_in_root(
                "ferris.css",
                "
.ferris-explain {
  width: 100px;
  height: 50;
}
                ",
            )
            .chapter(Chapter::new(
                "",
                r#"
<img class="ferris-explain" src="img/image.png" alt="alt text" title = "a title">
                "#,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
        ├─ latex/output.tex
        │ \includegraphics[width=1.04167in,height=0.52083in]{book/latex/src/img/image.png}
        ├─ latex/src/chapter.md
        │ [Plain [Image ("", ["ferris-explain"], [("height", "50"), ("width", "100px")]) [Str "alt text"] ("book/latex/src/img/image.png", "a title"), Str "
        │ "]]
        ├─ latex/src/img/image.png
        "#);
    }

    #[test]
    fn regression_inline_code_newline() {
        let book = MDBook::init()
            .config(Config::pandoc())
            .chapter(Chapter::new(
                "",
                // Important for inline code to be in the same inline container as the rest of the item
                "- Writing a program that prints `Hello, world!`",
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir    
        ├─ markdown/pandoc-ir
        │ [ BulletList
        │     [ [ Plain
        │           [ Str "Writing a program that prints "
        │           , Code ( "" , [] , [] ) "Hello, world!"
        │           ]
        │       ]
        │     ]
        │ ]
        "#);
    }

    #[test]
    fn footnotes_get_preprocessed() {
        let book = MDBook::init()
            .config(Config::pandoc())
            .chapter(Chapter::new(
                "",
                r#"
hello[^1]

[^1]: a footnote containing another footnote[^2]
[^2]: <a href="example.com"></a>
                "#,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r#"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir    
        ├─ markdown/pandoc-ir
        │ [ Para
        │     [ Str "hello"
        │     , Note
        │         [ Para
        │             [ Str "a footnote containing another footnote"
        │             , Note
        │                 [ Para
        │                     [ Link
        │                         ( "" , [] , [ ( "href" , "example.com" ) ] )
        │                         []
        │                         ( "example.com" , "" )
        │                     ]
        │                 ]
        │             ]
        │         ]
        │     ]
        │ ]
        "#);
    }

    #[test]
    fn code_escaping() {
        let book = MDBook::init()
            .config(Config::pandoc())
            .chapter(Chapter::new(
                "",
                r###"
```rust
"foo"; r"foo";                     // foo
"\"foo\""; r#""foo""#;             // "foo"

"foo #\"# bar";
r##"foo #"# bar"##;                // foo #"# bar

"\x52"; "R"; r"R";                 // R
"\\x52"; r"\x52";                  // \x52
```
`"foo"; r"foo";                     // foo`
`"\"foo\""; r#""foo""#;             // "foo"`
`"foo #\"# bar";`
`r##"foo #"# bar"##;                // foo #"# bar`
`"\x52"; "R"; r"R";                 // R`
`"\\x52"; r"\x52";                  // \x52`
                "###,
                "chapter.md",
            ))
            .build();
        insta::assert_snapshot!(book, @r###"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir    
        ├─ markdown/pandoc-ir
        │ [ CodeBlock
        │     ( "" , [ "rust" ] , [] )
        │     "\"foo\"; r\"foo\";                     // foo\n\"\\\"foo\\\"\"; r#\"\"foo\"\"#;             // \"foo\"\n\n\"foo #\\\"# bar\";\nr##\"foo #\"# bar\"##;                // foo #\"# bar\n\n\"\\x52\"; \"R\"; r\"R\";                 // R\n\"\\\\x52\"; r\"\\x52\";                  // \\x52\n"
        │ , Para
        │     [ Code
        │         ( "" , [] , [] )
        │         "\"foo\"; r\"foo\";                     // foo"
        │     , SoftBreak
        │     , Code
        │         ( "" , [] , [] )
        │         "\"\\\"foo\\\"\"; r#\"\"foo\"\"#;             // \"foo\""
        │     , SoftBreak
        │     , Code ( "" , [] , [] ) "\"foo #\\\"# bar\";"
        │     , SoftBreak
        │     , Code
        │         ( "" , [] , [] )
        │         "r##\"foo #\"# bar\"##;                // foo #\"# bar"
        │     , SoftBreak
        │     , Code
        │         ( "" , [] , [] )
        │         "\"\\x52\"; \"R\"; r\"R\";                 // R"
        │     , SoftBreak
        │     , Code
        │         ( "" , [] , [] )
        │         "\"\\\\x52\"; r\"\\x52\";                  // \\x52"
        │     ]
        │ ]
        "###);
    }

    #[test]
    fn repeated_identifiers() {
        let book = MDBook::init()
            .config(Config::pandoc())
            .chapter(Chapter::new(
                "",
                "# Hello\n# Hello\n[first](#hello)[second](#hello-1)",
                "chapter.md",
            ))
            .chapter(Chapter::new(
                "",
                "# Hello\n# Hello\n[first](#hello)[second](#hello-1)",
                "chapter2.md",
            ))
            .chapter(Chapter::new("", "# ?\n# ?\n[second](#-1)", "weird-ids.md"))
            .build();
        insta::assert_snapshot!(book, @r##"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir    
        ├─ markdown/pandoc-ir
        │ [ Header
        │     1
        │     ( "book__markdown__src__chapter.md__hello" , [] , [] )
        │     [ Str "Hello" ]
        │ , Header
        │     1
        │     ( "book__markdown__src__chapter.md__hello-1"
        │     , [ "unnumbered" , "unlisted" ]
        │     , []
        │     )
        │     [ Str "Hello" ]
        │ , Para
        │     [ Link
        │         ( "" , [] , [] )
        │         [ Str "first" ]
        │         ( "#book__markdown__src__chapter.md__hello" , "" )
        │     , Link
        │         ( "" , [] , [] )
        │         [ Str "second" ]
        │         ( "#book__markdown__src__chapter.md__hello-1" , "" )
        │     ]
        │ , Header
        │     1
        │     ( "book__markdown__src__chapter2.md__hello" , [] , [] )
        │     [ Str "Hello" ]
        │ , Header
        │     1
        │     ( "book__markdown__src__chapter2.md__hello-1"
        │     , [ "unnumbered" , "unlisted" ]
        │     , []
        │     )
        │     [ Str "Hello" ]
        │ , Para
        │     [ Link
        │         ( "" , [] , [] )
        │         [ Str "first" ]
        │         ( "#book__markdown__src__chapter2.md__hello" , "" )
        │     , Link
        │         ( "" , [] , [] )
        │         [ Str "second" ]
        │         ( "#book__markdown__src__chapter2.md__hello-1" , "" )
        │     ]
        │ , Header 1 ( "" , [] , [] ) [ Str "?" ]
        │ , Header
        │     1
        │     ( "book__markdown__src__weird-ids.md__-1"
        │     , [ "unnumbered" , "unlisted" ]
        │     , []
        │     )
        │     [ Str "?" ]
        │ , Para
        │     [ Link
        │         ( "" , [] , [] )
        │         [ Str "second" ]
        │         ( "#book__markdown__src__weird-ids.md__-1" , "" )
        │     ]
        │ ]
        "##);
    }

    #[test]
    fn pandoc_working_dir_is_root() {
        let cfg = r#"
[output.pandoc.profile.foo]
output-file = "foo.md"
include-in-header = ["file-in-root"]
        "#;
        let book = MDBook::init()
            .mdbook_config(cfg.parse().unwrap())
            .file_in_root("file-in-root", "some text")
            .build();
        insta::assert_snapshot!(book, @r"
        ├─ log output
        │  INFO mdbook::book: Running the pandoc backend    
        │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
        │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/foo/foo.md    
        ├─ foo/foo.md
        │ some text
        ");
    }

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
}
