use std::{fmt, num::ParseIntError, process::Command, str::FromStr};

use anyhow::{anyhow, Context as _};

mod profile;
pub use profile::Profile;

mod renderer;
pub use renderer::{Context as RenderContext, OutputFormat, Renderer};

pub mod native;

/// Minimum compatible version of Pandoc
const MINIMUM_VERSION: Version =
    // Defaults files introduced in 2.8
    Version {
        major: 2,
        minor: 8,
        patch: 0,
    };

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for Version {
    type Err = ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut components = s.split('.').map(|component| component.parse());
        let mut next_component = || components.next().unwrap_or(Ok(0));
        Ok(Self {
            major: next_component()?,
            minor: next_component()?,
            patch: next_component()?,
        })
    }
}

pub fn check_compatibility() -> anyhow::Result<Version> {
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
            Some((_, version)) => version
                .trim()
                .parse()
                .with_context(|| format!("failed to parse Pandoc version '{version}'"))?,
            _ => anyhow::bail!("`pandoc -v` output does not contain `pandoc VERSION`"),
        }
    };
    if version >= MINIMUM_VERSION {
        Ok(version)
    } else {
        Err(anyhow!(
            "mdbook-pandoc is incompatible with detected Pandoc version \
            (requires at least {MINIMUM_VERSION}, but using {version})"
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    use super::*;

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
        insta::assert_snapshot!(stderr, @r#"
        Error producing PDF.
        ! LaTeX Error: Too deeply nested.

        See the LaTeX manual or LaTeX Companion for explanation.
        Type  H <return>  for immediate help.
         ...                                              
                                                          
        l.74         \begin{itemize}

        "#);
    }

    #[test]
    fn versions() {
        let a = Version::from_str("2.10").unwrap();
        let b = Version::from_str("2.12.1").unwrap();
        let c = Version::from_str("3.1.12.3").unwrap();
        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
    }
}
