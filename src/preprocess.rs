use std::{
    borrow::Cow,
    cmp,
    collections::{hash_map::DefaultHasher, HashMap, VecDeque},
    fmt,
    fs::{self, File},
    hash::{Hash, Hasher},
    io::{self, Write},
    iter::{self, Peekable},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{anyhow, Context as _};
use itertools::Itertools;
use mdbook::{
    book::{BookItems, Chapter},
    BookItem,
};
use once_cell::sync::Lazy;
use pulldown_cmark::{CodeBlockKind, CowStr, HeadingLevel};
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
                        .normalize_link(src.parent().unwrap(), dst.into(), LinkContext::Link)
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

    pub fn preprocess(self) -> Preprocess<'book> {
        Preprocess {
            items: self.ctx.book.book.iter(),
            preprocessor: self,
            part_num: 0,
        }
    }

    fn preprocess_chapter(
        &mut self,
        chapter: &'book Chapter,
        out: impl io::Write,
    ) -> anyhow::Result<()> {
        let preprocessed = PreprocessChapter::new(self, chapter);
        struct IoWriteAdapter<W>(W);
        impl<W: io::Write> fmt::Write for IoWriteAdapter<W> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0.write_all(s.as_bytes()).map_err(|_| fmt::Error)
            }
        }
        pulldown_cmark_to_cmark::cmark(preprocessed, IoWriteAdapter(out))
            .context("Failed to write preprocessed chapter")?;
        Ok(())
    }

    fn normalize_link_or_leave_as_is<'link>(
        &self,
        chapter: &Chapter,
        link: CowStr<'link>,
        ctx: LinkContext,
    ) -> CowStr<'link> {
        let Some(chapter_path) = &chapter.path else {
            return link;
        };
        let chapter_dir = chapter_path.parent().unwrap();
        self.normalize_link(chapter_dir, link, ctx)
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
        &self,
        chapter_dir: &Path,
        link: CowStr<'link>,
        ctx: LinkContext,
    ) -> Result<CowStr<'link>, (anyhow::Error, CowStr<'link>)> {
        // URI scheme definition: https://datatracker.ietf.org/doc/html/rfc3986#section-3.1
        static SCHEME: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"^(?P<scheme>[a-zA-Z][a-z0-9+.-]*):").unwrap());

        let pathbuf_to_utf8 = |path: PathBuf| {
            path.into_os_string()
                .into_string()
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
                            .and_then(|path| pathbuf_to_utf8(path).map(CowStr::from))
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
                let path = match &link[path_range] {
                    // Internal reference within chapter
                    "" if link.starts_with('#') => return Ok(link),
                    path => Path::new(path),
                };
                let path = chapter_dir.join(path);

                let normalized = self
                    .normalize_path(&self.ctx.book.source_dir.join(&path))
                    .or_else(|err| {
                        self.normalize_path(&self.preprocessed.join(path))
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
                            pathbuf_to_utf8(normalized.preprocessed_path_relative_to_root)
                                .map(Cow::Owned)
                        }
                    });
                match normalized {
                    Ok(normalized_relative_path) => {
                        let mut link = link.into_string();
                        link.replace_range(path_range, &normalized_relative_path);
                        Ok(link.into())
                    }
                    Err(err) => Err((err, link)),
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

    fn update_heading<'b>(
        &mut self,
        chapter: &Chapter,
        level: HeadingLevel,
        id: Option<&'b str>,
        mut classes: Vec<&'b str>,
    ) -> Option<(HeadingLevel, Option<&'b str>, Vec<&'b str>)> {
        const PANDOC_UNNUMBERED_CLASS: &str = "unnumbered";
        const PANDOC_UNLISTED_CLASS: &str = "unlisted";

        if level != HeadingLevel::H1
            && (self.ctx.pandoc)
                .enable_extension(pandoc::Extension::Attributes)
                .is_available()
        {
            classes.push(PANDOC_UNNUMBERED_CLASS);
            classes.push(PANDOC_UNLISTED_CLASS);
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
            .nth(chapter.parent_names.len())
        else {
            log::warn!(
                "Heading (level {level}) converted to paragraph in chapter: {}",
                chapter.name
            );
            return None;
        };
        Some((level, id, classes))
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
                self.preprocessor.preprocess_chapter(chapter, writer)?;
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

    pub fn render_context(&mut self) -> &mut RenderContext<'book> {
        &mut self.preprocessor.ctx
    }

    pub fn output_dir(&self) -> &Path {
        &self.preprocessor.preprocessed
    }
}

struct PreprocessChapter<'book, 'preprocessor> {
    preprocessor: &'preprocessor mut Preprocessor<'book>,
    chapter: &'book Chapter,
    parser: Peekable<pulldown_cmark::Parser<'book, 'book>>,
    current_block: VecDeque<pulldown_cmark::Event<'book>>,
    start_tags: Vec<pulldown_cmark::Tag<'book>>,
}

impl<'book, 'preprocessor> PreprocessChapter<'book, 'preprocessor> {
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

    fn new(preprocessor: &'preprocessor mut Preprocessor<'book>, chapter: &'book Chapter) -> Self {
        Self {
            preprocessor,
            chapter,
            parser: pulldown_cmark::Parser::new_ext(&chapter.content, Self::PARSER_OPTIONS)
                .peekable(),
            current_block: Default::default(),
            start_tags: Default::default(),
        }
    }
}

impl<'book> Iterator for PreprocessChapter<'book, '_> {
    type Item = pulldown_cmark::Event<'book>;

    fn next(&mut self) -> Option<Self::Item> {
        use pulldown_cmark::{Event, Tag};

        let preprocess_non_html_event = |this: &mut Self, event| match event {
            Event::Start(tag) => {
                let tag = match tag {
                    Tag::List(start_number) => {
                        this.preprocessor.ctx.cur_list_depth += 1;
                        this.preprocessor.ctx.max_list_depth = cmp::max(
                            this.preprocessor.ctx.max_list_depth,
                            this.preprocessor.ctx.cur_list_depth,
                        );
                        Tag::List(start_number)
                    }
                    Tag::Strikethrough => {
                        // TODO: pandoc requires ~~, but commonmark's extension allows ~ or ~~.
                        // pulldown_cmark_to_cmark always generates ~~, so this is okay,
                        // although it'd be good to have an option to configure this explicitly.
                        (this.preprocessor.ctx.pandoc)
                            .enable_extension(pandoc::Extension::Strikeout);
                        Tag::Strikethrough
                    }
                    Tag::FootnoteDefinition(label) => {
                        (this.preprocessor.ctx.pandoc)
                            .enable_extension(pandoc::Extension::Footnotes);
                        Tag::FootnoteDefinition(label)
                    }
                    Tag::Table(alignment) => {
                        (this.preprocessor.ctx.pandoc)
                            .enable_extension(pandoc::Extension::PipeTables);
                        Tag::Table(alignment)
                    }
                    Tag::Heading(level, id, classes) => this
                        .preprocessor
                        .update_heading(this.chapter, level, id, classes)
                        .map(|(level, id, classes)| {
                            if id.is_some() || !classes.is_empty() {
                                // pandoc does not support `header_attributes` with commonmark
                                // so use `attributes`, which is a superset
                                (this.preprocessor.ctx.pandoc)
                                    .enable_extension(pandoc::Extension::Attributes);
                            }
                            Tag::Heading(level, id, classes)
                        })
                        .unwrap_or(Tag::Paragraph),
                    Tag::Link(link_ty, destination, title) => {
                        let destination = this.preprocessor.normalize_link_or_leave_as_is(
                            this.chapter,
                            destination,
                            LinkContext::Link,
                        );
                        Tag::Link(link_ty, destination, title)
                    }
                    Tag::Image(link_ty, destination, title) => {
                        let destination = this.preprocessor.normalize_link_or_leave_as_is(
                            this.chapter,
                            destination,
                            LinkContext::Image,
                        );
                        Tag::Image(link_ty, destination, title)
                    }
                    Tag::CodeBlock(CodeBlockKind::Fenced(info_string)) => {
                        // MdBook supports custom attributes on Rust code blocks.
                        // Attributes are separated by a comma, space, or tab from the 'rust' prefix.
                        // See https://rust-lang.github.io/mdBook/format/mdbook.html#rust-code-block-attributes
                        // This strips out the attributes.
                        static MDBOOK_ATTRIBUTES: Lazy<Regex> =
                            Lazy::new(|| Regex::new(r"^rust[, \t].*").unwrap());
                        let info_string = match MDBOOK_ATTRIBUTES.replace(&info_string, "rust") {
                            Cow::Borrowed(_) => info_string,
                            Cow::Owned(info_string) => info_string.into(),
                        };
                        Tag::CodeBlock(CodeBlockKind::Fenced(info_string))
                    }
                    tag => tag,
                };
                this.start_tags.push(tag.clone());
                Event::Start(tag)
            }
            Event::End(_) => {
                let tag = this.start_tags.pop().unwrap();
                if let Tag::List(_) = &tag {
                    this.preprocessor.ctx.cur_list_depth -= 1;
                };
                Event::End(tag)
            }
            Event::TaskListMarker(checked) => {
                (this.preprocessor.ctx.pandoc).enable_extension(pandoc::Extension::TaskLists);
                Event::TaskListMarker(checked)
            }
            event => event,
        };

        let parse_html = |this: &mut Self, mut html: CowStr<'book>| {
            // TODO: need to convert entire block--keep a VecDeque of current block contents?
            // If HTML is inline, convert rest of paragraph from commonmark->html->commonmark,
            // which parses HTML elements like links and images into their markdown equivalents

            static FONT_AWESOME_ICON_I: Lazy<Regex> = Lazy::new(|| {
                Regex::new(r#"<i\s+class\s*=\s*"fa fa-(?P<icon>.*?)"(>\s*</i>|/>)"#).unwrap()
            });
            html = match FONT_AWESOME_ICON_I
                .replace_all(&html, r#"<span class="fa fa-$icon"></span>"#)
            {
                Cow::Borrowed(_) => html,
                Cow::Owned(html) => html.into(),
            };
            let from = {
                let mut format = String::from("html");
                if pandoc::Extension::AutoIdentifiers
                    .check_availability(&this.preprocessor.ctx.pandoc.version)
                    .is_available()
                {
                    format.push('-');
                    format.push_str(pandoc::Extension::AutoIdentifiers.name());
                }
                format
            };
            let format = {
                let mut format = String::from("commonmark");
                if html.contains("<dl>")
                    && (this.preprocessor.ctx.pandoc)
                        .enable_extension(pandoc::Extension::DefinitionLists)
                        .is_available()
                {
                    format.push('+');
                    format.push_str(pandoc::Extension::DefinitionLists.name());
                }
                for extension in [
                    pandoc::Extension::Strikeout,
                    pandoc::Extension::Footnotes,
                    pandoc::Extension::PipeTables,
                    pandoc::Extension::TaskLists,
                    pandoc::Extension::Attributes,
                    pandoc::Extension::GfmAutoIdentifiers,
                    pandoc::Extension::RawAttribute,
                    pandoc::Extension::FencedDivs,
                ] {
                    if (this.preprocessor.ctx.pandoc)
                        .enable_extension(extension)
                        .is_available()
                    {
                        format.push('+');
                        format.push_str(extension.name());
                    }
                }
                format
            };
            let mut pandoc = Command::new("pandoc")
                .args(["-f", &from, "-t", &format])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            write!(pandoc.stdin.take().unwrap(), "{html}").unwrap();
            let output = pandoc.wait_with_output().unwrap();
            let mut commonmark = if output.status.success() {
                // Pandoc was provided UTF8 so should output UTF8
                String::from_utf8(output.stdout).unwrap()
            } else {
                log::warn!(
                    "Failed to parse raw HTML with Pandoc: {}",
                    String::from_utf8_lossy(&output.stderr),
                );
                todo!()
            };
            if let OutputFormat::Latex { packages } = &mut this.preprocessor.ctx.output {
                if (this.preprocessor.ctx.pandoc)
                    .enable_extension(pandoc::Extension::RawAttribute)
                    .is_available()
                {
                    // Pandoc always emits spans in a standardized format with
                    // a separate closing tag and no extra whitespace
                    static FONT_AWESOME_ICON_SPAN: Lazy<Regex> = Lazy::new(|| {
                        Regex::new(r#"<span class="fa fa-(?P<icon>.*?)"></span>"#).unwrap()
                    });
                    commonmark = match FONT_AWESOME_ICON_SPAN
                        .replace_all(&commonmark, r"`\faicon{$icon}`{=latex}")
                    {
                        Cow::Borrowed(_) => commonmark,
                        Cow::Owned(commonmark) => {
                            packages.need(latex::Package::FontAwesome);
                            commonmark
                        }
                    };
                }
            }
            if commonmark.contains("Bruijn") {
                dbg!(&html, &commonmark);
            }
            pulldown_cmark::Parser::new_ext(
                // TODO: don't leak
                String::leak(commonmark),
                Self::PARSER_OPTIONS,
            )
        };

        loop {
            // Try popping an event off the front of the current block
            if let Some(event) = self.current_block.pop_front() {
                break Some(preprocess_non_html_event(self, event));
            }

            assert_eq!(self.start_tags, []);

            // Parse the next block
            match self.parser.next()? {
                // Standalone HTML block
                Event::Html(html) => {
                    let mut html_string = html.into_string();
                    while let Some(Event::Html(more)) = self.parser.peek() {
                        html_string.push_str(more);
                        self.parser.next();
                    }
                    let parsed_html = parse_html(self, html_string.into());
                    self.current_block.extend(parsed_html);
                }
                start @ Event::Start(_) => {
                    self.current_block.push_back(start);
                    let mut opened_tags = 1;
                    let mut contains_html = false;
                    self.current_block
                        .extend(self.parser.peeking_take_while(|event| {
                            if opened_tags == 0 {
                                return false;
                            }
                            match event {
                                Event::Start(_) => opened_tags += 1,
                                Event::End(_) => opened_tags -= 1,
                                Event::Html(_) => contains_html = true,
                                _ => {}
                            }
                            true
                        }));
                    if contains_html {
                        dbg!(&self.current_block);
                        let mut html = String::new();
                        pulldown_cmark::html::push_html(&mut html, self.current_block.drain(..));
                        let parsed_html = parse_html(self, html.into());
                        self.current_block.extend(parsed_html);
                    }
                }
                event => break Some(preprocess_non_html_event(self, event)),
            }
        }
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
