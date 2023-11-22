use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    fmt,
    fs::{self, File},
    hash::{Hash, Hasher},
    io::{self, Write},
    iter::{self, Peekable},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context};
use mdbook::{
    book::{Book, BookItems, Chapter},
    BookItem,
};
use once_cell::sync::Lazy;
use pulldown_cmark::{CowStr, HeadingLevel};
use regex::Regex;

use crate::markdown_extensions;

pub struct Preprocessor<'a> {
    book: &'a Book,
    source_dir: &'a Path,
    destination: Cow<'a, Path>,
    options: Options,
}

pub struct Options {
    pub latex: bool,
}

pub struct PreprocessedFiles<'a> {
    preprocessor: Preprocessor<'a>,
    items: BookItems<'a>,
    part_num: usize,
}

#[derive(Debug)]
struct NormalizedPath {
    src_absolute_path: PathBuf,
    destination_absolute_path: PathBuf,
    destination_relative_path: PathBuf,
}

impl<'a> Preprocessor<'a> {
    pub fn new(
        book: &'a Book,
        source_dir: &'a Path,
        destination: Cow<'a, Path>,
        options: Options,
    ) -> Self {
        Self {
            book,
            source_dir,
            destination,
            options,
        }
    }

    pub fn preprocess(self) -> anyhow::Result<PreprocessedFiles<'a>> {
        if self.destination.try_exists()? {
            fs::remove_dir_all(&self.destination)?;
        }
        fs::create_dir_all(&self.destination)?;
        Ok(PreprocessedFiles {
            items: self.book.iter(),
            preprocessor: self,
            part_num: 0,
        })
    }

    fn preprocess_chapter(&self, chapter: &Chapter, out: impl io::Write) -> anyhow::Result<()> {
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
    ) -> CowStr<'link> {
        let Some(chapter_path) = &chapter.source_path else {
            return link;
        };
        let chapter_dir = self.source_dir.join(chapter_path.parent().unwrap());
        self.normalize_link(&chapter_dir, link)
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
    ) -> Result<CowStr<'link>, (anyhow::Error, CowStr<'link>)> {
        // URI scheme definition: https://datatracker.ietf.org/doc/html/rfc3986#section-3.1
        static SCHEME: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z][a-z0-9+.-]*:").unwrap());

        if SCHEME.is_match(&link) {
            // Leave URIs with schemes (e.g. https://google.com) untouched
            Ok(link)
        } else {
            // URI is a relative-reference: https://datatracker.ietf.org/doc/html/rfc3986#section-4.2
            if link.starts_with('/') {
                // URI is a network-path reference or absolute-path reference;
                // leave both untouched
                Ok(link)
            } else {
                // URI is a relative-path reference, which must be normalized
                let path_range = ..link.find(['?', '#']).unwrap_or(link.len());
                let relative_path = Path::new(&link[path_range]);

                let normalized = self
                    .normalize_path(&chapter_dir.join(relative_path))
                    .and_then(|normalized| {
                        if !normalized.exists()? {
                            normalized.copy_to_destination()?;
                        }
                        normalized
                            .destination_relative_path
                            .into_os_string()
                            .into_string()
                            .map_err(|path| anyhow!("Path is not valid UTF8: {path:?}"))
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

    /// Converts an absolute path to a normalized form usable as a relative path within the destination directory.
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
                _ => Err(err),
            })
            .with_context(|| format!("Unable to canonicalize path: {}", path.display()))?;
        let destination_relative_path = absolute_path
            .strip_prefix(self.source_dir)
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
            destination_absolute_path: self.destination.join(&destination_relative_path),
            destination_relative_path,
        })
    }

    fn update_heading<'b>(
        &self,
        chapter: &Chapter,
        level: HeadingLevel,
        id: Option<&'b str>,
        mut classes: Vec<&'b str>,
    ) -> Option<(HeadingLevel, Option<&'b str>, Vec<&'b str>)> {
        const PANDOC_UNNUMBERED_CLASS: &str = "unnumbered";
        const PANDOC_UNLISTED_CLASS: &str = "unlisted";

        if !matches!(level, HeadingLevel::H1) {
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
}

impl Iterator for PreprocessedFiles<'_> {
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

impl PreprocessedFiles<'_> {
    pub fn output_dir(&self) -> &Path {
        &self.preprocessor.destination
    }

    fn preprocess_book_item(&mut self, item: &BookItem) -> anyhow::Result<Option<PathBuf>> {
        match item {
            BookItem::Chapter(chapter) => {
                let Some(chapter_path) = &chapter.source_path else {
                    return Ok(None);
                };
                let chapter_path = self.preprocessor.source_dir.join(chapter_path);
                let normalized = self.preprocessor.normalize_path(&chapter_path)?;
                let writer = io::BufWriter::new(normalized.create()?);
                self.preprocessor.preprocess_chapter(chapter, writer)?;
                Ok(Some(normalized.destination_relative_path))
            }
            BookItem::Separator => {
                log::warn!("Ignoring separator");
                Ok(None)
            }
            BookItem::PartTitle(name) => {
                if self.preprocessor.options.latex {
                    self.part_num += 1;
                    let kebab_case_name = name
                        .replace(|c: char| c.is_whitespace() || c == '_', "-")
                        .replace(|c: char| !(c.is_ascii_alphanumeric() || c == '-'), "");
                    let path =
                        PathBuf::from(format!("part-{}-{kebab_case_name}.md", self.part_num));
                    let mut file = File::options()
                        .write(true)
                        .create_new(true)
                        .open(self.preprocessor.destination.join(&path))
                        .with_context(|| format!("Unable to create file for part '{name}'"))?;
                    writeln!(file, r"`\part{{{name}}}`{{=latex}}")?;
                    Ok(Some(path))
                } else {
                    log::warn!("Ignoring part separator: {}", name);
                    Ok(None)
                }
            }
        }
    }
}

struct PreprocessChapter<'a> {
    preprocessor: &'a Preprocessor<'a>,
    chapter: &'a Chapter,
    parser: Peekable<pulldown_cmark::Parser<'a, 'a>>,
}

impl<'a> PreprocessChapter<'a> {
    fn new(preprocessor: &'a Preprocessor<'a>, chapter: &'a Chapter) -> Self {
        // Follow mdbook Commonmark extensions
        let options = markdown_extensions()
            .fold(pulldown_cmark::Options::empty(), |options, extension| {
                options | extension.pulldown
            });

        Self {
            preprocessor,
            chapter,
            parser: pulldown_cmark::Parser::new_ext(&chapter.content, options).peekable(),
        }
    }
}

impl<'a> Iterator for PreprocessChapter<'a> {
    type Item = pulldown_cmark::Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        use pulldown_cmark::{Event, Tag};

        Some(match self.parser.next()? {
            Event::Start(Tag::Heading(level, id, classes)) => {
                let tag = self
                    .preprocessor
                    .update_heading(self.chapter, level, id, classes)
                    .map(|(level, id, classes)| Tag::Heading(level, id, classes))
                    .unwrap_or(Tag::Paragraph);
                Event::Start(tag)
            }
            Event::End(Tag::Heading(level, id, classes)) => {
                let tag = self
                    .preprocessor
                    .update_heading(self.chapter, level, id, classes)
                    .map(|(level, id, classes)| Tag::Heading(level, id, classes))
                    .unwrap_or(Tag::Paragraph);
                Event::End(tag)
            }
            Event::Start(Tag::Link(link_ty, destination, title)) => {
                let destination = self
                    .preprocessor
                    .normalize_link_or_leave_as_is(self.chapter, destination);
                Event::Start(Tag::Link(link_ty, destination, title))
            }
            Event::End(Tag::Link(link_ty, destination, title)) => {
                let destination = self
                    .preprocessor
                    .normalize_link_or_leave_as_is(self.chapter, destination);
                Event::End(Tag::Link(link_ty, destination, title))
            }
            Event::Start(Tag::Image(link_ty, destination, title)) => {
                let destination = self
                    .preprocessor
                    .normalize_link_or_leave_as_is(self.chapter, destination);
                Event::Start(Tag::Image(link_ty, destination, title))
            }
            Event::End(Tag::Image(link_ty, destination, title)) => {
                let destination = self
                    .preprocessor
                    .normalize_link_or_leave_as_is(self.chapter, destination);
                Event::End(Tag::Image(link_ty, destination, title))
            }
            Event::Html(mut html) => {
                while let Some(Event::Html(more)) = self.parser.peek() {
                    let mut string = html.into_string();
                    string.push_str(more);
                    html = string.into();
                    // Actually consume the item from the iterator
                    self.parser.next();
                }
                if self.preprocessor.options.latex {
                    static FONT_AWESOME_ICON: Lazy<Regex> = Lazy::new(|| {
                        Regex::new(r#"<i\s+class\s*=\s*"fa fa-(?P<icon>.*?)"(>\s*</i>|/>)"#)
                            .unwrap()
                    });
                    html = match FONT_AWESOME_ICON.replace_all(&html, r"`\faicon{$icon}`{=latex}") {
                        Cow::Borrowed(_) => html,
                        Cow::Owned(html) => html.into(),
                    };
                }
                Event::Html(html)
            }
            event => event,
        })
    }
}

impl NormalizedPath {
    fn copy_to_destination(&self) -> anyhow::Result<()> {
        let path = &self.destination_absolute_path;
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
        self.destination_absolute_path.try_exists()
    }

    fn create(&self) -> anyhow::Result<File> {
        let path = &self.destination_absolute_path;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Unable to create directory: {}", parent.display()))?;
        }
        File::create(path).with_context(|| format!("Unable to create file: {}", path.display()))
    }
}
