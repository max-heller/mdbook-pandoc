#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PandocExtension {
    Strikeout,
    Footnotes,
    PipeTables,
    TaskLists,
    Attributes,
    GfmAutoIdentifiers,
    RawAttribute,
    // TODO: pandoc's `rebase_relative_paths` extension works for Markdown links and images,
    // but not for raw HTML links and images. Switch if/when pandoc supports HTML as well.
    /// Treat paths as relative to the chapter containing them
    #[allow(dead_code)]
    RebaseRelativePaths,
}

impl PandocExtension {
    pub const fn name(&self) -> &str {
        match self {
            PandocExtension::Strikeout => "strikeout",
            PandocExtension::Footnotes => "footnotes",
            PandocExtension::PipeTables => "pipe_tables",
            PandocExtension::TaskLists => "task_lists",
            PandocExtension::Attributes => "attributes",
            PandocExtension::GfmAutoIdentifiers => "gfm_auto_identifiers",
            PandocExtension::RawAttribute => "raw_attribute",
            PandocExtension::RebaseRelativePaths => "rebase_relative_paths",
        }
    }

    pub fn version_requirement(&self) -> semver::VersionReq {
        let (major, minor, patch) = match self {
            PandocExtension::Strikeout => (0, 10, 0),
            PandocExtension::Footnotes => (2, 10, 1),
            PandocExtension::PipeTables => (0, 10, 0),
            PandocExtension::TaskLists => (2, 6, 0),
            PandocExtension::Attributes => (2, 10, 1),
            PandocExtension::GfmAutoIdentifiers => (2, 0, 0),
            PandocExtension::RawAttribute => (2, 10, 1),
            PandocExtension::RebaseRelativePaths => (2, 14, 0),
        };
        semver::VersionReq {
            comparators: vec![semver::Comparator {
                op: semver::Op::GreaterEq,
                major,
                minor: Some(minor),
                patch: Some(patch),
                pre: semver::Prerelease::EMPTY,
            }],
        }
    }
}

/// Markdown extensions enabled by mdBook.
///
/// See https://rust-lang.github.io/mdBook/format/markdown.html#extensions
pub fn mdbook_extensions() -> impl Iterator<Item = (pulldown_cmark::Options, PandocExtension)> {
    use pulldown_cmark::Options;
    [
        // TODO: pandoc requires ~~, but commonmark's extension allows ~ or ~~.
        // pulldown_cmark_to_cmark always generates ~~, so this is okay,
        // although it'd be good to have an option to configure this explicitly.
        (Options::ENABLE_STRIKETHROUGH, PandocExtension::Strikeout),
        (Options::ENABLE_FOOTNOTES, PandocExtension::Footnotes),
        (Options::ENABLE_TABLES, PandocExtension::PipeTables),
        (Options::ENABLE_TASKLISTS, PandocExtension::TaskLists),
        // pandoc does not support `header_attributes` with commonmark
        // so use `attributes`, which is a superset
        (
            Options::ENABLE_HEADING_ATTRIBUTES,
            PandocExtension::Attributes,
        ),
    ]
    .into_iter()
}
