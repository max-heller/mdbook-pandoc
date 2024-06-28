use super::Version;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Extension {
    Strikeout,
    Footnotes,
    PipeTables,
    TaskLists,
    Attributes,
    GfmAutoIdentifiers,
    RawAttribute,
    FencedDivs,
    // TODO: pandoc's `rebase_relative_paths` extension works for Markdown links and images,
    // but not for raw HTML links and images. Switch if/when pandoc supports HTML as well.
    /// Treat paths as relative to the chapter containing them
    #[allow(dead_code)]
    RebaseRelativePaths,
}

impl Extension {
    /// The name of the extension.
    pub const fn name(&self) -> &str {
        match self {
            Extension::Strikeout => "strikeout",
            Extension::Footnotes => "footnotes",
            Extension::PipeTables => "pipe_tables",
            Extension::TaskLists => "task_lists",
            Extension::Attributes => "attributes",
            Extension::GfmAutoIdentifiers => "gfm_auto_identifiers",
            Extension::RawAttribute => "raw_attribute",
            Extension::FencedDivs => "fenced_divs",
            Extension::RebaseRelativePaths => "rebase_relative_paths",
        }
    }

    /// Returns the pandoc version that added support for this extension.
    fn introduced_in(&self) -> Version {
        let (major, minor, patch) = match self {
            Extension::Strikeout => (0, 10, 0),
            Extension::Footnotes => (2, 10, 1),
            Extension::PipeTables => (0, 10, 0),
            Extension::TaskLists => (2, 6, 0),
            Extension::Attributes => (2, 10, 1),
            Extension::GfmAutoIdentifiers => (2, 0, 0),
            Extension::RawAttribute => (2, 10, 1),
            Extension::FencedDivs => (2, 0, 0),
            Extension::RebaseRelativePaths => (2, 14, 0),
        };
        Version {
            major,
            minor,
            patch,
        }
    }

    pub fn check_availability(&self, pandoc: &Version) -> Availability {
        let introduced_in = self.introduced_in();
        if *pandoc >= introduced_in {
            Availability::Available
        } else {
            Availability::Unavailable { introduced_in }
        }
    }
}

pub enum Availability {
    Available,
    Unavailable { introduced_in: Version },
}

impl Availability {
    pub fn is_available(&self) -> bool {
        match self {
            Self::Available => true,
            Self::Unavailable { .. } => false,
        }
    }
}
