use std::{
    borrow::{Borrow, Cow},
    cmp,
    collections::{hash_map::DefaultHasher, HashMap, VecDeque},
    ffi::OsString,
    fmt::{self, Write},
    fs::{self, File},
    hash::{Hash, Hasher},
    io::{self, Write as _},
    num::NonZeroU32,
    ops::Range,
    path::{Path, PathBuf},
    str,
};

use anyhow::{anyhow, Context};
use ego_tree::NodeId;
use html5ever::{expanded_name, local_name, ns, tendril::format_tendril};
use mdbook::book::{BookItem, BookItems, Chapter};
use normpath::PathExt;
use once_cell::sync::Lazy;
use pulldown_cmark::{CowStr, Event, HeadingLevel, LinkType, Tag, TagEnd};
use regex::Regex;
use walkdir::WalkDir;

use crate::{
    latex,
    pandoc::{self, native::ColWidth, OutputFormat, RenderContext},
    url, CommonConfig as Config, MarkdownExtensionConfig,
};

mod code;

pub mod tree;
use tree::{Element, MdElement, Node, QualNameExt, TreeBuilder};

pub struct Preprocessor<'book> {
    pub(crate) ctx: RenderContext<'book>,
    preprocessed: PathBuf,
    preprocessed_relative_to_root: PathBuf,
    redirects: HashMap<PathBuf, String>,
    hosted_html: Option<&'book str>,
    cfg: &'book Config,
    unresolved_links: bool,
    chapters: HashMap<&'book Path, IndexedChapter<'book>>,
}

pub struct Preprocess<'book> {
    preprocessor: Preprocessor<'book>,
    items: BookItems<'book>,
    part_num: usize,
}

struct IndexedChapter<'book> {
    chapter: &'book Chapter,
    anchors: Option<ChapterAnchors<'book>>,
}

#[derive(Default, Debug)]
struct ChapterAnchors<'book> {
    /// Anchor to the beginning of the chapter, usable as a link fragment.
    beginning: Option<CowStr<'book>>,
}

#[derive(Debug)]
struct NormalizedPath {
    src_absolute_path: PathBuf,
    preprocessed_absolute_path: PathBuf,
    preprocessed_path_relative_to_root: PathBuf,
}

impl<'book> Preprocessor<'book> {
    pub fn new(ctx: RenderContext<'book>, cfg: &'book Config) -> anyhow::Result<Self> {
        let preprocessed = ctx.destination.join("src");

        if preprocessed.try_exists()? {
            fs::remove_dir_all(&preprocessed)?;
        }
        fs::create_dir_all(&preprocessed)?;

        for entry in WalkDir::new(&ctx.book.source_dir).follow_links(true) {
            let entry = entry?;
            let src = entry.path();
            if src.starts_with(ctx.book.destination.as_path()) {
                continue;
            }
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

        let mut chapters = HashMap::new();
        for section in ctx.book.book.iter() {
            if let BookItem::Chapter(
                chapter @ Chapter {
                    source_path: Some(path),
                    ..
                },
            ) = section
            {
                let chapter = IndexedChapter {
                    chapter,
                    anchors: Default::default(),
                };
                chapters.insert(path.as_path(), chapter);
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
            cfg,
            unresolved_links: false,
            chapters,
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
                tracing::debug!("Processing redirect: {src} => {dst}");

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
                        .normalize_link(&src, LinkType::Autolink, dst.into())
                        .map_err(|(err, _)| err)
                        .context("Unable to normalize redirect destination")?;
                    let src = self
                        .normalize_path(&src)
                        .context("Unable to normalize redirect source")?
                        .preprocessed_path_relative_to_root;

                    tracing::debug!("Registered redirect: {} => {dst}", src.display());
                    self.redirects.insert(src, dst.into_string());
                    Ok(())
                })
                .map_err(|err| (err, entry))
            })
            .filter_map(Result::err)
            .for_each(|(err, (src, dst))| {
                tracing::warn!("Failed to resolve redirect: {src} => {dst}: {err:#}")
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
    ) -> CowStr<'link> {
        let Some(chapter_path) = &chapter.source_path else {
            return link;
        };
        self.normalize_link(chapter_path, link_type, link)
            .unwrap_or_else(|(err, link)| {
                tracing::warn!(
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
        link_type: LinkType,
        link: CowStr<'link>,
    ) -> Result<CowStr<'link>, (anyhow::Error, CowStr<'link>)> {
        use LinkType::*;
        match link_type {
            // Don't try to normalize emails
            Email => return Ok(link),
            Inline
            | Reference
            | ReferenceUnknown
            | Collapsed
            | CollapsedUnknown
            | Shortcut
            | ShortcutUnknown
            | Autolink
            | WikiLink { .. } => {}
        }

        // URI scheme definition: https://datatracker.ietf.org/doc/html/rfc3986#section-3.1
        static SCHEME: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z][a-z0-9+.-]*:").unwrap());

        let os_to_utf8 = |os: OsString| {
            os.into_string()
                .map_err(|path| anyhow!("Path is not valid UTF8: {path:?}"))
        };

        let link_path_range = || ..link.find(['?', '#']).unwrap_or(link.len());

        if SCHEME.is_match(&link) {
            // Leave URIs with schemes untouched
            return Ok(link);
        }

        // URI is a relative-reference: https://datatracker.ietf.org/doc/html/rfc3986#section-4.2
        if link.starts_with("//") {
            // URI is a network-path reference; leave it untouched
            Ok(link)
        } else {
            // URI is an absolute-path or relative-path reference, which must be resolved
            // relative to the book root or the current chapter's directory, respectively
            let path_range = link_path_range();
            let link_path = match &link[path_range] {
                // Internal reference within chapter
                "" if link.starts_with('#') => return Ok(link),
                path => Path::new(path),
            };
            let path = if let Ok(relative_to_root) = link_path.strip_prefix("/") {
                self.preprocessed.join(relative_to_root)
            } else {
                let chapter_dir = chapter_path.parent().unwrap();
                chapter_dir.join(link_path)
            };

            enum LinkDestination<'a> {
                PartiallyResolved(NormalizedPath),
                FullyResolved(Cow<'a, str>),
            }

            let normalized_path = self
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
                        Ok(LinkDestination::FullyResolved(Cow::Borrowed(path)))
                    } else {
                        if !normalized.exists()? {
                            normalized.copy_to_preprocessed()?;
                        }
                        Ok(LinkDestination::PartiallyResolved(normalized))
                    }
                });
            let normalized_link = match normalized_path {
                Err(err) => Err((err, link)),
                Ok(normalized_path) => {
                    let (normalized_path, add_anchor) = match normalized_path {
                        LinkDestination::FullyResolved(path) => (path, None),
                        LinkDestination::PartiallyResolved(normalized_path) => {
                            // Check whether link is anchored (points to a section within a document)
                            let already_anchored = link[path_range.end..].contains('#');

                            // As of version 3.2, pandoc no longer generates an anchor at the beginning
                            // of each file, so we need to find alternate destination for chapter links
                            let add_anchor = if already_anchored {
                                None
                            } else {
                                let relative_path = normalized_path
                                    .preprocessed_path_relative_to_root
                                    .strip_prefix(&self.preprocessed_relative_to_root)
                                    .unwrap();
                                let chapter = self.chapters.get_mut(relative_path);
                                match chapter {
                                    None => {
                                        tracing::trace!(
                                            "Not recognized as a chapter: {}",
                                            relative_path.display(),
                                        );
                                        None
                                    }
                                    Some(IndexedChapter {
                                        chapter,
                                        ref mut anchors,
                                    }) => {
                                        let anchors = match anchors {
                                            Some(anchors) => anchors,
                                            None => match ChapterAnchors::new(chapter) {
                                                Ok(found) => anchors.insert(found),
                                                Err(err) => return Err((err, link)),
                                            },
                                        };
                                        match &anchors.beginning {
                                            Some(anchor) => Some(anchor),
                                            None => {
                                                let err = anyhow!(
                                                    "failed to link to beginning of chapter"
                                                );
                                                return Err((err, link));
                                            }
                                        }
                                    }
                                }
                            };

                            match os_to_utf8(
                                normalized_path
                                    .preprocessed_path_relative_to_root
                                    .into_os_string(),
                            ) {
                                Ok(path) => (path.into(), add_anchor),
                                Err(err) => return Err((err, link)),
                            }
                        }
                    };

                    let mut link = link.into_string();
                    link.replace_range(path_range, &normalized_path);

                    if let Some(anchor) = add_anchor {
                        link.push('#');
                        link.push_str(anchor);
                    }

                    Ok(link.into())
                }
            };
            normalized_link
                .or_else(|(err, original_link)| {
                    self.hosted_html
                        .ok_or_else(|| {
                            self.unresolved_links = true;
                            err
                        })
                        .and_then(|hosted_html_uri| {
                            let mut hosted = OsString::from(hosted_html_uri.trim_end_matches('/'));
                            hosted.push("/");
                            hosted.push(
                                path.strip_prefix(&self.ctx.book.source_dir)
                                    .or(path.strip_prefix(&self.preprocessed))
                                    .unwrap_or(&path),
                            );
                            let hosted = os_to_utf8(hosted)?;
                            tracing::event!(
                                // In tests, log at a higher level to detect link breakage
                                if cfg!(test) {
                                    tracing::Level::INFO
                                } else {
                                    tracing::Level::DEBUG
                                },
                                "Failed to resolve link '{original_link}' in chapter '{}', \
                                 linking to hosted HTML book at '{hosted}'",
                                chapter_path.display(),
                            );
                            Ok(hosted)
                        })
                        .map(Cow::Owned)
                        .map_err(|err| (err, original_link))
                })
                .map(CowStr::from)
        }
    }

    /// Generates a GitHub Markdown-flavored identifier for a heading with the provided content.
    fn make_gfm_identifier<E>(content: impl IntoIterator<Item = E>) -> String
    where
        E: Borrow<Event<'book>>,
    {
        let mut id = String::new();
        for event in content {
            if let Event::Text(text) | Event::Code(text) = event.borrow() {
                for c in text.chars() {
                    match c {
                        ' ' => id.push('-'),
                        c @ ('-' | '_') => id.push(c),
                        c if c.is_alphanumeric() => id.extend(c.to_lowercase()),
                        _ => {}
                    }
                }
            }
        }
        id
    }

    /// Converts an absolute path to a normalized form usable as a relative path within the preprocessed source directory.
    ///
    /// The normalized form:
    /// - Is a relative path
    /// - Does not traverse parent directories
    /// - Uniquely corresponds to the file at the original path
    fn normalize_path(&self, path: &Path) -> anyhow::Result<NormalizedPath> {
        let absolute_path = path
            .normalize()
            .or_else(|err| match path.extension() {
                Some(extension) if extension == "html" => {
                    if path.file_stem().is_some_and(|name| name == "index") {
                        if let Ok(path) = path.with_file_name("README.md").normalize() {
                            return Ok(path);
                        }
                    }
                    path.with_extension("md").normalize()
                }
                Some(extension) if extension == "md" => path.with_extension("html").normalize(),
                _ => Err(err),
            })
            .with_context(|| format!("Unable to normalize path: {}", path.display()))?
            .into_path_buf();
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
                let mut writer = io::BufWriter::new(normalized.create()?);
                let res = self
                    .preprocess_chapter(chapter, &mut writer)
                    .with_context(|| format!("failed to preprocess chapter '{}'", chapter.name));
                writer.flush()?;
                drop(writer);
                if let Err(err) = res {
                    match fs::read_to_string(normalized.preprocessed_absolute_path) {
                        Ok(preprocessed) => {
                            tracing::error!(
                                "Failed to preprocess chapter '{}' with content:\n{}",
                                chapter.name,
                                chapter.content,
                            );
                            tracing::error!("Partially preprocessed chapter: {preprocessed}")
                        }
                        Err(err) => {
                            tracing::error!("Failed to read partially preprocessed chapter: {err}")
                        }
                    }
                    return Err(err);
                }
                Ok(Some(normalized.preprocessed_path_relative_to_root))
            }
            BookItem::Separator => {
                tracing::debug!("Ignoring separator");
                Ok(None)
            }
            BookItem::PartTitle(name) => match self.preprocessor.ctx.output {
                OutputFormat::Latex { .. } => {
                    self.part_num += 1;
                    let kebab_case_name = Preprocessor::make_kebab_case(name);
                    let path =
                        PathBuf::from(format!("part-{}-{kebab_case_name}.md", self.part_num));
                    let mut file = File::options()
                        .write(true)
                        .create_new(true)
                        .open(self.preprocessor.preprocessed.join(&path))
                        .with_context(|| format!("Unable to create file for part '{name}'"))?;
                    writeln!(
                        file,
                        r#"[Para [RawInline (Format "latex") "\\part{{{name}}}"]]"#
                    )?;
                    Ok(Some(
                        self.preprocessor.preprocessed_relative_to_root.join(path),
                    ))
                }
                _ => {
                    tracing::warn!("Ignoring part separator: {name}");
                    Ok(None)
                }
            },
        }
    }

    fn preprocess_chapter(
        &mut self,
        chapter: &'book Chapter,
        out: impl io::Write,
    ) -> anyhow::Result<()> {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::debug!("Preprocessing '{}':\n{}", chapter.name, chapter.content);
        } else {
            tracing::debug!("Preprocessing '{}'", chapter.name);
        }
        let preprocessed = PreprocessChapter::new(&mut self.preprocessor, chapter, self.part_num);
        preprocessed.preprocess(out)
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

pub struct PreprocessChapter<'book, 'preprocessor> {
    pub(crate) preprocessor: &'preprocessor mut Preprocessor<'book>,
    chapter: &'book Chapter,
    part_num: usize,
    parser: Parser<'book>,
    stack: Vec<NodeId>,
    first_heading: Option<HeadingLevel>,
    identifiers: HashMap<String, NonZeroU32>,
    in_code: bool,
    in_table_head: bool,
}

struct Parser<'book> {
    lookahead: VecDeque<(Event<'book>, Range<usize>)>,
    parser: pulldown_cmark::OffsetIter<'book, pulldown_cmark::DefaultBrokenLinkCallback>,
}

impl<'book> Parser<'book> {
    fn new(
        md: &'book str,
        config: &mdbook::config::HtmlConfig,
        extensions: MarkdownExtensionConfig,
    ) -> Self {
        use pulldown_cmark::Options;

        // See https://rust-lang.github.io/mdBook/format/markdown.html#extensions
        let options = {
            let mut options = Options::empty()
                .union(Options::ENABLE_TABLES)
                .union(Options::ENABLE_FOOTNOTES)
                .union(Options::ENABLE_STRIKETHROUGH)
                .union(Options::ENABLE_TASKLISTS)
                .union(Options::ENABLE_HEADING_ATTRIBUTES);

            let MarkdownExtensionConfig {
                math,
                superscript,
                subscript,
            } = extensions;

            if config.definition_lists {
                options |= Options::ENABLE_DEFINITION_LIST;
            }
            if config.admonitions {
                options |= Options::ENABLE_GFM;
            }
            if math {
                options |= Options::ENABLE_MATH;
            }
            if superscript {
                options |= Options::ENABLE_SUPERSCRIPT;
            }
            if subscript {
                options |= Options::ENABLE_SUBSCRIPT;
            }
            options
        };

        Self {
            lookahead: Default::default(),
            parser: pulldown_cmark::Parser::new_ext(md, options).into_offset_iter(),
        }
    }

    fn next_if(&mut self, func: impl FnOnce(&Event<'book>) -> bool) -> Option<Event<'book>> {
        if self.lookahead.is_empty() {
            self.lookahead.push_back(self.parser.next()?);
        }
        match self.lookahead.front() {
            Some((event, _)) if func(event) => self.lookahead.pop_front().map(|(event, _)| event),
            _ => None,
        }
    }

    fn peek_until(
        &mut self,
        mut end: impl FnMut(&Event<'book>) -> bool,
    ) -> impl Iterator<Item = &'_ Event<'book>> + '_ {
        let n = self
            .lookahead
            .iter()
            .enumerate()
            .find_map(|(idx, (event, _))| end(event).then_some(idx));
        let n = n.unwrap_or_else(|| loop {
            let (event, range) = self
                .parser
                .next()
                .expect("start tag should be followed by a matching end tag");
            let done = end(&event);
            self.lookahead.push_back((event, range));
            if done {
                break self.lookahead.len();
            }
        });
        (0..n)
            .map(|idx| &self.lookahead[idx])
            .map(|(event, _)| event)
    }
}

impl<'book> Iterator for Parser<'book> {
    type Item = (Event<'book>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        self.lookahead.pop_front().or_else(|| self.parser.next())
    }
}

impl<'book, 'preprocessor> PreprocessChapter<'book, 'preprocessor> {
    fn new(
        preprocessor: &'preprocessor mut Preprocessor<'book>,
        chapter: &'book Chapter,
        part_num: usize,
    ) -> Self {
        Self {
            chapter,
            parser: Parser::new(
                &chapter.content,
                preprocessor.ctx.html,
                preprocessor.cfg.markdown.extensions,
            ),
            preprocessor,
            stack: Vec::new(),
            first_heading: None,
            identifiers: Default::default(),
            part_num,
            in_code: false,
            in_table_head: false,
        }
    }

    pub fn part_num(&self) -> usize {
        self.part_num
    }

    pub fn chapter(&self) -> &Chapter {
        self.chapter
    }

    fn preprocess_heading<'b>(
        &mut self,
        id: Option<CowStr<'b>>,
        level: HeadingLevel,
        mut classes: Vec<CowStr<'b>>,
        attrs: Vec<(CowStr<'b>, Option<CowStr<'b>>)>,
    ) -> MdElement<'b> {
        const PANDOC_UNNUMBERED_CLASS: &str = "unnumbered";
        const PANDOC_UNLISTED_CLASS: &str = "unlisted";

        let cfg = &self.preprocessor.cfg;

        let id = Some(match id {
            Some(id) => id,
            None => {
                let mut id = Preprocessor::make_gfm_identifier(
                    self.parser
                        .peek_until(|event| matches!(event, Event::End(TagEnd::Heading(..)))),
                );
                if let Some(count) = self.identifiers.get_mut(&id) {
                    write!(id, "-{}", count.get()).unwrap();
                    *count = count.saturating_add(1);
                } else {
                    self.identifiers.insert(id.clone(), NonZeroU32::MIN);
                }
                id.into()
            }
        });

        // Number the first heading in each numbered chapter, allowing Pandoc to generate a
        // table of contents that mirror's mdbook's. Unfortunately, it will not match exactly,
        // since mdbook generates the TOC based on SUMMARY.md, but there's no obvious way to
        // tell Pandoc to use those chapter names for the TOC instead of the heading names.
        if (self.chapter.number.is_none() || self.first_heading.is_some())
            && !cfg.number_internal_headings
        {
            classes.push(PANDOC_UNNUMBERED_CLASS.into());
        }
        let first_heading = if let Some(first_heading) = self.first_heading {
            if !cfg.list_internal_headings {
                classes.push(PANDOC_UNLISTED_CLASS.into());
            }
            first_heading
        } else {
            *self.first_heading.insert(level)
        };

        let to_int = |level| match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        };
        let num_parents = self.chapter.parent_names.len();
        let num_parents = num_parents.try_into().unwrap_or(u16::MAX);
        let shrink_by = num_parents.saturating_sub(to_int(first_heading) - 1);
        let level = to_int(level).saturating_add(shrink_by);
        MdElement::Heading {
            level,
            id,
            classes,
            attrs,
        }
    }

    pub fn column_widths<'table>(
        &self,
        table: &'table str,
    ) -> impl Iterator<Item = Option<ColWidth>> + 'table {
        let mut wide = false;
        let mut rows = table.lines().inspect(|line| {
            if line.len() > self.preprocessor.ctx.columns {
                wide = true;
            }
        });
        // The second row of a table is the delimiter row
        // See: https://github.github.com/gfm/#tables-extension-
        let delimiter_row = rows.nth(1).expect("table did not contain a delimiter row");
        let column_widths = || {
            delimiter_row
                // Cells are separated by pipes
                .split('|')
                .map(|cell| cell.chars().filter(char::is_ascii_punctuation).count())
                .filter(|&width| width > 0)
        };
        // Consume iterator to finish checking for long rows
        rows.for_each(|_| ());
        let total_width = column_widths().sum::<usize>() as f64;
        column_widths().map(move |width| wide.then(|| ColWidth(width as f64 / total_width)))
    }

    fn preprocess(mut self, writer: impl io::Write) -> anyhow::Result<()> {
        let mut tree = TreeBuilder::new();
        while let Some((event, range)) = self.parser.next() {
            self.preprocess_event(event, range.clone(), &mut tree)
                .with_context(|| {
                    format!("failed to preprocess '{}'", &self.chapter.content[range])
                })?;
        }
        let events = tree.finish();

        tracing::trace!("Writing Pandoc AST for chapter '{}'", self.chapter.name);
        pandoc::native::Serializer::serialize(writer, self, |blocks| events.emit(blocks))
    }

    fn preprocess_event(
        &mut self,
        event: Event<'book>,
        range: Range<usize>,
        tree: &mut TreeBuilder<'book>,
    ) -> anyhow::Result<()> {
        tracing::trace!("Preprocessing event: {event:?}");
        match event {
            Event::Start(tag) => {
                let push_element = |this: &mut Self, tree: &mut TreeBuilder<'book>, element| {
                    let node = tree.create_element(element)?;
                    this.stack.push(node);
                    Ok::<_, anyhow::Error>(node)
                };
                let push_html_element = |this: &mut Self, tree: &mut TreeBuilder<'book>, name| {
                    let node = tree.create_html_element(name)?;
                    this.stack.push(node);
                    Ok(node)
                };
                match tag {
                    Tag::List(start_number) => {
                        self.preprocessor.ctx.cur_list_depth += 1;
                        self.preprocessor.ctx.max_list_depth = cmp::max(
                            self.preprocessor.ctx.max_list_depth,
                            self.preprocessor.ctx.cur_list_depth,
                        );
                        push_element(self, tree, MdElement::List(start_number))
                    }
                    Tag::Item => push_element(self, tree, MdElement::Item),
                    Tag::DefinitionList => {
                        self.preprocessor.ctx.cur_list_depth += 1;
                        self.preprocessor.ctx.max_list_depth = cmp::max(
                            self.preprocessor.ctx.max_list_depth,
                            self.preprocessor.ctx.cur_list_depth,
                        );
                        push_html_element(self, tree, local_name!("dl"))
                    }
                    Tag::DefinitionListTitle => push_html_element(self, tree, local_name!("dt")),
                    Tag::DefinitionListDefinition => {
                        push_html_element(self, tree, local_name!("dd"))
                    }
                    Tag::FootnoteDefinition(label) => {
                        let node = push_element(self, tree, MdElement::FootnoteDefinition)?;
                        tree.footnote(label, node);
                        Ok(node)
                    }
                    Tag::Table(alignment) => push_element(
                        self,
                        tree,
                        MdElement::Table {
                            alignment,
                            source: &self.chapter.content[range],
                        },
                    ),
                    Tag::TableHead => {
                        self.in_table_head = true;
                        push_html_element(self, tree, local_name!("thead"))
                    }
                    Tag::TableRow => push_html_element(self, tree, local_name!("tr")),
                    Tag::TableCell if self.in_table_head => {
                        push_html_element(self, tree, local_name!("th"))
                    }
                    Tag::TableCell => push_html_element(self, tree, local_name!("td")),
                    Tag::Heading {
                        level,
                        id,
                        classes,
                        attrs,
                    } => {
                        let element = self.preprocess_heading(id, level, classes, attrs);
                        push_element(self, tree, element)
                    }
                    Tag::Link {
                        link_type,
                        dest_url,
                        title,
                        id: _,
                    } => {
                        let dest_url =
                            url::encode(self.preprocessor.normalize_link_or_leave_as_is(
                                self.chapter,
                                link_type,
                                url::best_effort_decode(dest_url),
                            ));
                        push_element(self, tree, MdElement::Link { dest_url, title })
                    }
                    Tag::Paragraph => push_element(self, tree, MdElement::Paragraph),
                    Tag::BlockQuote(kind) => push_element(self, tree, MdElement::BlockQuote(kind)),
                    Tag::CodeBlock(kind) => {
                        self.in_code = true;
                        push_element(self, tree, MdElement::CodeBlock(kind))
                    }
                    Tag::Emphasis => push_element(self, tree, MdElement::Emphasis),
                    Tag::Strong => push_element(self, tree, MdElement::Strong),
                    Tag::Strikethrough => push_html_element(self, tree, local_name!("s")),
                    Tag::Superscript => push_html_element(self, tree, local_name!("sup")),
                    Tag::Subscript => push_html_element(self, tree, local_name!("sub")),
                    Tag::Image {
                        link_type,
                        dest_url,
                        title,
                        id,
                    } => push_element(
                        self,
                        tree,
                        MdElement::Image {
                            link_type,
                            dest_url,
                            title,
                            id,
                        },
                    ),
                    Tag::HtmlBlock => return Ok(()),
                    Tag::MetadataBlock(_) => {
                        tracing::warn!("Ignoring metadata block");
                        for (event, _) in &mut self.parser {
                            if let Event::End(TagEnd::MetadataBlock(_)) = event {
                                break;
                            }
                        }
                        return Ok(());
                    }
                }?;
                Ok(())
            }
            Event::End(TagEnd::HtmlBlock | TagEnd::MetadataBlock(_)) => Ok(()),
            Event::End(end) => {
                let node = self
                    .stack
                    .pop()
                    .unwrap_or_else(|| panic!("unmatched {end:?}"));
                let html = {
                    let tree = tree.html.tokenizer.sink.sink.tree.borrow();
                    let Node::Element(element) = tree.tree.get(node).unwrap().value() else {
                        unreachable!()
                    };
                    match element {
                        Element::Markdown(MdElement::List(_)) => {
                            self.preprocessor.ctx.cur_list_depth -= 1
                        }
                        Element::Markdown(MdElement::CodeBlock(_)) => self.in_code = false,
                        Element::Html(element)
                            if element.name.expanded() == expanded_name!(html "dl") =>
                        {
                            self.preprocessor.ctx.cur_list_depth -= 1
                        }
                        Element::Html(element)
                            if element.name.expanded() == expanded_name!(html "thead") =>
                        {
                            self.in_table_head = false
                        }
                        _ => {}
                    }
                    let name = element.name();
                    (!name.is_void_element()).then(|| format_tendril!("</{}>", name.local))
                };
                if let Some(html) = html {
                    tree.process_html(html);
                }
                Ok(())
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                tree.process_html(html.as_ref().into());
                Ok(())
            }
            Event::Text(text) => {
                let push_text = |tree: &mut TreeBuilder<'book>, text| {
                    tree.create_element(MdElement::Text(text))?;
                    tree.process_html("</span>".into());
                    Ok::<_, anyhow::Error>(())
                };
                // Emulate the HTML renderer's mathjax support by parsing \[, $$, and \( delimited
                // math and generating pandoc DisplayMath/InlineMath nodes
                if self.preprocessor.ctx.html.mathjax_support
                    && !self.in_code
                    && text.contains(['\\', '$'])
                {
                    static MATHJAX: Lazy<Regex> = Lazy::new(|| {
                        Regex::new(r"(?s)(\\\[.*?\\\]|\$\$.*?\$\$|\\\(.*?\\\))").unwrap()
                    });

                    // Collect subsequent text events into a single string since pulldown-cmark tends
                    // to split math-like expressions into multiple events
                    let mut text = text.into_string();
                    while let Some(event) = self
                        .parser
                        .next_if(|event| matches!(event, Event::Text(_) | Event::SoftBreak))
                    {
                        match event {
                            Event::Text(t) => text.push_str(&t),
                            Event::SoftBreak => text.push('\n'),
                            _ => unreachable!(),
                        }
                    }

                    // Separate math from text
                    let mut pushed_up_to = 0;
                    for mathjax in MATHJAX.find_iter(&text) {
                        let preceding_text = &text[pushed_up_to..mathjax.start()];
                        if !preceding_text.is_empty() {
                            push_text(tree, preceding_text.to_owned().into())?;
                        }
                        let (delim, rest) = mathjax.as_str().split_at(2);
                        let math = &rest[..rest.len() - 2];
                        let kind = match delim {
                            "\\(" => latex::MathType::Inline,
                            "\\[" | "$$" => latex::MathType::Display,
                            _ => unreachable!(),
                        };
                        self.create_math_node(math.to_owned().into(), kind, tree)?;
                        pushed_up_to = mathjax.end();
                    }
                    let remaining_text = &text[pushed_up_to..];
                    if !remaining_text.is_empty() {
                        push_text(tree, remaining_text.to_owned().into())?;
                    }
                    Ok(())
                } else {
                    push_text(tree, text)
                }
            }
            Event::Code(code) => {
                tree.create_element(MdElement::InlineCode(code))?;
                tree.process_html("</code>".into());
                Ok(())
            }
            Event::FootnoteReference(label) => {
                tree.create_element(MdElement::FootnoteReference(label))?;
                tree.process_html("</sup>".into());
                Ok(())
            }
            Event::SoftBreak => {
                tree.create_element(MdElement::SoftBreak)?;
                Ok(())
            }
            Event::HardBreak => {
                tree.process_html("<br>".into());
                Ok(())
            }
            Event::Rule => {
                tree.process_html("<hr>".into());
                Ok(())
            }
            Event::TaskListMarker(checked) => {
                tree.create_element(MdElement::TaskListMarker(checked))?;
                Ok(())
            }
            Event::InlineMath(math) => self.create_math_node(math, latex::MathType::Inline, tree),
            Event::DisplayMath(math) => self.create_math_node(math, latex::MathType::Display, tree),
        }
    }

    fn create_math_node(
        &mut self,
        mut math: CowStr<'book>,
        kind: latex::MathType,
        tree: &mut TreeBuilder<'book>,
    ) -> anyhow::Result<()> {
        if matches!(self.preprocessor.ctx.output, OutputFormat::Latex { .. }) {
            // Extract TeX macro definitions into a latex raw block to mirror the behavior of MathJax
            // where macros defined within a math inline/block are available in other inlines/blocks
            let mut macros = Vec::new();
            let without_macros =
                latex::MACRO_DEFINITION.replace_all(&math, |caps: &regex::Captures<'_>| {
                    if caps.name("newcommand").is_some() {
                        // Replace each \newcommand with a \providecommand + \renewcommand
                        // to avoid errors when the same command is defined in multiple chapters
                        let (command, definition) = (&caps["command"], &caps["definition"]);
                        macros.push(format!(
                            r"\providecommand{command}{{}}\renewcommand{command}{definition}"
                        ));
                    } else {
                        macros.push(caps[0].to_string());
                    }
                    "" // Remove macro definitions from the math block
                });
            if let Cow::Owned(without_macros) = without_macros {
                math = without_macros.trim().to_string().into();
            }
            if !macros.is_empty() {
                tree.create_element(MdElement::RawInline {
                    format: "latex",
                    raw: macros.join("\n").into(),
                })?;
                tree.process_html("</span>".into());
                if !math.is_empty() {
                    tree.create_element(MdElement::SoftBreak)?;
                }
            }
        }
        if !math.trim().is_empty() {
            tree.create_element(MdElement::Math(kind, math))?;
            tree.process_html("</span>".into());
        }
        Ok(())
    }

    pub fn resolve_image_url<'url>(
        &mut self,
        dest_url: CowStr<'url>,
        link_type: LinkType,
    ) -> CowStr<'url> {
        let resolved = match self.chapter.source_path.as_ref() {
            None => Err((anyhow!("chapter has no path"), dest_url)),
            Some(chapter_path) => {
                self.preprocessor
                    .normalize_link(chapter_path, link_type, dest_url)
            }
        };
        match resolved {
            Ok(link) => link,
            Err((err, link)) => {
                tracing::warn!(
                    "Failed to resolve image link '{link}' in chapter '{}': {err:#}",
                    self.chapter.name,
                );
                link
            }
        }
    }
}

impl<'book> ChapterAnchors<'book> {
    /// Searches for tags in the provided chapter with identifiers that can be used as link anchors.
    fn new(chapter: &'book Chapter) -> anyhow::Result<Self> {
        use pulldown_cmark::{Options, Parser};
        let mut parser = Parser::new_ext(&chapter.content, Options::ENABLE_HEADING_ATTRIBUTES);
        let beginning = 'beginning: {
            let heading_id = loop {
                let Some(event) = parser.next() else {
                    break 'beginning None;
                };
                if let Event::Start(Tag::Heading { id, .. }) = event {
                    break id;
                }
            };
            Some(heading_id.unwrap_or_else(|| {
                let heading_contents =
                    parser.take_while(|event| !matches!(event, Event::End(TagEnd::Heading(_))));
                Preprocessor::make_gfm_identifier(heading_contents).into()
            }))
        };
        if beginning.is_none() {
            tracing::warn!(
                "Failed to determine suitable anchor for beginning of chapter '{}'\
                --does it contain any headings?",
                chapter.name,
            );
        }
        Ok(Self { beginning })
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

impl fmt::Debug for IndexedChapter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IndexedChapter")
            .field("chapter", &self.chapter.name)
            .field("anchors", &self.anchors)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::Preprocessor;

    #[test]
    fn gfm_identifiers() {
        use pulldown_cmark::{Event, Tag, TagEnd};
        let convert = |source| {
            let mut parser = pulldown_cmark::Parser::new(source);
            assert!(matches!(
                parser.next().unwrap(),
                Event::Start(Tag::Heading { .. })
            ));
            Preprocessor::make_gfm_identifier(
                parser.take_while(|event| !matches!(event, Event::End(TagEnd::Heading(_)))),
            )
        };
        assert_eq!(convert("# hello"), "hello");
        insta::assert_debug_snapshot!(
            [
                "# Heading	Identifier",
                "# Heading identifiers in HTML",
                "# Maître d'hôtel",
                "# *Dogs*?--in *my* house?",
                "# [HTML], [S5], or [RTF]?",
                "# 3. Applications",
                "# 33",
                "# With _ Underscores_In It",
                "# has-hyphens",
                "# Unicode Σ",
                "# Running `mdbook` in Continuous Integration",
                "# `--passes`: add more rustdoc passes",
                "# Method-call 🐙 expressions \u{1f47c}",
                "# _-_12345",
                "# 12345",
                "# 中文",
                "# にほんご",
                "# 한국어",
                "# 中文標題 CJK title",
                "# Über",
            ]
            .map(convert),
            @r###"
            [
                "headingidentifier",
                "heading-identifiers-in-html",
                "maître-dhôtel",
                "dogs--in-my-house",
                "html-s5-or-rtf",
                "3-applications",
                "33",
                "with-_-underscores_in-it",
                "has-hyphens",
                "unicode-σ",
                "running-mdbook-in-continuous-integration",
                "--passes-add-more-rustdoc-passes",
                "method-call--expressions-",
                "_-_12345",
                "12345",
                "中文",
                "にほんご",
                "한국어",
                "中文標題-cjk-title",
                "über",
            ]
            "###
        );
    }
}
