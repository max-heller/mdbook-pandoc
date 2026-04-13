use std::{
    borrow::Cow,
    collections::HashMap,
    fmt,
    io::{self, Write},
};

use aho_corasick::AhoCorasick;
use anyhow::Context;
use base64::Engine as _;
use ego_tree::{NodeId, NodeRef};
use font_awesome_as_a_crate as fa;
use html5ever::{
    expanded_name,
    interface::ElemName,
    local_name, ns,
    serialize::Serializer,
    tendril::{fmt::UTF8, format_tendril, StrTendril, Tendril, TendrilSink},
    LocalName,
};
use indexmap::{IndexMap, IndexSet};
use pulldown_cmark::{BlockQuoteKind, CowStr, LinkType};
use tracing::trace;

use crate::{
    html,
    pandoc::{self, native::Attributes as _},
    url,
};

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
            let scripting_allowed = false;
            opts.tree_builder.scripting_enabled = scripting_allowed;
            html5ever::driver::parse_fragment(
                HtmlTreeSink::new(),
                opts,
                html::name!(html "body"),
                Vec::new(),
                scripting_allowed,
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
        tracing::trace!("Writing Pandoc AST for {:?}", node.value());
        match node.value() {
            Node::Document => unreachable!(),
            Node::HtmlComment(comment) => {
                serializer.serialize_raw_html(|serializer| serializer.write_comment(comment))
            }
            Node::HtmlText(text) => {
                let none_or_element = |node: Option<NodeRef<_>>| {
                    node.is_none_or(|node| matches!(node.value(), Node::Element(..)))
                };
                // Drop newlines between elements
                if text.as_ref() == "\n"
                    && (none_or_element(node.prev_sibling())
                        || none_or_element(node.next_sibling()))
                {
                    return Ok(());
                }
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
                    *level,
                    (id.as_deref(), classes, attrs),
                    |inlines| {
                        inlines.serialize_nested(|serializer| {
                            self.serialize_children(node, serializer)
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
                    let head = children.next().unwrap();
                    let body = children.next();
                    debug_assert!(children.next().is_none());

                    let thead = match head.value() {
                        Node::Element(Element::Html(element))
                            if element.name.expanded() == expanded_name!(html "thead") =>
                        {
                            element
                        }
                        event => anyhow::bail!("expected table head, got {event:?}"),
                    };
                    let body = body
                        .map(|node| match node.value() {
                            Node::Element(Element::Html(element))
                                if element.name.expanded() == expanded_name!(html "tbody") =>
                            {
                                Ok((node, element))
                            }
                            event => anyhow::bail!("expected table body, got {event:?}"),
                        })
                        .transpose()?;

                    serializer.blocks()?.serialize_element()?.serialize_table(
                        (),
                        (alignment.iter().copied().map(Into::into)).zip(column_widths),
                        (&thead.attrs, |serializer| {
                            for row in head.children() {
                                let tr = match row.value() {
                                    Node::Element(Element::Html(e)) if e.name.expanded() == expanded_name!(html "tr") => e,
                                    event => anyhow::bail!("expected table row, got {event:?}"),
                                };
                                serializer.serialize_element()?.serialize_row(&tr.attrs, |cells| {
                                    for cell in row.children() {
                                        let th = match cell.value() {
                                            Node::Element(Element::Html(e)) if e.name.expanded() == expanded_name!(html "th") => e,
                                            event => anyhow::bail!("expected table cell, got {event:?}"),
                                        };
                                        for node in cell.children() {
                                            cells.serialize_element()?.serialize_cell(
                                                &th.attrs,
                                                |blocks| {
                                                    blocks.serialize_nested(|serializer| {
                                                        self.serialize_node(node, serializer)
                                                    })
                                                },
                                            )?;
                                        }
                                    }
                                    Ok(())
                                })?
                            }
                            Ok(())
                        }),
                        |serializer| {
                            let Some((body, tbody)) = body else {
                                return Ok(())
                            };
                            serializer.serialize_element()?.serialize_body(&tbody.attrs, |serializer| {
                                for row in body.children() {
                                    let tr = match row.value() {
                                        Node::Element(Element::Html(e))
                                            if e.name.expanded() == expanded_name!(html "tr") => e,
                                        event => anyhow::bail!("expected table row, got {event:?}"),
                                    };
                                    serializer.serialize_element()?.serialize_row(
                                        &tr.attrs,
                                        |cells| {
                                            for cell in row.children() {
                                                let td = match cell.value() {
                                                    Node::Element(Element::Html(e))
                                                        if e.name.expanded() == expanded_name!(html "td") => e,
                                                    event => anyhow::bail!("expected table data (<td>), got {event:?}"),
                                                };
                                                cells
                                                    .serialize_element()?
                                                    .serialize_cell(&td.attrs, |blocks| {
                                                        blocks.serialize_nested(
                                                            |serializer| {
                                                                for node in cell.children() {
                                                                    self.serialize_node(node, serializer)?;
                                                                }
                                                                Ok(())
                                                            },
                                                        )
                                                    })?
                                            }
                                            Ok(())
                                        },
                                    )?
                                }
                                Ok(())
                            })
                        }
                    )
                }
                MdElement::FootnoteDefinition => Ok(()),
                MdElement::FootnoteReference(label) => match self.footnotes.get(label) {
                    None => {
                        tracing::warn!("Undefined footnote: {label}");
                        Ok(())
                    }
                    Some(definition) => {
                        let open_footnotes = &mut serializer.serializer().footnotes;
                        if open_footnotes.contains(label.as_ref()) {
                            tracing::warn!(
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
                MdElement::BlockQuote(Some(kind)) => {
                    let (class, title) = match kind {
                        BlockQuoteKind::Note => ("note", "Note"),
                        BlockQuoteKind::Tip => ("tip", "Tip"),
                        BlockQuoteKind::Important => ("important", "Important"),
                        BlockQuoteKind::Warning => ("warning", "Warning"),
                        BlockQuoteKind::Caution => ("caution", "Caution"),
                    };
                    serializer.blocks()?.serialize_element()?.serialize_div(
                        (None, &[class.into()], &[]),
                        |blocks| {
                            blocks.serialize_element()?.serialize_div(
                                (None, &["title".into()], &[]),
                                |header| {
                                    header.serialize_element()?.serialize_para(|header| {
                                        header.serialize_element()?.serialize_str(title)
                                    })
                                },
                            )?;
                            blocks.serialize_nested(|serializer| {
                                self.serialize_children(node, serializer)
                            })
                        },
                    )
                }
                MdElement::BlockQuote(None) => serializer
                    .blocks()?
                    .serialize_element()?
                    .serialize_block_quote(|blocks| {
                        blocks.serialize_nested(|serializer| {
                            self.serialize_children(node, serializer)
                        })
                    }),
                MdElement::InlineCode(s) => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_code((), s)
                }),
                MdElement::CodeBlock(kind) => {
                    let ctx = &serializer.preprocessor().preprocessor.ctx;

                    let mut code_block = code::CodeBlock::new(kind, &ctx.html.code);

                    let lines = node.children().map(|node| {
                        match node.value() {
                            Node::Element(Element::Markdown(MdElement::Text(text))) => text,
                            event => panic!("Code blocks should contain only literal text, but encountered {event:?}"),
                        }
                    }).flat_map(|text| text.lines());
                    let lines = code_block.lines(lines, ctx.code);

                    if let pandoc::OutputFormat::Latex { .. } =
                        serializer.preprocessor().preprocessor.ctx.output
                    {
                        const CODE_BLOCK_LINE_LENGTH_LIMIT: usize = 1000;

                        // Pandoc+fvextra only wraps long lines in code blocks with info strings
                        // so fall back to "text"
                        match &mut code_block.language {
                            code::Language::Other { language, .. } => {
                                *language = language.or(Some("text"))
                            }
                            code::Language::Rust => {}
                        }

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

                    serializer
                        .blocks()?
                        .serialize_element()?
                        .serialize_code_block(code_block, |code| {
                            for line in lines {
                                code.serialize_code(&line)?;
                                code.serialize_code("\n")?;
                            }
                            Ok(())
                        })
                }
                MdElement::RawInline { format, raw } => serializer.serialize_inlines(|inlines| {
                    inlines
                        .serialize_element()?
                        .serialize_raw_inline(format, |serializer| {
                            serializer.write_all(raw.as_bytes())
                        })
                }),
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
                MdElement::Math(kind, math) => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_math(*kind, math)
                }),
                MdElement::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                } => serializer.serialize_inlines(|inlines| {
                    let dest_url = inlines.serializer.preprocessor.resolve_image_url(
                        url::best_effort_decode(dest_url.as_ref().into()),
                        *link_type,
                    );
                    inlines.serialize_element()?.serialize_image(
                        (Some(id.as_ref()), &[], &[]),
                        |alt| alt.serialize_nested(|alt| self.serialize_children(node, alt)),
                        &url::encode(dest_url),
                        title,
                    )
                }),
            },
            Node::Element(Element::Html(element)) => {
                let ctx = &mut serializer.preprocessor().preprocessor.ctx;
                if !matches!(ctx.output, pandoc::OutputFormat::HtmlLike) {
                    for (prop, val) in element.attrs.css_properties(&ctx.css.styles) {
                        // Don't render elements with display: none
                        if prop == "display" && val == "none" {
                            return Ok(());
                        }
                    }
                }
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
                    local_name!("s") => {
                        return serializer.serialize_inlines(|inlines| {
                            inlines.serialize_element()?.serialize_strikeout(|inlines| {
                                inlines.serialize_nested(|serializer| {
                                    self.serialize_children(node, serializer)
                                })
                            })
                        })
                    }
                    local_name!("sup") => {
                        return serializer.serialize_inlines(|inlines| {
                            inlines
                                .serialize_element()?
                                .serialize_superscript(|inlines| {
                                    inlines.serialize_nested(|serializer| {
                                        self.serialize_children(node, serializer)
                                    })
                                })
                        })
                    }
                    local_name!("sub") => {
                        return serializer.serialize_inlines(|inlines| {
                            inlines.serialize_element()?.serialize_subscript(|inlines| {
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
                    local_name!("figure") => {
                        let is_figcaption = |node: &NodeRef<_>| {
                            matches!(
                                node.value(),
                                Node::Element(Element::Html(element))
                                    if matches!(element.name.expanded(), expanded_name!(html "figcaption"))
                            )
                        };
                        let caption = node.children().find(is_figcaption);
                        return serializer.blocks()?.serialize_element()?.serialize_figure(
                            &element.attrs,
                            |caption_blocks| {
                                if let Some(caption) = caption {
                                    caption_blocks.serialize_nested(|serializer| {
                                        self.serialize_children(caption, serializer)
                                    })
                                } else {
                                    Ok(())
                                }
                            },
                            |content_blocks| {
                                content_blocks.serialize_nested(|serializer| {
                                    for node in node.children().filter(|node| !is_figcaption(node))
                                    {
                                        self.serialize_node(node, serializer)?;
                                    }
                                    Ok(())
                                })
                            },
                        );
                    }
                    local_name!("svg") => {
                        let mut svg_attrs = element.attrs.clone();
                        let [_xmlns, _alt, title] = [
                            html::name!("xmlns"),
                            html::name!("alt"),
                            html::name!("title"),
                        ]
                        .map(|attr| svg_attrs.rest.swap_remove(&attr));

                        trace!("element: {:?}", element);
                        let svg_tag = format!("{:?}", element); // TODO: risky business

                        fn extract_content(n: NodeRef<'_, Node<'_>>) -> String {
                            n.children()
                                .map(|child| match child.value() {
                                    Node::Document => todo!(),
                                    Node::HtmlComment(_) => "".to_string(),
                                    Node::HtmlText(tendril) => tendril.to_string(),
                                    Node::Element(element) => format!(
                                        "{:?}{}</{}>",
                                        element,
                                        extract_content(child),
                                        element
                                            .name()
                                            .local_name()
                                            .to_lowercase()
                                            .chars()
                                            .filter(|c| c != &'"')
                                            .collect::<String>()
                                    ),
                                })
                                .collect()
                        }
                        let svg_subcontent = extract_content(node);
                        // .descendants()
                        // .map(|v| match v.value() {
                        //     Node::Document => todo!(),
                        //     Node::HtmlComment(_) => "".to_string(),
                        //     Node::HtmlText(tendril) => tendril.to_string(),
                        //     Node::Element(element) => format!("{:?}", element),
                        // })
                        // .collect::<Vec<String>>()
                        // .join("");
                        // trace!("svg_subcontent: {}", svg_subcontent);
                        let content = format!("{}{}</svg>", svg_tag, svg_subcontent);
                        trace!("content: {}", content);
                        let content = base64::engine::general_purpose::STANDARD.encode(content);
                        let data_uri = format!("data:image/svg+xml;base64,{}", content);
                        trace!("data_uri: {}", data_uri);
                        return serializer.serialize_inlines(|inlines| {
                            inlines.serialize_element()?.serialize_image(
                                &svg_attrs,
                                |_serializer| Ok(()),
                                &url::encode(data_uri.into()),
                                title.as_ref().map_or("", |s| s.as_ref()),
                            )
                        });
                    }
                    local_name!("img") => {
                        let mut attrs = element.attrs.clone();
                        let [src, alt, title] =
                            [html::name!("src"), html::name!("alt"), html::name!("title")]
                                .map(|attr| attrs.rest.swap_remove(&attr));
                        let Some(src) = src else { return Ok(()) };
                        let src = serializer.preprocessor().resolve_image_url(
                            url::best_effort_decode(src.as_ref().into()),
                            LinkType::Inline,
                        );
                        return serializer.serialize_inlines(|inlines| {
                            inlines.serialize_element()?.serialize_image(
                                &attrs,
                                |serializer| match alt {
                                    Some(alt) => {
                                        serializer.serialize_element()?.serialize_str(&alt)
                                    }
                                    None => Ok(()),
                                },
                                &url::encode(src),
                                title.as_ref().map_or("", |s| s.as_ref()),
                            )
                        });
                    }
                    local_name!("i") if !node.has_children() => {
                        let Attributes { id, classes, rest } = &element.attrs;

                        // Check for Font Awesome icons
                        let font_awesome_icon = {
                            let mut type_ = fa::Type::Regular;
                            let mut icon = None;
                            let classes = classes
                                .split_ascii_whitespace()
                                .filter(|&class| {
                                    if matches!(class, "fa" | "fa-regular") {
                                        type_ = fa::Type::Regular;
                                        false
                                    } else if matches!(class, "fas" | "fa-solid") {
                                        type_ = fa::Type::Solid;
                                        false
                                    } else if matches!(class, "fab" | "fa-brands") {
                                        type_ = fa::Type::Brands;
                                        false
                                    } else if let Some(class) = class.strip_prefix("fa-") {
                                        icon = Some(class);
                                        false
                                    } else {
                                        true
                                    }
                                })
                                .map(CowStr::Borrowed)
                                .collect::<Vec<_>>()
                                .join(" ");
                            icon.map(|icon| (type_, icon, classes))
                        };
                        if let Some((type_, icon, classes)) = font_awesome_icon {
                            if let Ok(svg) = fa::svg(type_, icon) {
                                let data_url = {
                                    let mut data = String::from("data:image/svg+xml;base64,");
                                    base64::engine::general_purpose::STANDARD
                                        .encode_string(svg, &mut data);
                                    data
                                };
                                // If the icon does not already have a width/height specified,
                                // assign it one matching the text height
                                let attrs = {
                                    let mut attrs = Attributes {
                                        id: id.clone(),
                                        classes: classes.into(),
                                        rest: rest.clone(),
                                    };
                                    let css = serializer.preprocessor().preprocessor.ctx.css;
                                    if !attrs
                                        .css_properties(&css.styles)
                                        .any(|(prop, _)| matches!(prop, "width" | "height"))
                                    {
                                        attrs.rest = IndexMap::from_iter([(
                                            html::name!("height"),
                                            "1em".into(),
                                        )]);
                                    }
                                    attrs
                                };
                                return serializer.serialize_inlines(|inlines| {
                                    inlines.serialize_element()?.serialize_image(
                                        attrs,
                                        |_alt| Ok(()),
                                        &data_url,
                                        "",
                                    )
                                });
                            }
                        }
                    }
                    local_name!("dl") => {
                        enum Component {
                            Term,
                            Definition,
                        }
                        let mut components = node
                            .children()
                            .filter_map(|node| match node.value() {
                                Node::Element(Element::Html(element)) => {
                                    match element.name.expanded() {
                                        expanded_name!(html "dt") => {
                                            Some((Component::Term, node, &element.attrs))
                                        }
                                        expanded_name!(html "dd") => {
                                            Some((Component::Definition, node, &element.attrs))
                                        }
                                        _ => None,
                                    }
                                }
                                _ => None,
                            })
                            .peekable();
                        return serializer
                            .blocks()?
                            .serialize_element()?
                            .serialize_definition_list(|items| {
                                while let Some((component, node, attrs)) = components.next() {
                                    match component {
                                        Component::Term => {}
                                        Component::Definition => {
                                            anyhow::bail!("definition list definition with no term")
                                        }
                                    };
                                    items.serialize_element()?.serialize_item(
                                        |term| {
                                            if attrs.is_empty() {
                                                term.serialize_nested(|serializer| {
                                                    self.serialize_children(node, serializer)
                                                })
                                            } else {
                                                // Wrap term in a span with the attributes since Pandoc
                                                // doesn't support attributes on definition list terms
                                                term.serialize_element()?.serialize_span(
                                                    attrs,
                                                    |inlines| {
                                                        inlines.serialize_nested(|serializer| {
                                                            self.serialize_children(
                                                                node, serializer,
                                                            )
                                                        })
                                                    },
                                                )
                                            }
                                        },
                                        |definitions| {
                                            while let Some((_, definition, _)) =
                                                components.next_if(|(component, _, _)| {
                                                    matches!(component, Component::Definition)
                                                })
                                            {
                                                let mut serializer =
                                                    definitions.serialize_element()??;
                                                serializer.serialize_nested(|serializer| {
                                                    self.serialize_children(definition, serializer)
                                                })?;
                                                serializer.finish()?;
                                            }
                                            Ok(())
                                        },
                                    )?
                                }
                                Ok(())
                            });
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
