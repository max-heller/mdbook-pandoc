use std::collections::BTreeSet;

use once_cell::sync::Lazy;
use regex::Regex;

/// Commands that define new macros, as supported by MathJax:
/// <https://docs.mathjax.org/en/latest/input/tex/macros.html>
pub static MACRO_DEFINITION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        &[
            // \(re)newcommand
            r"\\(?P<newcommand>(re)?newcommand) *(\\\w+|\{\\\w+\}) *(\[\d+\])* *\{.+\}",
            // \def
            r"\\def *\\\w+ *\{.+\}",
            // \let
            r"\\let *\\\w+ *=? *(.|\\\w+)",
        ]
        .join("|"),
    )
    .unwrap()
});

#[derive(Clone, Copy, Debug)]
pub enum MathType {
    Display,
    Inline,
}

#[derive(Debug, Default)]
pub struct Packages {
    needed: BTreeSet<Package>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Package {
    FontAwesome,
    EnumItem,
}

impl Packages {
    pub fn need(&mut self, package: Package) {
        self.needed.insert(package);
    }

    pub fn needed(&self) -> impl Iterator<Item = Package> + '_ {
        self.needed.iter().cloned()
    }
}

impl Package {
    pub fn name(&self) -> &str {
        match self {
            Self::FontAwesome => "fontawesome",
            Self::EnumItem => "enumitem",
        }
    }
}
