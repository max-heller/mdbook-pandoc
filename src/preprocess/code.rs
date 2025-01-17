use std::{borrow::Cow, iter, str};

use pulldown_cmark::CodeBlockKind;

use crate::CodeConfig;

pub enum CodeBlock<'book> {
    Rust,
    Other {
        language: Option<&'book str>,
        hidelines_prefix: Option<&'book str>,
    },
}

impl<'book> CodeBlock<'book> {
    pub fn new(kind: &'book CodeBlockKind<'_>, cfg: Option<&'book mdbook::config::Code>) -> Self {
        // MdBook supports custom attributes in code block info strings.
        // Attributes are separated by a comma, space, or tab from the language name.
        // See https://rust-lang.github.io/mdBook/format/mdbook.html#rust-code-block-attributes
        // This processes and strips out the attributes.
        let (language, mut attributes) = {
            let info_string = match kind {
                CodeBlockKind::Indented => "",
                CodeBlockKind::Fenced(info_string) => info_string,
            };
            let mut parts = info_string.split([',', ' ', '\t']).map(|part| part.trim());
            (parts.next(), parts)
        };

        match language {
            Some("rust") => Self::Rust,
            language => {
                let hidelines_override =
                    attributes.find_map(|attr| attr.strip_prefix("hidelines="));
                let hidelines_prefix = hidelines_override.or_else(|| {
                    // Respect [output.html.code.hidelines]
                    Some(cfg?.hidelines.get(language?)?.as_str())
                });
                Self::Other {
                    language,
                    hidelines_prefix,
                }
            }
        }
    }
}

impl CodeBlock<'_> {
    pub fn language(&self) -> Option<&str> {
        match self {
            Self::Rust => Some("rust"),
            Self::Other { language, .. } => *language,
        }
    }

    pub fn lines<'code>(
        &self,
        code: impl Iterator<Item = &'code str>,
        cfg: &CodeConfig,
    ) -> Vec<Cow<'code, str>> {
        /// Like [`str::Lines`] but yields [""] on ""
        enum Lines<'a> {
            One(iter::Once<&'a str>),
            Lines(str::Lines<'a>),
        }

        impl<'a> Iterator for Lines<'a> {
            type Item = &'a str;

            fn next(&mut self) -> Option<Self::Item> {
                match self {
                    Self::One(one) => one.next(),
                    Self::Lines(lines) => lines.next(),
                }
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                match self {
                    Self::One(one) => one.size_hint(),
                    Self::Lines(lines) => lines.size_hint(),
                }
            }
        }

        let lines = code.flat_map(|code| {
            if code.is_empty() {
                Lines::One(iter::once(code))
            } else {
                Lines::Lines(code.lines())
            }
        });

        // https://rust-lang.github.io/mdBook/format/mdbook.html#hiding-code-lines
        match self {
            Self::Rust => lines
                .filter_map(|line| Self::displayed_rust_line(line, cfg))
                .collect(),
            Self::Other {
                hidelines_prefix, ..
            } => {
                if let Some(prefix) = hidelines_prefix {
                    if cfg.show_hidden_lines {
                        lines
                            .map(|line| {
                                if let Some((prefix, suffix)) = line.split_once(prefix) {
                                    format!("{prefix}{suffix}").into()
                                } else {
                                    line.into()
                                }
                            })
                            .collect()
                    } else {
                        lines
                            .filter(|line| !line.trim_start().starts_with(prefix))
                            .map(Cow::Borrowed)
                            .collect()
                    }
                } else {
                    lines.map(Cow::Borrowed).collect()
                }
            }
        }
    }

    fn displayed_rust_line<'line>(line: &'line str, cfg: &CodeConfig) -> Option<Cow<'line, str>> {
        let Some(start) = line.find(|c: char| !c.is_whitespace()) else {
            return Some(line.into());
        };
        let (whitespace, trimmed) = line.split_at(start);
        let mut chars = trimmed.chars();
        match chars.next() {
            Some('#') => match chars.next() {
                // Two consecutive hashes override line hiding
                // https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html#hiding-portions-of-the-example
                Some('#') => Some(format!("{whitespace}#{}", chars.as_str()).into()),
                Some(' ') if cfg.show_hidden_lines => {
                    Some(format!("{whitespace}{}", chars.as_str()).into())
                }
                None if cfg.show_hidden_lines => Some(whitespace.into()),
                Some(' ') | None => None,
                Some(_) => Some(line.into()),
            },
            _ => Some(line.into()),
        }
    }
}
