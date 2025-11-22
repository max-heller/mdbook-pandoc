use std::{borrow::Cow, iter, str};

use pulldown_cmark::CodeBlockKind;

use crate::{pandoc, CodeConfig};

pub struct CodeBlock<'book> {
    pub language: Language<'book>,
    attributes: Vec<&'book str>,
}

pub enum Language<'book> {
    Rust,
    Other {
        language: Option<&'book str>,
        hidelines_prefix: Option<&'book str>,
    },
}

impl<'book> CodeBlock<'book> {
    pub fn new(kind: &'book CodeBlockKind<'_>, cfg: &'book mdbook::config::Code) -> Self {
        // MdBook supports custom attributes in code block info strings.
        // Attributes are separated by a comma, space, or tab from the language name.
        // See https://rust-lang.github.io/mdBook/format/mdbook.html#rust-code-block-attributes
        // This processes and strips out the attributes.
        let (language, attributes) = {
            let info_string = match kind {
                CodeBlockKind::Indented => "",
                CodeBlockKind::Fenced(info_string) => info_string,
            };
            let mut parts = info_string.split([',', ' ', '\t']).map(|part| part.trim());
            (parts.next(), parts)
        };

        let mut hidelines_override = None;
        let attributes = attributes
            .filter(|attr| {
                if let Some(hidelines) = attr.strip_prefix("hidelines=") {
                    hidelines_override = Some(hidelines);
                    false
                } else {
                    true
                }
            })
            .collect();

        let language = match language {
            Some("rust") => Language::Rust,
            language => {
                let hidelines_prefix = hidelines_override.or_else(|| {
                    // Respect [output.html.code.hidelines]
                    Some(cfg.hidelines.get(language?)?.as_str())
                });
                Language::Other {
                    language,
                    hidelines_prefix,
                }
            }
        };

        Self {
            language,
            attributes,
        }
    }
}

impl CodeBlock<'_> {
    pub fn language(&self) -> Option<&str> {
        match self.language {
            Language::Rust => Some("rust"),
            Language::Other { language, .. } => language,
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
        match self.language {
            Language::Rust => lines
                .filter_map(|line| Self::displayed_rust_line(line, cfg))
                .collect(),
            Language::Other {
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

impl pandoc::native::Attributes for CodeBlock<'_> {
    fn id(&self) -> Option<&str> {
        None
    }

    fn classes(&self) -> impl Iterator<Item = &str> {
        self.language()
            .into_iter()
            .chain(self.attributes.iter().copied())
    }

    fn attrs(&self) -> impl Iterator<Item = (&str, &str)> {
        std::iter::empty()
    }
}
