use std::{collections::BTreeMap, process::Command};

use anyhow::{anyhow, Context as _};
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

pub fn check_compatibility() -> anyhow::Result<semver::Version> {
    let version = {
        let output = Command::new("pandoc")
            .arg("-v")
            .output()
            .context("Unable to run `pandoc -v`")?;
        anyhow::ensure!(
            output.status.success(),
            "`pandoc -v` exited with error code {}",
            output.status
        );
        let output = String::from_utf8(output.stdout).context("`pandoc -v` output is not UTF8")?;
        match output.lines().next().and_then(|line| line.split_once(' ')) {
            Some(("pandoc", mut version)) => {
                // Pandoc versions can contain more than three components (e.g. a.b.c.d).
                // If this is the case, only consider the first three.
                if let Some((idx, _)) = version.match_indices('.').nth(2) {
                    version = &version[..idx];
                }
                semver::Version::parse(version.trim()).unwrap()
            }
            _ => anyhow::bail!("`pandoc -v` output does not contain `pandoc VERSION`"),
        }
    };
    if VERSION_REQ.matches(&version) {
        Ok(version)
    } else {
        Err(anyhow!(
            "mdbook-pandoc is incompatible with detected Pandoc version (requires version {}, but using {})",
            *VERSION_REQ, version,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    // Canary to detect if Pandoc ever adds native support for lists
    // nested more than four layers deep when rendering to LaTeX
    #[test]
    fn five_item_deep_list() {
        let mut pandoc = Command::new("pandoc")
            .args(["-t", "pdf", "-o", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = pandoc.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "
- one
    - two
        - three
            - four
                - five
            "
        )
        .unwrap();
        let output = pandoc.wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        insta::assert_snapshot!(stderr, @r###"
        Error producing PDF.
        ! LaTeX Error: Too deeply nested.

        See the LaTeX manual or LaTeX Companion for explanation.
        Type  H <return>  for immediate help.
         ...                                              
                                                          
        l.78         \begin{itemize}

        "###);
    }
}
