use std::{
    borrow::Cow,
    cmp,
    collections::{hash_map::DefaultHasher, HashMap},
    ffi::OsString,
    fmt::{self, Write as _},
    fs::{self, File},
    hash::{Hash, Hasher},
    io::{self, Write as _},
    iter::{self, Peekable},
    path::{Path, PathBuf},
};

use aho_corasick::AhoCorasick;
use anyhow::{anyhow, Context as _};
use mdbook::{
    book::{BookItems, Chapter},
    BookItem,
};
use once_cell::sync::Lazy;
use pulldown_cmark::{CodeBlockKind, CowStr, HeadingLevel, LinkType};
use regex::Regex;
use walkdir::WalkDir;

use crate::{
    latex,
    pandoc::{self, OutputFormat, RenderContext},
};

pub struct Preprocessor<'book> {
    ctx: RenderContext<'book>,
    preprocessed: PathBuf,
    preprocessed_relative_to_root: PathBuf,
    redirects: HashMap<PathBuf, String>,
    hosted_html: Option<&'book str>,
    unresolved_links: bool,
}

pub struct Preprocess<'book> {
    preprocessor: Preprocessor<'book>,
    items: BookItems<'book>,
    part_num: usize,
}

#[derive(Debug)]
struct NormalizedPath {
    src_absolute_path: PathBuf,
    preprocessed_absolute_path: PathBuf,
    preprocessed_path_relative_to_root: PathBuf,
}

#[derive(Copy, Clone)]
enum LinkContext {
    Link,
    Image,
}

impl<'book> Preprocessor<'book> {
    pub fn new(ctx: RenderContext<'book>) -> anyhow::Result<Self> {
        let preprocessed = ctx.destination.join("src");

        if preprocessed.try_exists()? {
            fs::remove_dir_all(&preprocessed)?;
        }
        fs::create_dir_all(&preprocessed)?;

        for entry in WalkDir::new(&ctx.book.source_dir).follow_links(true) {
            let entry = entry?;
            let src = entry.path();
            let dest = preprocessed.join(src.strip_prefix(&ctx.book.source_dir).unwrap());
            if entry.file_type().is_dir() {
                fs::create_dir_all(&dest)
                    .with_context(|| format!("Unable to create directory '{}'", dest.display()))?
            } else {
                fs::copy(src, &dest).with_context(|| {
                    format!("Unable to copy '{}' -> '{}'", src.display(), dest.display())
                })?;
            }
        }

        Ok(Self {
            preprocessed_relative_to_root: preprocessed
                .strip_prefix(&ctx.book.root)
                .unwrap_or(&preprocessed)
                .to_path_buf(),
            preprocessed,
            redirects: Default::default(),
            hosted_html: Default::default(),
            unresolved_links: false,
            ctx,
        })
    }

    /// Processes redirect entries in the [output.html.redirect] table
    pub fn add_redirects<'iter>(
        &mut self,
        redirects: impl IntoIterator<Item = (&'iter str, &'iter str)>,
    ) {
        redirects
            .into_iter()
            .map(|entry @ (src, dst)| {
                log::debug!("Processing redirect: {src} => {dst}");

                let res = (|| {
                    let src_rel_path = src.trim_start_matches('/');
                    let src = self.preprocessed.join(src_rel_path);

                    let Some(parent) = src.parent() else {
                        anyhow::bail!(
                            "Redirect source has no parent directory: '{}'",
                            src.display()
                        )
                    };

                    fs::create_dir_all(parent).with_context(|| {
                        format!("Unable to create directory '{}'", parent.display())
                    })?;

                    File::create(&src)
                        .with_context(|| format!("Unable to create file '{}'", src.display()))?;

                    Ok((src, dst))
                })();
                (res, entry)
            })
            // Create all redirect sources before resolving destinations
            // because a redirect may reference other redirects
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(res, entry)| {
                res.and_then(|(src, dst)| {
                    let dst = self
                        .normalize_link(
                            &src,
                            src.parent().unwrap(),
                            LinkType::Autolink,
                            dst.into(),
                            LinkContext::Link,
                        )
                        .map_err(|(err, _)| err)
                        .context("Unable to normalize redirect destination")?;
                    let src = self
                        .normalize_path(&src)
                        .context("Unable to normalize redirect source")?
                        .preprocessed_path_relative_to_root;

                    log::debug!("Registered redirect: {} => {dst}", src.display());
                    self.redirects.insert(src, dst.into_string());
                    Ok(())
                })
                .map_err(|err| (err, entry))
            })
            .filter_map(Result::err)
            .for_each(|(err, (src, dst))| {
                log::warn!("Failed to resolve redirect: {src} => {dst}: {err:#}")
            })
    }

    pub fn hosted_html(&mut self, uri: &'book str) {
        self.hosted_html = Some(uri);
    }

    pub fn preprocess(self) -> Preprocess<'book> {
        Preprocess {
            items: self.ctx.book.book.iter(),
            preprocessor: self,
            part_num: 0,
        }
    }

    fn normalize_link_or_leave_as_is<'link>(
        &mut self,
        chapter: &Chapter,
        link_type: LinkType,
        link: CowStr<'link>,
        ctx: LinkContext,
    ) -> CowStr<'link> {
        let Some(chapter_path) = &chapter.path else {
            return link;
        };
        let chapter_dir = chapter_path.parent().unwrap();
        self.normalize_link(chapter_path, chapter_dir, link_type, link, ctx)
            .unwrap_or_else(|(err, link)| {
                log::warn!(
                    "Unable to normalize link '{}' in chapter '{}': {err:#}",
                    link,
                    chapter.name,
                );
                link
            })
    }

    fn normalize_link<'link>(
        &mut self,
        chapter_path: &Path,
        chapter_dir: &Path,
        link_type: LinkType,
        link: CowStr<'link>,
        ctx: LinkContext,
    ) -> Result<CowStr<'link>, (anyhow::Error, CowStr<'link>)> {
        use LinkType::*;
        match link_type {
            // Don't try to normalize emails
            Email => return Ok(link),
            Inline | Reference | ReferenceUnknown | Collapsed | CollapsedUnknown | Shortcut
            | ShortcutUnknown | Autolink => {}
        }

        // URI scheme definition: https://datatracker.ietf.org/doc/html/rfc3986#section-3.1
        static SCHEME: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"^(?P<scheme>[a-zA-Z][a-z0-9+.-]*):").unwrap());

        let os_to_utf8 = |os: OsString| {
            os.into_string()
                .map_err(|path| anyhow!("Path is not valid UTF8: {path:?}"))
        };

        let link_path_range = || ..link.find(['?', '#']).unwrap_or(link.len());

        if let Some(scheme) = SCHEME.captures(&link).and_then(|caps| caps.name("scheme")) {
            match (ctx, scheme.as_str()) {
                (LinkContext::Image, "http" | "https") => {
                    /// Pandoc usually downloads remote images and embeds them in documents, but it
                    /// doesn't handle some cases--we special case those here.
                    const PANDOC_UNSUPPORTED_IMAGE_EXTENSIONS: &[&str] = &[
                        // e.g. https://img.shields.io/github/actions/workflow/status/rust-lang/mdBook/main.yml?style=flat-square
                        ".yml",
                    ];

                    let path = &link[link_path_range()];
                    if PANDOC_UNSUPPORTED_IMAGE_EXTENSIONS
                        .iter()
                        .any(|extension| path.ends_with(extension))
                    {
                        self.download_remote_image(&link)
                            .and_then(|path| os_to_utf8(path.into_os_string()).map(CowStr::from))
                            .map_err(|err| (err, link))
                    } else {
                        Ok(link)
                    }
                }
                // Leave all other URIs with schemes untouched
                _ => Ok(link),
            }
        } else {
            // URI is a relative-reference: https://datatracker.ietf.org/doc/html/rfc3986#section-4.2
            if link.starts_with('/') {
                // URI is a network-path reference or absolute-path reference;
                // leave both untouched
                Ok(link)
            } else {
                // URI is a relative-path reference, which must be normalized
                let path_range = link_path_range();
                let link_path = match &link[path_range] {
                    // Internal reference within chapter
                    "" if link.starts_with('#') => return Ok(link),
                    path => Path::new(path),
                };
                let path = chapter_dir.join(link_path);

                let normalized = self
                    .normalize_path(&self.ctx.book.source_dir.join(&path))
                    .or_else(|err| {
                        self.normalize_path(&self.preprocessed.join(&path))
                            .map_err(|_| err)
                    })
                    .and_then(|normalized| {
                        if let Some(mut path) = self
                            .redirects
                            .get(&normalized.preprocessed_path_relative_to_root)
                        {
                            while let Some(dest) = self.redirects.get(Path::new(path)) {
                                path = dest;
                            }
                            Ok(Cow::Borrowed(path))
                        } else {
                            if !normalized.exists()? {
                                normalized.copy_to_preprocessed()?;
                            }
                            os_to_utf8(
                                normalized
                                    .preprocessed_path_relative_to_root
                                    .into_os_string(),
                            )
                            .map(Cow::Owned)
                        }
                    })
                    .or_else(|err| {
                        self.hosted_html
                            .ok_or(err)
                            .and_then(|uri| {
                                let mut hosted = OsString::from(uri.trim_end_matches('/'));
                                hosted.push("/");
                                hosted.push(&path);
                                let hosted = os_to_utf8(hosted)?;
                                log::debug!(
                                    "Unable to resolve relative path '{}' in chapter '{}', \
                                    linking to hosted HTML book at '{hosted}'",
                                    link_path.display(),
                                    chapter_path.display(),
                                );
                                Ok(hosted)
                            })
                            .map(Cow::Owned)
                    });
                match normalized {
                    Ok(normalized_relative_path) => {
                        let mut link = link.into_string();
                        link.replace_range(path_range, &normalized_relative_path);
                        Ok(link.into())
                    }
                    Err(err) => {
                        self.unresolved_links = true;
                        Err((err, link))
                    }
                }
            }
        }
    }

    fn download_remote_image(&self, link: &str) -> anyhow::Result<PathBuf> {
        match ureq::get(link).call() {
            Err(err) => anyhow::bail!("Unable to load remote image '{link}': {err:#}"),
            Ok(response) => {
                const IMAGE_CONTENT_TYPES: &[(&str, &str)] = &[("image/svg+xml", "svg")];
                let extension = IMAGE_CONTENT_TYPES.iter().find_map(|&(ty, extension)| {
                    (ty == response.content_type()).then_some(extension)
                });
                match extension {
                    None => anyhow::bail!("Unrecognized content-type: {}", response.content_type()),
                    Some(extension) => {
                        let mut filename = PathBuf::from(Self::make_kebab_case(link));
                        filename.set_extension(extension);
                        let path = self.preprocessed.join(filename);

                        File::create(&path)
                            .and_then(|file| {
                                io::copy(&mut response.into_reader(), &mut io::BufWriter::new(file))
                            })
                            .with_context(|| {
                                format!(
                                    "Unable to write downloaded image from '{}' to file '{}'",
                                    link,
                                    path.display(),
                                )
                            })
                            .map(|_| path)
                    }
                }
            }
        }
    }

    /// Converts an absolute path to a normalized form usable as a relative path within the preprocessed source directory.
    ///
    /// The normalized form:
    /// - Is a relative path
    /// - Does not traverse parent directories
    /// - Uniquely corresponds to the file at the original path
    fn normalize_path(&self, path: &Path) -> anyhow::Result<NormalizedPath> {
        let absolute_path = path
            .canonicalize()
            .or_else(|err| match path.extension() {
                Some(extension) if extension == "html" => {
                    if path.file_stem().is_some_and(|name| name == "index") {
                        if let Ok(path) = path.with_file_name("README.md").canonicalize() {
                            return Ok(path);
                        }
                    }
                    path.with_extension("md").canonicalize()
                }
                Some(extension) if extension == "md" => path.with_extension("html").canonicalize(),
                _ => Err(err),
            })
            .with_context(|| format!("Unable to canonicalize path: {}", path.display()))?;
        let preprocessed_relative_path = absolute_path
            .strip_prefix(&self.ctx.book.source_dir)
            .or_else(|_| absolute_path.strip_prefix(&self.preprocessed))
            .map(|path| path.to_path_buf())
            .unwrap_or_else(|_| {
                let mut hasher = DefaultHasher::new();
                absolute_path.hash(&mut hasher);
                let hash = hasher.finish();
                let mut name = PathBuf::from(format!("{hash:x}"));
                if let Some(extension) = absolute_path.extension() {
                    name.set_extension(extension);
                }
                name
            });

        Ok(NormalizedPath {
            src_absolute_path: absolute_path,
            preprocessed_absolute_path: self.preprocessed.join(&preprocessed_relative_path),
            preprocessed_path_relative_to_root: self
                .preprocessed_relative_to_root
                .join(&preprocessed_relative_path),
        })
    }

    fn make_kebab_case(s: &str) -> String {
        const SEPARATORS: &[char] = &['_', '/', '.', '&', '?', '='];
        s
            // Replace separator-like characters with hyphens
            .replace(|c: char| c.is_whitespace() || SEPARATORS.contains(&c), "-")
            // Strip non alphanumeric/hyphen characters
            .replace(|c: char| !(c.is_ascii_alphanumeric() || c == '-'), "")
    }
}

impl Iterator for Preprocess<'_> {
    type Item = anyhow::Result<PathBuf>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.items.next()?;
            if let Some(res) = self.preprocess_book_item(item).transpose() {
                break Some(res);
            }
        }
    }
}

impl<'book> Preprocess<'book> {
    fn preprocess_book_item(&mut self, item: &'book BookItem) -> anyhow::Result<Option<PathBuf>> {
        match item {
            BookItem::Chapter(chapter) => {
                let Some(chapter_path) = &chapter.source_path else {
                    return Ok(None);
                };
                let chapter_path = self.preprocessor.ctx.book.source_dir.join(chapter_path);
                let normalized = self.preprocessor.normalize_path(&chapter_path)?;
                let writer = io::BufWriter::new(normalized.create()?);
                self.preprocess_chapter(chapter, writer)?;
                Ok(Some(normalized.preprocessed_path_relative_to_root))
            }
            BookItem::Separator => {
                log::debug!("Ignoring separator");
                Ok(None)
            }
            BookItem::PartTitle(name) => match self.preprocessor.ctx.output {
                OutputFormat::Latex { .. }
                    if (self.preprocessor.ctx.pandoc)
                        .enable_extension(pandoc::Extension::RawAttribute)
                        .is_available() =>
                {
                    self.part_num += 1;
                    let kebab_case_name = Preprocessor::make_kebab_case(name);
                    let path =
                        PathBuf::from(format!("part-{}-{kebab_case_name}.md", self.part_num));
                    let mut file = File::options()
                        .write(true)
                        .create_new(true)
                        .open(self.preprocessor.preprocessed.join(&path))
                        .with_context(|| format!("Unable to create file for part '{name}'"))?;
                    writeln!(file, r"`\part{{{name}}}`{{=latex}}")?;
                    Ok(Some(
                        self.preprocessor.preprocessed_relative_to_root.join(path),
                    ))
                }
                _ => {
                    log::warn!("Ignoring part separator: {}", name);
                    Ok(None)
                }
            },
        }
    }

    fn preprocess_chapter(
        &mut self,
        chapter: &'book Chapter,
        mut out: impl io::Write,
    ) -> anyhow::Result<()> {
        if chapter.number.is_none() && self.part_num > 0 {
            match self.preprocessor.ctx.output {
                OutputFormat::Latex { .. }
                    if (self.preprocessor.ctx.pandoc)
                        .enable_extension(pandoc::Extension::RawAttribute)
                        .is_available() =>
                {
                    writeln!(out, r"`\bookmarksetup{{startatroot}}`{{=latex}}")?;
                }
                _ => {}
            }
        }

        let preprocessed = PreprocessChapter::new(&mut self.preprocessor, chapter);
        genawaiter::stack::let_gen_using!(preprocessed, |co| preprocessed.preprocess(co));

        struct IoWriteAdapter<W>(W);
        impl<W: io::Write> fmt::Write for IoWriteAdapter<W> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0.write_all(s.as_bytes()).map_err(|_| fmt::Error)
            }
        }

        pulldown_cmark_to_cmark::cmark(preprocessed.into_iter(), IoWriteAdapter(out))
            .context("Failed to write preprocessed chapter")?;
        Ok(())
    }

    pub fn render_context(&mut self) -> &mut RenderContext<'book> {
        &mut self.preprocessor.ctx
    }

    pub fn output_dir(&self) -> &Path {
        &self.preprocessor.preprocessed
    }

    pub fn unresolved_links(&self) -> bool {
        self.preprocessor.unresolved_links
    }
}

struct PreprocessChapter<'book, 'preprocessor> {
    preprocessor: &'preprocessor mut Preprocessor<'book>,
    chapter: &'book Chapter,
    parser: Peekable<pulldown_cmark::OffsetIter<'book, pulldown_cmark::DefaultBrokenLinkCallback>>,
    matching_tags: Vec<pulldown_cmark::TagEnd>,
    encountered_h1: bool,
}

impl<'book, 'preprocessor> PreprocessChapter<'book, 'preprocessor> {
    fn new(preprocessor: &'preprocessor mut Preprocessor<'book>, chapter: &'book Chapter) -> Self {
        /// Markdown extensions supported by mdBook
        ///
        /// See https://rust-lang.github.io/mdBook/format/markdown.html#extensions
        const PARSER_OPTIONS: pulldown_cmark::Options = {
            use pulldown_cmark::Options;
            Options::empty()
                .union(Options::ENABLE_STRIKETHROUGH)
                .union(Options::ENABLE_FOOTNOTES)
                .union(Options::ENABLE_TABLES)
                .union(Options::ENABLE_TASKLISTS)
                .union(Options::ENABLE_HEADING_ATTRIBUTES)
        };

        Self {
            preprocessor,
            chapter,
            parser: pulldown_cmark::Parser::new_ext(&chapter.content, PARSER_OPTIONS)
                .into_offset_iter()
                .peekable(),
            matching_tags: Default::default(),
            encountered_h1: false,
        }
    }

    fn update_heading<'b>(
        &mut self,
        level: HeadingLevel,
        mut classes: Vec<CowStr<'b>>,
    ) -> Option<(HeadingLevel, Vec<CowStr<'b>>)> {
        const PANDOC_UNNUMBERED_CLASS: &str = "unnumbered";
        const PANDOC_UNLISTED_CLASS: &str = "unlisted";

        if (self.preprocessor.ctx.pandoc)
            .enable_extension(pandoc::Extension::Attributes)
            .is_available()
        {
            if let HeadingLevel::H1 = level {
                // Number the first H1 in each numbered chapter, mirroring mdBook
                if self.encountered_h1 {
                    classes.push(PANDOC_UNNUMBERED_CLASS.into());
                    classes.push(PANDOC_UNLISTED_CLASS.into());
                } else if self.chapter.number.is_none() {
                    classes.push(PANDOC_UNNUMBERED_CLASS.into());
                }
                self.encountered_h1 = true;
            } else {
                classes.push(PANDOC_UNNUMBERED_CLASS.into());
                classes.push(PANDOC_UNLISTED_CLASS.into());
            }
        }

        let shift_smaller = |level| {
            use HeadingLevel::*;
            match level {
                H1 => Some(H2),
                H2 => Some(H3),
                H3 => Some(H4),
                H4 => Some(H5),
                H5 => Some(H6),
                H6 => None,
            }
        };
        let Some(level) = iter::successors(Some(level), |level| shift_smaller(*level))
            .nth(self.chapter.parent_names.len())
        else {
            log::warn!(
                "Heading (level {level}) converted to paragraph in chapter: {}",
                self.chapter.name
            );
            return None;
        };
        Some((level, classes))
    }

    fn column_width_annotation(&self, table: &str) -> Option<String> {
        let mut wide = false;
        let mut rows = table.lines().inspect(|line| {
            if line.len() > self.preprocessor.ctx.columns {
                wide = true;
            }
        });
        // The second row of a table is the delimiter row
        // See: https://github.github.com/gfm/#tables-extension-
        let delimiter_row = rows.nth(1).expect("table did not contain a delimiter row");
        let mut column_widths = delimiter_row
            // Cells are separated by pipes
            .split('|')
            .map(|cell| cell.chars().filter(char::is_ascii_punctuation).count())
            .filter(|&width| width > 0);
        // Consume iterator to finish checking for long rows
        rows.for_each(|_| ());
        wide.then(|| {
            let mut annotation = String::from("<!-- mdbook-pandoc::table: ");
            if let Some(width) = column_widths.next() {
                write!(annotation, "{width}").unwrap();
            }
            for width in column_widths {
                write!(annotation, "|{width}").unwrap();
            }
            write!(annotation, " -->").unwrap();
            annotation
        })
    }

    async fn preprocess(mut self, co: genawaiter::stack::Co<'_, pulldown_cmark::Event<'book>>) {
        use pulldown_cmark::{Event, Tag, TagEnd};

        while let Some((event, range)) = self.parser.next() {
            let event = match event {
                Event::Start(tag) => 'current_event: {
                    let tag = match tag {
                        Tag::List(start_number) => {
                            self.preprocessor.ctx.cur_list_depth += 1;
                            self.preprocessor.ctx.max_list_depth = cmp::max(
                                self.preprocessor.ctx.max_list_depth,
                                self.preprocessor.ctx.cur_list_depth,
                            );
                            Tag::List(start_number)
                        }
                        Tag::Strikethrough => {
                            // TODO: pandoc requires ~~, but commonmark's extension allows ~ or ~~.
                            // pulldown_cmark_to_cmark always generates ~~, so this is okay,
                            // although it'd be good to have an option to configure this explicitly.
                            (self.preprocessor.ctx.pandoc)
                                .enable_extension(pandoc::Extension::Strikeout);
                            Tag::Strikethrough
                        }
                        Tag::FootnoteDefinition(label) => {
                            (self.preprocessor.ctx.pandoc)
                                .enable_extension(pandoc::Extension::Footnotes);
                            Tag::FootnoteDefinition(label)
                        }
                        Tag::Table(alignment) => {
                            (self.preprocessor.ctx.pandoc)
                                .enable_extension(pandoc::Extension::PipeTables);
                            if let Some(annotation) =
                                self.column_width_annotation(&self.chapter.content[range])
                            {
                                co.yield_(Event::Start(Tag::HtmlBlock)).await;
                                co.yield_(Event::Html(annotation.into())).await;
                                co.yield_(Event::End(TagEnd::HtmlBlock)).await;
                            }
                            Tag::Table(alignment)
                        }
                        Tag::Heading {
                            level,
                            id,
                            classes,
                            attrs,
                        } => self
                            .update_heading(level, classes)
                            .map(|(level, classes)| {
                                if id.is_some() || !classes.is_empty() {
                                    // pandoc does not support `header_attributes` with commonmark
                                    // so use `attributes`, which is a superset
                                    (self.preprocessor.ctx.pandoc)
                                        .enable_extension(pandoc::Extension::Attributes);
                                }
                                Tag::Heading {
                                    level,
                                    id,
                                    classes,
                                    attrs,
                                }
                            })
                            .unwrap_or(Tag::Paragraph),
                        Tag::Link {
                            link_type,
                            dest_url,
                            title,
                            id,
                        } => {
                            let dest_url = self.preprocessor.normalize_link_or_leave_as_is(
                                self.chapter,
                                link_type,
                                dest_url,
                                LinkContext::Link,
                            );
                            Tag::Link {
                                link_type,
                                dest_url,
                                title,
                                id,
                            }
                        }
                        Tag::Image {
                            link_type,
                            dest_url,
                            title,
                            id,
                        } => {
                            let dest_url = self.preprocessor.normalize_link_or_leave_as_is(
                                self.chapter,
                                link_type,
                                dest_url,
                                LinkContext::Image,
                            );
                            Tag::Image {
                                link_type,
                                dest_url,
                                title,
                                id,
                            }
                        }
                        Tag::CodeBlock(CodeBlockKind::Fenced(mut info_string)) => {
                            // MdBook supports custom attributes on Rust code blocks.
                            // Attributes are separated by a comma, space, or tab from the 'rust' prefix.
                            // See https://rust-lang.github.io/mdBook/format/mdbook.html#rust-code-block-attributes
                            // This strips out the attributes.
                            static MDBOOK_ATTRIBUTES: Lazy<Regex> =
                                Lazy::new(|| Regex::new(r"^rust[, \t].*").unwrap());
                            if let Cow::Owned(info) =
                                MDBOOK_ATTRIBUTES.replace(&info_string, "rust")
                            {
                                info_string = info.into();
                            };

                            // Pandoc+fvextra only wraps long lines in code blocks with info strings
                            if info_string.is_empty() {
                                info_string = "text".into();
                            }

                            let code_block = Tag::CodeBlock(CodeBlockKind::Fenced(info_string));

                            if let OutputFormat::Latex { .. } = self.preprocessor.ctx.output {
                                const CODE_BLOCK_LINE_LENGTH_LIMIT: usize = 1000;

                                let mut texts = vec![];
                                let mut overly_long_line = false;
                                for (event, _) in &mut self.parser {
                                    match event {
                                        Event::Text(text) => {
                                            if text.lines().any(|line| {
                                                line.len() > CODE_BLOCK_LINE_LENGTH_LIMIT
                                            }) {
                                                overly_long_line = true;
                                            }
                                            texts.push(text)
                                        }
                                        Event::End(TagEnd::CodeBlock) => break,
                                        event => {
                                            co.yield_(Event::Start(code_block)).await;
                                            for text in texts {
                                                co.yield_(Event::Text(text)).await;
                                            }
                                            break 'current_event event;
                                        }
                                    }
                                }

                                if overly_long_line {
                                    (self.preprocessor.ctx.pandoc)
                                        .enable_extension(pandoc::Extension::RawAttribute);
                                    let raw_latex =
                                        Tag::CodeBlock(CodeBlockKind::Fenced("{=latex}".into()));
                                    let raw_latex_end = raw_latex.to_end();
                                    let lines = {
                                        let patterns = &[r"\", "{", "}", "$", "_", "^", "&", "]"];
                                        let replace_with = &[
                                            r"\textbackslash{}",
                                            r"\{",
                                            r"\}",
                                            r"\$",
                                            r"\_",
                                            r"\^",
                                            r"\&",
                                            r"{{]}}",
                                        ];
                                        let ac = AhoCorasick::new(patterns).unwrap();
                                        texts.iter().flat_map(|text| text.lines()).map(
                                            move |text| {
                                                let text = ac.replace_all(text, replace_with);
                                                Event::Text(format!(r"\texttt{{{text}}}\\").into())
                                            },
                                        )
                                    };
                                    for event in iter::once(Event::Start(raw_latex)).chain(lines) {
                                        co.yield_(event).await
                                    }
                                    break 'current_event Event::End(raw_latex_end);
                                } else {
                                    let end_tag = code_block.to_end();
                                    for event in iter::once(Event::Start(code_block))
                                        .chain(texts.into_iter().map(Event::Text))
                                    {
                                        co.yield_(event).await;
                                    }
                                    break 'current_event Event::End(end_tag);
                                }
                            }

                            code_block
                        }
                        tag => tag,
                    };
                    self.matching_tags.push(tag.to_end());
                    Event::Start(tag)
                }
                Event::End(_) => {
                    let end = self.matching_tags.pop().unwrap();
                    if let TagEnd::List(_) = &end {
                        self.preprocessor.ctx.cur_list_depth -= 1;
                    }
                    Event::End(end)
                }
                Event::Html(mut html) => {
                    while let Some((Event::Html(more), _)) = self.parser.peek() {
                        let mut string = html.into_string();
                        string.push_str(more);
                        html = string.into();
                        // Actually consume the item from the iterator
                        self.parser.next();
                    }
                    html = self.preprocess_contiguous_html(html);
                    Event::Html(html)
                }
                Event::InlineHtml(mut html) => {
                    while let Some((Event::InlineHtml(more), _)) = self.parser.peek() {
                        let mut string = html.into_string();
                        string.push_str(more);
                        html = string.into();
                        // Actually consume the item from the iterator
                        self.parser.next();
                    }
                    html = self.preprocess_contiguous_html(html);
                    Event::InlineHtml(html)
                }
                Event::TaskListMarker(checked) => {
                    (self.preprocessor.ctx.pandoc).enable_extension(pandoc::Extension::TaskLists);
                    Event::TaskListMarker(checked)
                }
                event => event,
            };
            co.yield_(event).await;
        }
    }

    fn preprocess_contiguous_html(&mut self, mut html: CowStr<'book>) -> CowStr<'book> {
        if let OutputFormat::Latex { packages } = &mut self.preprocessor.ctx.output {
            static FONT_AWESOME_ICON: Lazy<Regex> = Lazy::new(|| {
                Regex::new(r#"<i\s+class\s*=\s*"fa fa-(?P<icon>.*?)"(>\s*</i>|/>)"#).unwrap()
            });
            if (self.preprocessor.ctx.pandoc)
                .enable_extension(pandoc::Extension::RawAttribute)
                .is_available()
            {
                html = match FONT_AWESOME_ICON.replace_all(&html, r"`\faicon{$icon}`{=latex}") {
                    Cow::Borrowed(_) => html,
                    Cow::Owned(html) => {
                        packages.need(latex::Package::FontAwesome);
                        html.into()
                    }
                };
            }
        }
        html
    }
}

impl NormalizedPath {
    fn copy_to_preprocessed(&self) -> anyhow::Result<()> {
        let path = &self.preprocessed_absolute_path;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Unable to create directory: {}", parent.display()))?;
        }
        fs::copy(&self.src_absolute_path, path).with_context(|| {
            format!(
                "Unable to copy file from {} to {}",
                self.src_absolute_path.display(),
                path.display(),
            )
        })?;
        Ok(())
    }

    fn exists(&self) -> io::Result<bool> {
        self.preprocessed_absolute_path.try_exists()
    }

    fn create(&self) -> anyhow::Result<File> {
        let path = &self.preprocessed_absolute_path;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Unable to create directory: {}", parent.display()))?;
        }
        File::create(path).with_context(|| format!("Unable to create file: {}", path.display()))
    }
}
