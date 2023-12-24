use std::collections::{BTreeMap, BTreeSet};

use crate::{extensions::PandocExtension, PandocProfile};

pub struct Context {
    pub output: OutputFormat,
    pub pandoc: Pandoc,
}

pub struct Pandoc {
    pub version: semver::Version,
    pub extensions: BTreeMap<PandocExtension, Availability>,
}

pub enum Availability {
    Available,
    Unavailable(semver::VersionReq),
}

pub enum OutputFormat {
    Latex { packages: LatexPackages },
    Other,
}

#[derive(Default)]
pub struct LatexPackages {
    needed: BTreeSet<LatexPackage>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LatexPackage {
    FontAwesome,
}

impl LatexPackages {
    pub fn need(&mut self, package: LatexPackage) {
        self.needed.insert(package);
    }

    pub fn needed(&self) -> impl Iterator<Item = LatexPackage> + '_ {
        self.needed.iter().cloned()
    }
}

impl LatexPackage {
    pub fn name(&self) -> &str {
        match self {
            Self::FontAwesome => "fontawesome",
        }
    }
}

impl Context {
    pub(crate) fn new(profile: &PandocProfile, pandoc: Pandoc) -> Self {
        Self {
            output: if profile.uses_latex() {
                OutputFormat::Latex {
                    packages: Default::default(),
                }
            } else {
                OutputFormat::Other
            },
            pandoc,
        }
    }
}

impl Pandoc {
    pub fn new(version: semver::Version) -> Self {
        Self {
            extensions: [
                // Automatically generate section labels according to GitHub's method to
                // align with behavior of mdbook's HTML renderer
                PandocExtension::GfmAutoIdentifiers,
            ]
            .into_iter()
            .map(|extension| {
                let state = Availability::check(extension, &version);
                (extension, state)
            })
            .collect(),
            version,
        }
    }
}

impl Availability {
    fn check(extension: PandocExtension, pandoc: &semver::Version) -> Self {
        let version_req = extension.version_requirement();
        if version_req.matches(pandoc) {
            Availability::Available
        } else {
            Availability::Unavailable(version_req)
        }
    }
}

impl Pandoc {
    pub fn enable_extension(&mut self, extension: PandocExtension) -> &Availability {
        self.extensions
            .entry(extension)
            .or_insert_with(|| Availability::check(extension, &self.version))
    }

    pub fn enabled_extensions(
        &self,
    ) -> impl Iterator<Item = (&PandocExtension, &Availability)> + '_ {
        self.extensions.iter()
    }
}
