use std::{
    borrow::Cow,
    collections::HashMap,
    fmt,
    io::{self, Write},
};

use aho_corasick::AhoCorasick;
use anyhow::Context;
use ego_tree::{NodeId, NodeRef};
use html5ever::{
    expanded_name, local_name, namespace_url, ns,
    serialize::Serializer,
    tendril::{fmt::UTF8, format_tendril, StrTendril, Tendril, TendrilSink},
    LocalName,
};
use indexmap::IndexSet;
use pulldown_cmark::{CowStr, LinkType};

use crate::{html, latex, pandoc, preprocess::UnresolvableRemoteImage};

mod node;
pub use node::{Attributes, Element, MdElement, Node, QualNameExt};

mod sink;
pub use sink::HtmlTreeSink;

use super::code;

#[derive(Debug)]
pub struct Tree<'book> {
    errors: Vec<Cow<'static, str>>,
    pub tree: ego_tree::Tree<Node<'book>>,
}

pub struct TreeBuilder<'book> {
    pub html: html::Parser<'book>,
    footnotes: HashMap<CowStr<'book>, NodeId>,
}

pub struct Emitter<'book> {
    tree: Tree<'book>,
    footnotes: HashMap<CowStr<'book>, NodeId>,
}

impl Tree<'_> {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            tree: ego_tree::Tree::new(Node::Document),
        }
    }
}

impl<'book> TreeBuilder<'book> {
    pub fn new() -> Self {
        let html_parser = {
            let mut opts = html5ever::ParseOpts::default();
            // If this is enabled (the default) then the contents of <noscript> elements get parsed
            // as text, which doesn't play nice with the assumptions that the tree builder makes
            // about the creation of new elements when HTML tags are parsed.
            opts.tree_builder.scripting_enabled = false;
            html5ever::driver::parse_fragment(
                HtmlTreeSink::new(),
                opts,
                html::name!(html "body"),
                Vec::new(),
            )
        };
        Self {
            html: html_parser,
            footnotes: Default::default(),
        }
    }

    fn create_element_inner(&mut self, html: StrTendril) -> anyhow::Result<NodeId> {
        self.html.process(html.clone());
        let sink = &self.html.tokenizer.sink.sink;
        sink.most_recently_created_element.take().with_context(|| {
            format!("parsing HTML {html} did not result in the creation of a new element")
        })
    }

    pub fn create_element(&mut self, element: MdElement<'book>) -> anyhow::Result<NodeId> {
        let tag = format_tendril!("<{}>", element.name().local);
        let id = self.create_element_inner(tag)?;
        let mut tree = self.html.tokenizer.sink.sink.tree.borrow_mut();
        *tree.tree.get_mut(id).unwrap().value() = Node::Element(Element::Markdown(element));
        Ok(id)
    }

    pub fn create_html_element(&mut self, name: LocalName) -> anyhow::Result<NodeId> {
        self.create_element_inner(format_tendril!("<{}>", name))
    }

    pub fn process_html(&mut self, html: Tendril<UTF8>) {
        self.html.process(html);
        let sink = &self.html.tokenizer.sink.sink;
        sink.most_recently_created_element.take();
    }

    pub fn footnote(&mut self, label: CowStr<'book>, node: NodeId) {
        self.footnotes.insert(label, node);
    }

    pub fn finish(self) -> Emitter<'book> {
        Emitter {
            tree: self.html.finish(),
            footnotes: self.footnotes,
        }
    }
}

impl<'book> Emitter<'book> {
    pub fn serialize_children<'event>(
        &self,
        node: NodeRef<'_, Node>,
        serializer: &mut pandoc::native::SerializeNested<'_, '_, 'book, '_, impl io::Write>,
    ) -> anyhow::Result<()>
    where
        'book: 'event,
    {
        for node in node.children() {
            self.serialize_node(node, serializer)?;
        }
        Ok(())
    }

    pub fn serialize_node(
        &self,
        node: NodeRef<'_, Node>,
        serializer: &mut pandoc::native::SerializeNested<'_, '_, 'book, '_, impl io::Write>,
    ) -> anyhow::Result<()> {
        log::trace!("Writing Pandoc AST for {:?}", node.value());
        match node.value() {
            Node::Document => unreachable!(),
            Node::HtmlComment(comment) => {
                serializer.serialize_raw_html(|serializer| serializer.write_comment(comment))
            }
            Node::HtmlText(text) => {
                if matches!(
                    serializer.preprocessor().preprocessor.ctx.output,
                    pandoc::OutputFormat::HtmlLike
                ) {
                    serializer.serialize_raw_html(|serializer| serializer.write_text(text))
                } else {
                    serializer.serialize_inlines(|inlines| {
                        inlines.serialize_element()?.serialize_str(text)
                    })
                }
            }
            Node::Element(Element::Markdown(element)) => match element {
                MdElement::Paragraph => {
                    serializer
                        .blocks()?
                        .serialize_element()?
                        .serialize_para(|serializer| {
                            serializer.serialize_nested(|serializer| {
                                for node in node.children() {
                                    self.serialize_node(node, serializer)?;
                                }
                                Ok(())
                            })
                        })
                }
                MdElement::Text(text) => serializer
                    .serialize_inlines(|inlines| inlines.serialize_element()?.serialize_str(text)),
                MdElement::SoftBreak => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_soft_break()
                }),
                MdElement::Heading {
                    level,
                    id,
                    classes,
                    attrs,
                } => serializer.blocks()?.serialize_element()?.serialize_header(
                    *level as usize,
                    (id.as_deref(), classes, attrs),
                    |inlines| {
                        inlines.serialize_nested(|serializer| {
                            for node in node.children() {
                                self.serialize_node(node, serializer)?;
                            }
                            Ok(())
                        })
                    },
                ),
                MdElement::List(None) => serializer
                    .blocks()?
                    .serialize_element()?
                    .serialize_bullet_list(|items| {
                        for child in node.children() {
                            let mut item = items.serialize_element()??;
                            item.serialize_nested(|item| {
                                for node in child.children() {
                                    self.serialize_node(node, item)?;
                                }
                                Ok(())
                            })?;
                            item.finish()?;
                        }
                        Ok(())
                    }),
                MdElement::List(Some(first)) => serializer
                    .blocks()?
                    .serialize_element()?
                    .serialize_ordered_list(*first, |items| {
                        for child in node.children() {
                            let mut item = items.serialize_element()??;
                            item.serialize_nested(|item| {
                                for node in child.children() {
                                    self.serialize_node(node, item)?;
                                }
                                Ok(())
                            })?;
                            item.finish()?;
                        }
                        Ok(())
                    }),
                MdElement::Item => self.serialize_children(node, serializer),
                MdElement::TaskListMarker(checked) => serializer.serialize_inlines(|inlines| {
                    inlines
                        .serialize_element()?
                        .serialize_str_unescaped(if *checked { "\\9746" } else { "\\9744" })?;
                    inlines.serialize_element()?.serialize_space()
                }),
                MdElement::Link { dest_url, title } => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_link(
                        (None, &[], &[]),
                        |alt| alt.serialize_nested(|alt| self.serialize_children(node, alt)),
                        dest_url,
                        title,
                    )
                }),
                MdElement::Table { alignment, source } => {
                    let preprocessor = serializer.preprocessor();
                    let column_widths = preprocessor.column_widths(source);
                    let mut children = node.children();
                    let (head, body) = (children.next().unwrap(), children.next().unwrap());
                    debug_assert!(children.next().is_none());

                    let thead = match head.value() {
                        Node::Element(Element::Html(element))
                            if element.name.expanded() == expanded_name!(html "thead") =>
                        {
                            element
                        }
                        event => anyhow::bail!("expected table head, got {event:?}"),
                    };
                    let tbody = match body.value() {
                        Node::Element(Element::Html(element))
                            if element.name.expanded() == expanded_name!(html "tbody") =>
                        {
                            element
                        }
                        event => anyhow::bail!("expected table body, got {event:?}"),
                    };

                    serializer.blocks()?.serialize_element()?.serialize_table(
                        (),
                        (alignment.iter().copied().map(Into::into)).zip(column_widths),
                        (&thead.attrs, |serializer| {
                            for row in head.children() {
                                match row.value() {
                                    Node::Element(Element::Html(element)) if element.name.expanded() == expanded_name!(html "tr") => {
                                        serializer.serialize_element()?.serialize_row(&element.attrs, |cells| {
                                            for cell in row.children() {
                                                match cell.value() {
                                                    Node::Element(Element::Html(element)) if element.name.expanded() == expanded_name!(html "th") => {
                                                        for node in cell.children() {
                                                            cells.serialize_element()?.serialize_cell(
                                                                &element.attrs,
                                                                |blocks| {
                                                                    blocks.serialize_nested(|serializer| {
                                                                        self.serialize_node(
                                                                            node, serializer,
                                                                        )
                                                                    })
                                                                },
                                                            )?;
                                                        }
                                                    }
                                                    event => {
                                                        anyhow::bail!("expected table cell, got {event:?}")
                                                    }
                                                }
                                            }
                                            Ok(())
                                        })?
                                    }
                                    event => anyhow::bail!("expected table row, got {event:?}"),
                                }
                            }
                            Ok(())
                        }),
                        (&tbody.attrs, |serializer| {
                            for row in body.children() {
                                match row.value() {
                                    Node::Element(Element::Html(element))
                                        if element.name.expanded() == expanded_name!(html "tr") =>
                                    {
                                        serializer.serialize_element()?.serialize_row(
                                            &element.attrs,
                                            |cells| {
                                                for cell in row.children() {
                                                    match cell.value() {
                                                        Node::Element(Element::Html(element))
                                                            if element.name.expanded()
                                                                == expanded_name!(html "td") =>
                                                        {
                                                            cells
                                                                .serialize_element()?
                                                                .serialize_cell(&element.attrs, |blocks| {
                                                                    blocks.serialize_nested(
                                                                        |serializer| {
                                                                            for node in
                                                                                cell.children()
                                                                            {
                                                                                self.serialize_node(
                                                                                node, serializer,
                                                                            )?;
                                                                            }
                                                                            Ok(())
                                                                        },
                                                                    )
                                                                })?
                                                        }
                                                        event => {
                                                            anyhow::bail!(
                                                                "expected table data (<td>), got {event:?}"
                                                            )
                                                        }
                                                    }
                                                }
                                                Ok(())
                                            },
                                        )?
                                    }
                                    event => anyhow::bail!("expected table row, got {event:?}"),
                                }
                            }
                            Ok(())
                        }),
                    )
                }
                MdElement::FootnoteDefinition => Ok(()),
                MdElement::FootnoteReference(label) => match self.footnotes.get(label) {
                    None => {
                        log::warn!("Undefined footnote: {label}");
                        Ok(())
                    }
                    Some(definition) => {
                        let open_footnotes = &mut serializer.serializer().footnotes;
                        if open_footnotes.contains(label.as_ref()) {
                            log::warn!(
                                "Cycle in footnote definitions: {:?}",
                                FootnoteCycle(&serializer.serializer().footnotes, label)
                            );
                            Ok(())
                        } else {
                            open_footnotes.insert(label.to_string());
                            serializer.serialize_inlines(|serializer| {
                                serializer
                                    .serialize_element()?
                                    .serialize_note(|serializer| {
                                        serializer.serialize_nested(|serializer| {
                                            for node in
                                                self.tree.tree.get(*definition).unwrap().children()
                                            {
                                                self.serialize_node(node, serializer)?;
                                            }
                                            Ok(())
                                        })
                                    })
                            })?;
                            serializer.serializer().footnotes.pop();
                            Ok(())
                        }
                    }
                },
                MdElement::BlockQuote => serializer
                    .blocks()?
                    .serialize_element()?
                    .serialize_block_quote(|blocks| {
                        blocks.serialize_nested(|serializer| {
                            for node in node.children() {
                                self.serialize_node(node, serializer)?;
                            }
                            Ok(())
                        })
                    }),
                MdElement::InlineCode(s) => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_code((), s)
                }),
                MdElement::CodeBlock(kind) => {
                    let ctx = &serializer.preprocessor().preprocessor.ctx;

                    let code_block = code::CodeBlock::new(kind, ctx.html.map(|cfg| &cfg.code));

                    let lines = node.children().map(|node| {
                        match node.value() {
                            Node::Element(Element::Markdown(MdElement::Text(text))) => text,
                            event => panic!("Code blocks should contain only literal text, but encountered {event:?}"),
                        }
                    }).flat_map(|text| text.lines());
                    let lines = code_block.lines(lines, ctx.code);

                    let mut language = code_block.language();

                    if let pandoc::OutputFormat::Latex { .. } =
                        serializer.preprocessor().preprocessor.ctx.output
                    {
                        const CODE_BLOCK_LINE_LENGTH_LIMIT: usize = 1000;

                        // Pandoc+fvextra only wraps long lines in code blocks with info strings
                        // so fall back to "text"
                        language = language.or(Some("text"));

                        let overly_long_line = lines
                            .iter()
                            .any(|line| line.len() > CODE_BLOCK_LINE_LENGTH_LIMIT);
                        if overly_long_line {
                            let lines = {
                                let patterns = &[r"\", "{", "}", "$", "_", "^", "&", "]"];
                                let replace_with = &[
                                    r"\textbackslash{}",
                                    r"\{",
                                    r"\}",
                                    r"\$",
                                    r"\_",
                                    r"\^",
                                    r"\&",
                                    r"{{]}}",
                                ];
                                let ac = AhoCorasick::new(patterns).unwrap();
                                lines
                                    .into_iter()
                                    .map(move |line| ac.replace_all(&line, replace_with))
                            };
                            return serializer
                                .blocks()?
                                .serialize_element()?
                                .serialize_raw_block("latex", |raw| {
                                    for line in lines {
                                        raw.serialize_code(r"\texttt{{")?;
                                        raw.serialize_code(&line)?;
                                        raw.serialize_code(r"}}\\")?;
                                    }
                                    Ok(())
                                });
                        }
                    }

                    let language = language.map(CowStr::Borrowed);
                    let classes = language.as_slice();
                    serializer
                        .blocks()?
                        .serialize_element()?
                        .serialize_code_block((None, &classes, &[]), |code| {
                            for line in lines {
                                code.serialize_code(&line)?;
                                code.serialize_code("\n")?;
                            }
                            Ok(())
                        })
                }
                MdElement::Emphasis => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_emph(|inlines| {
                        inlines.serialize_nested(|serializer| {
                            self.serialize_children(node, serializer)
                        })
                    })
                }),
                MdElement::Strong => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_strong(|inlines| {
                        inlines.serialize_nested(|serializer| {
                            self.serialize_children(node, serializer)
                        })
                    })
                }),
                MdElement::Strikethrough => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_strikeout(|inlines| {
                        inlines.serialize_nested(|serializer| {
                            self.serialize_children(node, serializer)
                        })
                    })
                }),
                MdElement::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                } => serializer.serialize_inlines(|inlines| {
                    match inlines
                        .serializer
                        .preprocessor
                        .resolve_image_url(dest_url.as_ref().into(), *link_type)
                    {
                        Err(UnresolvableRemoteImage) => inlines
                            .serialize_nested(|inlines| self.serialize_children(node, inlines)),
                        Ok(dest_url) => inlines.serialize_element()?.serialize_image(
                            (Some(id.as_ref()), &[], &[]),
                            |alt| alt.serialize_nested(|alt| self.serialize_children(node, alt)),
                            &dest_url,
                            title,
                        ),
                    }
                }),
            },
            Node::Element(Element::Html(element)) => {
                match element.name.local {
                    local_name!("thead")
                    | local_name!("th")
                    | local_name!("tr")
                    | local_name!("td") => return self.serialize_children(node, serializer),
                    local_name!("br") => {
                        return serializer.serialize_inlines(|inlines| {
                            inlines.serialize_element()?.serialize_line_break()
                        })
                    }
                    local_name!("hr") => {
                        return serializer
                            .blocks()?
                            .serialize_element()?
                            .serialize_horizontal_rule()
                    }
                    local_name!("a") => {
                        let [href, title] = [html::name!("href"), html::name!("title")]
                            .map(|attr| element.attrs.rest.get(&attr));
                        return serializer.serialize_inlines(|inlines| {
                            if let Some(href) = href {
                                inlines.serialize_element()?.serialize_link(
                                    &element.attrs,
                                    |alt| {
                                        alt.serialize_nested(|alt| {
                                            self.serialize_children(node, alt)
                                        })
                                    },
                                    href,
                                    title.as_ref().map_or("", |s| s.as_ref()),
                                )
                            } else {
                                inlines.serialize_element()?.serialize_span(
                                    &element.attrs,
                                    |inlines| {
                                        inlines.serialize_nested(|serializer| {
                                            self.serialize_children(node, serializer)
                                        })
                                    },
                                )
                            }
                        });
                    }
                    local_name!("span") => {
                        return serializer.serialize_inlines(|inlines| {
                            inlines
                                .serialize_element()?
                                .serialize_span(&element.attrs, |inlines| {
                                    inlines.serialize_nested(|serializer| {
                                        self.serialize_children(node, serializer)
                                    })
                                })
                        })
                    }
                    local_name!("div") => {
                        return serializer.blocks()?.serialize_element()?.serialize_div(
                            &element.attrs,
                            |blocks| {
                                blocks.serialize_nested(|serializer| {
                                    self.serialize_children(node, serializer)
                                })
                            },
                        );
                    }
                    local_name!("img") => {
                        let mut attrs = element.attrs.clone();
                        let [src, alt, title] =
                            [html::name!("src"), html::name!("alt"), html::name!("title")]
                                .map(|attr| attrs.rest.swap_remove(&attr));
                        let Some(src) = src else { return Ok(()) };
                        return match serializer
                            .preprocessor()
                            .resolve_image_url(src.as_ref().into(), LinkType::Inline)
                        {
                            Err(UnresolvableRemoteImage) => match alt {
                                Some(alt) => serializer.serialize_inlines(|serializer| {
                                    serializer.serialize_element()?.serialize_str(&alt)
                                }),
                                None => Ok(()),
                            },
                            Ok(src) => serializer.serialize_inlines(|inlines| {
                                inlines.serialize_element()?.serialize_image(
                                    &attrs,
                                    |serializer| match alt {
                                        Some(alt) => {
                                            serializer.serialize_element()?.serialize_str(&alt)
                                        }
                                        None => Ok(()),
                                    },
                                    &src,
                                    title.as_ref().map_or("", |s| s.as_ref()),
                                )
                            }),
                        };
                    }
                    local_name!("i") => {
                        let Attributes { id, classes, rest } = &element.attrs;
                        if id.is_none() && rest.is_empty() {
                            if let Some(icon) = classes.strip_prefix("fa fa-") {
                                let ctx = &mut serializer.preprocessor().preprocessor.ctx;
                                if let pandoc::OutputFormat::Latex { packages } = &mut ctx.output {
                                    if !node.has_children() {
                                        packages.need(latex::Package::FontAwesome);
                                        return serializer.serialize_inlines(|inlines| {
                                            inlines
                                                .serialize_element()?
                                                .serialize_raw_inline("latex", |raw| {
                                                    write!(raw, r"\faicon{{{icon}}}")
                                                })
                                        });
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
                serializer.serialize_raw_html(|serializer| {
                    serializer.start_elem(
                        element.name.clone(),
                        element.attrs.iter().map(|(attr, val)| (attr, val.as_ref())),
                    )
                })?;
                // Wrap children in a span or div to ensure structure of HTML tree is carried into
                // the pandoc AST.
                // If the format strips raw HTML and the tag contains an `id`, move
                // the id to the wrapper so links to it don't break.
                let id = (!matches!(
                    serializer.preprocessor().preprocessor.ctx.output,
                    pandoc::OutputFormat::HtmlLike
                ))
                .then_some(element.attrs.id.as_ref())
                .flatten()
                .map(|s| s.as_ref());
                if node.has_children() || id.is_some() {
                    let attrs = (id, &[], &[]);
                    if serializer.is_blocks() {
                        if element.name.is_display_block() {
                            serializer.blocks()?.serialize_element()?.serialize_div(
                                attrs,
                                |serializer| {
                                    serializer.serialize_nested(|serializer| {
                                        self.serialize_children(node, serializer)
                                    })
                                },
                            )?
                        } else {
                            self.serialize_children(node, serializer)?
                        }
                    } else {
                        serializer.serialize_inlines(|serializer| {
                            serializer
                                .serialize_element()?
                                .serialize_span(attrs, |serializer| {
                                    serializer.serialize_nested(|serializer| {
                                        self.serialize_children(node, serializer)
                                    })
                                })
                        })?
                    }
                }
                serializer
                    .serialize_raw_html(|serializer| serializer.end_elem(element.name.clone()))
            }
        }
    }

    pub fn emit(
        self,
        serializer: &mut pandoc::native::SerializeBlocks<'_, 'book, '_, impl io::Write>,
    ) -> anyhow::Result<()> {
        let preprocessor = &mut serializer.serializer.preprocessor;
        let chapter = preprocessor.chapter();
        if chapter.number.is_none() && preprocessor.part_num() > 0 {
            if let pandoc::OutputFormat::Latex { .. } = preprocessor.preprocessor.ctx.output {
                serializer
                    .serialize_element()?
                    .serialize_raw_block("latex", |raw| {
                        raw.serialize_code(r"\bookmarksetup{{startatroot}}")
                    })?;
            }
        }

        let root = self.tree.tree.root().first_child().unwrap();
        serializer.serialize_nested(|serializer| self.serialize_children(root, serializer))
    }
}

struct DebugChildren<'event> {
    tree: &'event Emitter<'event>,
    parent: NodeRef<'event, Node<'event>>,
}

struct DebugNodeAndDescendants<'event> {
    tree: &'event Emitter<'event>,
    node: NodeRef<'event, Node<'event>>,
}

impl fmt::Debug for DebugChildren<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_list();
        for child in self.parent.children() {
            f.entry(&DebugNodeAndDescendants {
                tree: self.tree,
                node: child,
            });
        }
        f.finish()
    }
}

impl fmt::Debug for DebugNodeAndDescendants<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.node.value().fmt(f)?;
        if self.node.has_children() {
            write!(f, " => ")?;
            DebugChildren {
                tree: self.tree,
                parent: self.node,
            }
            .fmt(f)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Emitter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        DebugChildren {
            tree: self,
            parent: self.tree.tree.root(),
        }
        .fmt(f)
    }
}

struct FootnoteCycle<'a>(&'a IndexSet<String>, &'a str);

impl fmt::Debug for FootnoteCycle<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (mut footnotes, last) = (self.0.iter(), self.1);
        if let Some(first) = footnotes.next() {
            write!(f, "{first}")?;
        }
        for footnote in footnotes {
            write!(f, " => {footnote}")?;
        }
        write!(f, " => {last}")?;
        Ok(())
    }
}
