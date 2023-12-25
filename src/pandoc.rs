use std::collections::BTreeMap;

use once_cell::sync::Lazy;

pub mod extension;
pub use extension::Extension;

mod profile;
pub use profile::Profile;

mod renderer;
pub use renderer::{Context as RenderContext, OutputFormat, Renderer};

/// Defines compatible versions of Pandoc
pub static VERSION_REQ: Lazy<semver::VersionReq> =
    // commonmark input format introduced in 1.14
    Lazy::new(|| semver::VersionReq::parse(">=1.14").unwrap());

pub struct Context {
    pub version: semver::Version,
    enabled_extensions: BTreeMap<Extension, extension::Availability>,
}

impl Context {
    pub fn new(version: semver::Version) -> Self {
        let mut this = Self {
            enabled_extensions: Default::default(),
            version,
        };
        // Automatically generate section labels according to GitHub's method to
        // align with behavior of mdbook's HTML renderer
        this.enable_extension(Extension::GfmAutoIdentifiers);
        this
    }

    pub fn enable_extension(&mut self, extension: Extension) -> &extension::Availability {
        self.enabled_extensions
            .entry(extension)
            .or_insert_with(|| extension.check_availability(&self.version))
    }

    pub fn enabled_extensions(
        &self,
    ) -> impl Iterator<Item = (&Extension, &extension::Availability)> + '_ {
        self.enabled_extensions.iter()
    }
}
