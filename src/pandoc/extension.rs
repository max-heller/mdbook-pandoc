#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Extension {
    Strikeout,
    Footnotes,
    PipeTables,
    TaskLists,
    Attributes,
    AutoIdentifiers,
    GfmAutoIdentifiers,
    RawAttribute,
    DefinitionLists,
    FencedDivs,
    // TODO: pandoc's `rebase_relative_paths` extension works for Markdown links and images,
    // but not for raw HTML links and images. Switch if/when pandoc supports HTML as well.
    /// Treat paths as relative to the chapter containing them
    #[allow(dead_code)]
    RebaseRelativePaths,
}

impl Extension {
    pub const fn name(&self) -> &str {
        match self {
            Extension::Strikeout => "strikeout",
            Extension::Footnotes => "footnotes",
            Extension::PipeTables => "pipe_tables",
            Extension::TaskLists => "task_lists",
            Extension::Attributes => "attributes",
            Extension::AutoIdentifiers => "auto_identifiers",
            Extension::GfmAutoIdentifiers => "gfm_auto_identifiers",
            Extension::RawAttribute => "raw_attribute",
            Extension::DefinitionLists => "definition_lists",
            Extension::FencedDivs => "fenced_divs",
            Extension::RebaseRelativePaths => "rebase_relative_paths",
        }
    }

    fn version_requirement(&self) -> semver::VersionReq {
        let (major, minor, patch) = match self {
            Extension::Strikeout => (0, 10, 0),
            Extension::Footnotes => (2, 10, 1),
            Extension::PipeTables => (0, 10, 0),
            Extension::TaskLists => (2, 6, 0),
            Extension::Attributes => (2, 10, 1),
            Extension::AutoIdentifiers => (0, 10, 0),
            Extension::GfmAutoIdentifiers => (2, 0, 0),
            Extension::RawAttribute => (2, 10, 1),
            Extension::DefinitionLists => (0, 10, 0),
            Extension::FencedDivs => (2, 0, 0),
            Extension::RebaseRelativePaths => (2, 14, 0),
        };
        semver::VersionReq {
            comparators: vec![semver::Comparator {
                // Assumes that pandoc doesn't remove extensions once it has added them
                op: semver::Op::GreaterEq,
                major,
                minor: Some(minor),
                patch: Some(patch),
                pre: semver::Prerelease::EMPTY,
            }],
        }
    }

    pub fn check_availability(&self, pandoc: &semver::Version) -> Availability {
        let version_req = self.version_requirement();
        if version_req.matches(pandoc) {
            Availability::Available
        } else {
            Availability::Unavailable(version_req)
        }
    }
}

pub enum Availability {
    Available,
    Unavailable(semver::VersionReq),
}

impl Availability {
    pub fn is_available(&self) -> bool {
        match self {
            Self::Available => true,
            Self::Unavailable(_) => false,
        }
    }
}
