use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    io::{self, Write},
    iter, slice,
};

use aho_corasick::AhoCorasick;
use ego_tree::NodeRef;
use html5ever::{
    local_name, namespace_url,
    serialize::Serializer,
    tendril::{fmt::UTF8, Tendril, TendrilSink},
    LocalName, QualName,
};
use pulldown_cmark::{CodeBlockKind, CowStr, Event as MdEvent, LinkType};
use scraper::{node::Element, Node};

use crate::{html, latex, pandoc, preprocess::UnresolvableRemoteImage};

pub struct TreeBuilder<'book> {
    html: html::Parser,
    md: BTreeMap<html::NodeId, Vec<MdEvent<'book>>>,
    parent: html::NodeId,
    child: Option<html::NodeId>,
    event_node_name: QualName,
    footnotes: HashMap<CowStr<'book>, Bookmark>,
}

pub struct Emitter<'book> {
    html: scraper::Html,
    md: BTreeMap<html::NodeId, Vec<MdEvent<'book>>>,
    event_node_name: QualName,
    footnotes: HashMap<CowStr<'book>, Bookmark>,
}

pub enum Event<'a> {
    Markdown(&'a MdEvent<'a>),
    Html(NodeRef<'a, Node>),
}

pub struct Bookmark {
    node: html::NodeId,
    offset: usize,
}

impl<'book> Emitter<'book> {
    fn events<'a>(&'a self, node: NodeRef<'a, Node>) -> impl Iterator<Item = Event<'a>> + 'a
    where
        'book: 'a,
    {
        enum Iter<'a> {
            Md(slice::Iter<'a, MdEvent<'a>>),
            Html(iter::Once<NodeRef<'a, Node>>),
        }
        impl<'a> Iterator for Iter<'a> {
            type Item = Event<'a>;

            fn next(&mut self) -> Option<Self::Item> {
                match self {
                    Self::Md(events) => events.next().map(Event::Markdown),
                    Self::Html(events) => events.next().map(Event::Html),
                }
            }
        }
        match node.value() {
            Node::Element(element) if element.name == self.event_node_name => {
                debug_assert!(!node.has_children());
                let events = self.md[&node.id()].iter();
                Iter::Md(events)
            }
            _ => Iter::Html(iter::once(node)),
        }
    }

    fn children<'a>(&'a self, node: NodeRef<'a, Node>) -> impl Iterator<Item = Event<'a>> + 'a
    where
        'book: 'a,
    {
        node.children().flat_map(move |child| self.events(child))
    }

    fn load_bookmark(&self, bookmark: &Bookmark) -> impl Iterator<Item = Event<'_>> + '_ {
        let Bookmark { node, offset } = bookmark;
        let node = (self.html.tree.get(*node)).expect("bookmark should point to a valid node");
        self.events(node).skip(*offset).chain(
            node.next_siblings()
                .flat_map(|sibling| self.events(sibling)),
        )
    }
}

impl<'book> TreeBuilder<'book> {
    pub fn new() -> Self {
        let html_parser = html5ever::driver::parse_fragment(
            scraper::HtmlTreeSink::new(scraper::Html::new_fragment()),
            html5ever::ParseOpts::default(),
            html5ever::QualName::new(None, html5ever::ns!(html), html5ever::local_name!("body")),
            Vec::new(),
        );
        let parent = html::most_recently_created_open_element(&html_parser);
        Self {
            parent,
            child: None,
            md: Default::default(),
            html: html_parser,
            event_node_name: html::name(LocalName::from("mdbook-pandoc")),
            footnotes: Default::default(),
        }
    }

    pub fn process_html(&mut self, html: Tendril<UTF8>) {
        self.html.process(html);
        self.parent = html::most_recently_created_open_element(&self.html);
        self.child = None;
    }

    fn events(&mut self) -> (html::NodeId, &mut Vec<MdEvent<'book>>) {
        let child = *self.child.get_or_insert_with(|| {
            let mut html = self.html.tokenizer.sink.sink.0.borrow_mut();
            let mut parent = html.tree.get_mut(self.parent).unwrap();
            let child = parent.append(Node::Element(Element::new(
                self.event_node_name.clone(),
                Vec::new(),
            )));
            child.id()
        });
        (child, self.md.entry(child).or_default())
    }

    pub fn bookmark(&mut self) -> Bookmark {
        let (node, events) = self.events();
        Bookmark {
            node,
            offset: events.len(),
        }
    }

    pub fn generate_event(&mut self, event: MdEvent<'book>) {
        let (_, events) = self.events();
        events.push(event);
    }

    pub fn footnote(&mut self, label: CowStr<'book>, bookmark: Bookmark) {
        self.footnotes.insert(label, bookmark);
    }

    pub fn finish(self) -> Emitter<'book> {
        Emitter {
            html: self.html.finish(),
            md: self.md,
            event_node_name: self.event_node_name,
            footnotes: self.footnotes,
        }
    }
}

impl<'book> Emitter<'book> {
    pub fn serialize_events<'event>(
        &self,
        mut events: impl Iterator<Item = Event<'event>>,
        serializer: &mut pandoc::native::SerializeNested<'_, '_, 'book, '_, impl io::Write>,
    ) -> anyhow::Result<()>
    where
        'book: 'event,
    {
        while let Some(event) = events.next() {
            self.serialize_event(event, &mut events, serializer)?;
        }
        Ok(())
    }

    pub fn serialize_event<'event>(
        &self,
        event: Event<'event>,
        siblings: &mut impl Iterator<Item = Event<'event>>,
        serializer: &mut pandoc::native::SerializeNested<'_, '_, 'book, '_, impl io::Write>,
    ) -> anyhow::Result<()>
    where
        'book: 'event,
    {
        match event {
            Event::Html(node) => self.serialize_node(node, serializer),
            Event::Markdown(event) => self.serialize_md_event(event, siblings, serializer),
        }
    }

    pub fn serialize_children<'event>(
        &self,
        tag: &pulldown_cmark::Tag<'event>,
        siblings: &mut impl Iterator<Item = Event<'event>>,
        serializer: &mut pandoc::native::SerializeNested<'_, '_, 'book, '_, impl io::Write>,
    ) -> anyhow::Result<()>
    where
        'book: 'event,
    {
        let end = tag.to_end();
        while let Some(event) = siblings.next() {
            match event {
                Event::Markdown(MdEvent::End(tag)) if *tag == end => break,
                _ => self.serialize_event(event, siblings, serializer)?,
            }
        }
        Ok(())
    }

    pub fn skip_children<'event>(
        tag: &pulldown_cmark::Tag<'event>,
        siblings: &mut impl Iterator<Item = Event<'event>>,
    ) -> anyhow::Result<()> {
        let end = tag.to_end();
        while let Some(event) = siblings.next() {
            match event {
                Event::Markdown(MdEvent::End(tag)) if *tag == end => break,
                Event::Markdown(MdEvent::Start(tag)) => Self::skip_children(tag, siblings)?,
                _ => {}
            }
        }
        Ok(())
    }

    pub fn serialize_nested_children<'event>(
        &self,
        tag: &pulldown_cmark::Tag<'event>,
        mut child: impl FnMut(&pulldown_cmark::Tag<'event>) -> bool,
        siblings: &mut impl Iterator<Item = Event<'event>>,
        serializer: &mut pandoc::native::SerializeList<
            '_,
            'book,
            '_,
            impl io::Write,
            pandoc::native::List<pandoc::native::Block>,
        >,
    ) -> anyhow::Result<()>
    where
        'book: 'event,
    {
        let end = tag.to_end();
        while let Some(event) = siblings.next() {
            match event {
                Event::Markdown(MdEvent::End(tag)) if *tag == end => break,
                Event::Markdown(MdEvent::Start(tag)) if child(tag) => {
                    let mut blocks = serializer.serialize_element()??;
                    blocks.serialize_nested(|serializer| {
                        self.serialize_children(tag, siblings, serializer)
                    })?;
                    blocks.finish()?;
                }
                _ => anyhow::bail!("expected start of {tag:?} child, got {event:?}"),
            }
        }
        Ok(())
    }

    pub fn serialize_md_event<'event>(
        &self,
        event: &MdEvent<'event>,
        siblings: &mut impl Iterator<Item = Event<'event>>,
        serializer: &mut pandoc::native::SerializeNested<'_, '_, 'book, '_, impl io::Write>,
    ) -> anyhow::Result<()>
    where
        'book: 'event,
    {
        use pulldown_cmark::{Tag, TagEnd};
        match event {
            // HTML has already been parsed and stripped from the markdown events
            html @ (MdEvent::Html(_) | MdEvent::InlineHtml(_)) => {
                log::error!("HTML should have been filtered out of markdown events: {html:?}");
                Ok(())
            }
            MdEvent::Text(s) => serializer
                .serialize_inlines(|inlines| inlines.serialize_element()?.serialize_str(s)),
            MdEvent::Code(s) => serializer
                .serialize_inlines(|inlines| inlines.serialize_element()?.serialize_code((), s)),
            MdEvent::SoftBreak => serializer
                .serialize_inlines(|inlines| inlines.serialize_element()?.serialize_soft_break()),
            MdEvent::HardBreak => serializer
                .serialize_inlines(|inlines| inlines.serialize_element()?.serialize_line_break()),
            MdEvent::Rule => serializer
                .blocks()?
                .serialize_element()?
                .serialize_horizontal_rule(),
            MdEvent::TaskListMarker(checked) => serializer.serialize_inlines(|inlines| {
                inlines
                    .serialize_element()?
                    .serialize_str_unescaped(if *checked { "\\9746" } else { "\\9744" })?;
                inlines.serialize_element()?.serialize_space()
            }),
            MdEvent::End(TagEnd::HtmlBlock) => Ok(()),
            MdEvent::End(end) => {
                anyhow::bail!("end tag should have been handled by a recursive call: {end:?}")
            }
            MdEvent::Start(tag) => match tag {
                Tag::HtmlBlock => Ok(()),
                Tag::Paragraph => {
                    serializer
                        .blocks()?
                        .serialize_element()?
                        .serialize_para(|inlines| {
                            inlines.serialize_nested(|serializer| {
                                self.serialize_children(tag, siblings, serializer)
                            })
                        })
                }
                Tag::Heading {
                    level,
                    id,
                    classes,
                    attrs,
                } => serializer.blocks()?.serialize_element()?.serialize_header(
                    *level as usize,
                    (id.as_deref(), classes, attrs),
                    |inlines| {
                        inlines.serialize_nested(|serializer| {
                            self.serialize_children(tag, siblings, serializer)
                        })
                    },
                ),
                Tag::BlockQuote => serializer
                    .blocks()?
                    .serialize_element()?
                    .serialize_block_quote(|blocks| {
                        blocks.serialize_nested(|serializer| {
                            self.serialize_children(tag, siblings, serializer)
                        })
                    }),
                Tag::CodeBlock(kind) => {
                    // MdBook supports custom attributes in code block info strings.
                    // Attributes are separated by a comma, space, or tab from the language name.
                    // See https://rust-lang.github.io/mdBook/format/mdbook.html#rust-code-block-attributes
                    // This processes and strips out the attributes.
                    let (language, mut attributes) = {
                        let info_string = match kind {
                            CodeBlockKind::Indented => "",
                            CodeBlockKind::Fenced(info_string) => info_string,
                        };
                        let mut parts =
                            info_string.split([',', ' ', '\t']).map(|part| part.trim());
                        (parts.next(), parts)
                    };

                    // https://rust-lang.github.io/mdBook/format/mdbook.html?highlight=hide#hiding-code-lines
                    let hide_lines = !serializer.preprocessor().preprocessor.ctx.code.show_hidden_lines;
                    let hidden_line_prefix = hide_lines.then(|| {
                        let hidelines_override =
                            attributes.find_map(|attr| attr.strip_prefix("hidelines="));
                        hidelines_override.or_else(|| {
                            let lang = language?;
                            // Respect [output.html.code.hidelines]
                            let html = serializer.preprocessor().preprocessor.ctx.html;
                            html.and_then(|html| Some(html.code.hidelines.get(lang)?.as_str()))
                                .or((lang == "rust").then_some("#"))
                        })
                    }).flatten();

                    let texts = iter::from_fn(|| match siblings.next() {
                        Some(Event::Markdown(MdEvent::Text(text))) => Some(text),
                        Some(Event::Markdown(MdEvent::End(TagEnd::CodeBlock))) => None,
                        event => panic!("Code blocks should contain only literal text, but encountered {event:?}"),
                    });
                    let lines = texts.flat_map(|text| text.lines()).filter(|line| {
                        hidden_line_prefix.map_or(true, |prefix| !line.trim_start().starts_with(prefix))
                    }).collect::<Vec<_>>();

                    // Pandoc+fvextra only wraps long lines in code blocks with info strings
                    // so fall back to "text"
                    let language = language.unwrap_or("text");

                    if let pandoc::OutputFormat::Latex { .. } = serializer.preprocessor().preprocessor.ctx.output {
                        const CODE_BLOCK_LINE_LENGTH_LIMIT: usize = 1000;

                        let overly_long_line = lines.iter().any(|line| {
                            line.len() > CODE_BLOCK_LINE_LENGTH_LIMIT
                        });
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
                                lines.into_iter().map(move |line| {
                                    ac.replace_all(line, replace_with)
                                })
                            };
                            return serializer.blocks()?.serialize_element()?.serialize_raw_block("latex", |raw| {
                                for line in lines {
                                    raw.serialize_code(r"\texttt{{")?;
                                    raw.serialize_code(&line)?;
                                    raw.serialize_code(r"}}\\")?;
                                }
                                Ok(())
                            })
                        }
                    }

                    let classes = [CowStr::Borrowed(language)];
                    serializer
                        .blocks()?
                        .serialize_element()?
                        .serialize_code_block((None, &classes, &[]), |code| {
                            for line in lines {
                                code.serialize_code(line)?;
                                code.serialize_code("\n")?;
                            }
                            Ok(())
                        })
                }
                Tag::List(None) => serializer
                    .blocks()?
                    .serialize_element()?
                    .serialize_bullet_list(|items| {
                        self.serialize_nested_children(
                            tag,
                            |tag| matches!(tag, Tag::Item),
                            siblings,
                            items,
                        )
                    }),
                Tag::List(Some(first)) => serializer
                    .blocks()?
                    .serialize_element()?
                    .serialize_ordered_list(*first, |items| {
                        self.serialize_nested_children(
                            tag,
                            |tag| matches!(tag, Tag::Item),
                            siblings,
                            items,
                        )
                    }),
                Tag::Item => anyhow::bail!("list items should have been processed already"),
                Tag::FootnoteDefinition(_) => Self::skip_children(tag, siblings),
                Tag::Table(alignment) => {
                    let preprocessor = serializer.preprocessor();
                    let table = preprocessor.pop_table().unwrap();
                    let column_widths = preprocessor.column_widths(table);
                    serializer.blocks()?.serialize_element()?.serialize_table(
                        siblings,
                        (),
                        (alignment.iter().copied().map(Into::into)).zip(column_widths),
                        ((), |siblings, header| match siblings.next() {
                            Some(Event::Markdown(MdEvent::Start(Tag::TableHead))) => {
                                header.serialize_element()?.serialize_row((), |cells| loop {
                                    match siblings.next() {
                                        Some(Event::Markdown(MdEvent::End(TagEnd::TableHead))) => {
                                            break Ok(())
                                        }
                                        Some(Event::Markdown(MdEvent::Start(
                                            cell @ Tag::TableCell,
                                        ))) => cells.serialize_element()?.serialize_cell(
                                            (),
                                            |blocks| {
                                                blocks.serialize_nested(|serializer| {
                                                    self.serialize_children(
                                                        cell, siblings, serializer,
                                                    )
                                                })
                                            },
                                        )?,
                                        event => anyhow::bail!("expected table cell, got {event:?}"),
                                    }
                                })
                            }
                            event => anyhow::bail!("expected table head, got {event:?}"),
                        }),
                        ((), |siblings, body| loop {
                            match siblings.next() {
                                Some(Event::Markdown(MdEvent::End(TagEnd::Table))) => break Ok(()),
                                Some(Event::Markdown(MdEvent::Start(Tag::TableRow))) => {
                                    body.serialize_element()?.serialize_row((), |cells| loop {
                                        match siblings.next() {
                                            Some(Event::Markdown(MdEvent::End(
                                                TagEnd::TableRow,
                                            ))) => break Ok(()),
                                            Some(Event::Markdown(MdEvent::Start(
                                                cell @ Tag::TableCell,
                                            ))) => cells.serialize_element()?.serialize_cell(
                                                (),
                                                |blocks| {
                                                    blocks.serialize_nested(|serializer| {
                                                        self.serialize_children(
                                                            cell, siblings, serializer,
                                                        )
                                                    })
                                                },
                                            )?,
                                            event => anyhow::bail!("expected table cell, got {event:?}"),
                                        }
                                    })?
                                }
                                event => anyhow::bail!("expected table row, got {event:?}"),
                            }
                        }),
                    )
                }
                Tag::TableHead | Tag::TableRow | Tag::TableCell => anyhow::bail!("table contents should have been processed already"),
                Tag::Emphasis => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_emph(|inlines| {
                        inlines.serialize_nested(|serializer| {
                            self.serialize_children(tag, siblings, serializer)
                        })
                    })
                }),
                Tag::Strong => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_strong(|inlines| {
                        inlines.serialize_nested(|serializer| {
                            self.serialize_children(tag, siblings, serializer)
                        })
                    })
                }),
                Tag::Strikethrough => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_strikeout(|inlines| {
                        inlines.serialize_nested(|serializer| {
                            self.serialize_children(tag, siblings, serializer)
                        })
                    })
                }),
                Tag::Link {
                    link_type: _,
                    dest_url,
                    title,
                    id: _,
                } => serializer.serialize_inlines(|inlines| {
                    inlines.serialize_element()?.serialize_link(
                        (None, &[], &[]),
                        |alt| {
                            alt.serialize_nested(|alt| self.serialize_children(tag, siblings, alt))
                        },
                        dest_url,
                        title,
                    )
                }),
                Tag::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                } => {
                    serializer.serialize_inlines(|inlines| {
                        match inlines.serializer.preprocessor.resolve_image_url(dest_url.as_ref().into(), *link_type) {
                            Err(UnresolvableRemoteImage) => {
                                inlines.serialize_nested(|inlines| self.serialize_children(tag, siblings, inlines))
                            },
                            Ok(dest_url) => {
                                inlines.serialize_element()?.serialize_image(
                                    (Some(id.as_ref()), &[], &[]),
                                    |alt| alt.serialize_nested(|alt| self.serialize_children(tag, siblings, alt)),
                                    &dest_url,
                                    title,
                                )
                            }
                        }
                    })
                },
                Tag::MetadataBlock(_kind) => {
                    log::warn!("Ignoring metadata block");
                    Ok(())
                }
            },
            MdEvent::FootnoteReference(label) => match self.footnotes.get(label) {
                None => {
                    log::warn!("Undefined footnote reference: {label}");
                    Ok(())
                }
                Some(bookmark) => serializer.serialize_inlines(|serializer| {
                    serializer
                        .serialize_element()?
                        .serialize_note(|serializer| {
                            serializer.serialize_nested(|serializer| {
                                let mut events = self.load_bookmark(bookmark);
                                match events.next() {
                                    Some(Event::Markdown(MdEvent::Start(tag @ Tag::FootnoteDefinition(l)))) => {
                                        debug_assert_eq!(l, label);
                                        self.serialize_children(tag, &mut events, serializer)
                                    }
                                    event => {
                                        log::warn!("Failed to look up footnote definition: found {event:?} instead");
                                        Ok(())
                                    }
                                }
                            })
                        })
                }),
            },
        }
    }

    pub fn serialize_node(
        &self,
        node: NodeRef<'_, Node>,
        serializer: &mut pandoc::native::SerializeNested<'_, '_, 'book, '_, impl io::Write>,
    ) -> anyhow::Result<()> {
        match node.value() {
            Node::Document | Node::Fragment | Node::Doctype(_) | Node::ProcessingInstruction(_) => {
                Ok(())
            }
            Node::Comment(comment) => {
                serializer.serialize_raw_html(|serializer| serializer.write_comment(comment))
            }
            Node::Text(text) => {
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
            Node::Element(element) => {
                debug_assert_ne!(element.name, self.event_node_name);
                match element.name.local {
                    local_name!("a") => {
                        let [href, title] = [local_name!("href"), local_name!("title")]
                            .map(|attr| element.attrs.get(&html::name(attr)));
                        return serializer.serialize_inlines(|inlines| {
                            if let Some(href) = href {
                                inlines.serialize_element()?.serialize_link(
                                    &element.attrs,
                                    |alt| {
                                        alt.serialize_nested(|alt| {
                                            self.serialize_events(self.children(node), alt)
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
                                            self.serialize_events(self.children(node), serializer)
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
                                        self.serialize_events(self.children(node), serializer)
                                    })
                                })
                        })
                    }
                    local_name!("div") => {
                        return serializer.blocks()?.serialize_element()?.serialize_div(
                            &element.attrs,
                            |blocks| {
                                blocks.serialize_nested(|serializer| {
                                    self.serialize_events(self.children(node), serializer)
                                })
                            },
                        );
                    }
                    local_name!("img") => {
                        let mut attrs = element.attrs.clone();
                        let [src, alt, title] =
                            [local_name!("src"), local_name!("alt"), local_name!("title")]
                                .map(|attr| attrs.swap_remove(&html::name(attr)));
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
                        let mut attrs = element.attrs.iter();
                        match attrs.next() {
                            Some((attr, val))
                                if matches!(attr.local, local_name!("class"))
                                    && attrs.next().is_none() =>
                            {
                                if let Some(icon) = val.strip_prefix("fa fa-") {
                                    let ctx = &mut serializer.preprocessor().preprocessor.ctx;
                                    if let pandoc::OutputFormat::Latex { packages } =
                                        &mut ctx.output
                                    {
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
                            _ => {}
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
                .then(|| element.attrs.get(&html::name(local_name!("id"))))
                .flatten()
                .map(|s| s.as_ref());
                let attrs = (id, &[], &[]);
                match serializer.blocks() {
                    Ok(serializer) => {
                        serializer
                            .serialize_element()?
                            .serialize_div(attrs, |serializer| {
                                serializer.serialize_nested(|serializer| {
                                    self.serialize_events(self.children(node), serializer)
                                })
                            })
                    }
                    Err(_) => serializer.serialize_inlines(|serializer| {
                        serializer
                            .serialize_element()?
                            .serialize_span(attrs, |serializer| {
                                serializer.serialize_nested(|serializer| {
                                    self.serialize_events(self.children(node), serializer)
                                })
                            })
                    }),
                }?;
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

        serializer.serialize_nested(|serializer| {
            self.serialize_events(self.children(*self.html.root_element()), serializer)
        })
    }
}

impl fmt::Debug for Event<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Markdown(event) => write!(f, "{event:?}"),
            Self::Html(event) => event.value().fmt(f),
        }
    }
}

struct DebugChildren<'event> {
    tree: &'event Emitter<'event>,
    parent: NodeRef<'event, Node>,
}

struct DebugEventAndDescendants<'event> {
    tree: &'event Emitter<'event>,
    event: Event<'event>,
}

impl fmt::Debug for DebugChildren<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_list();
        for event in self.tree.children(self.parent) {
            f.entry(&DebugEventAndDescendants {
                tree: self.tree,
                event,
            });
        }
        f.finish()
    }
}

impl fmt::Debug for DebugEventAndDescendants<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.event.fmt(f)?;
        match self.event {
            Event::Markdown(_) => Ok(()),
            Event::Html(node) => {
                if node.has_children() {
                    write!(f, " => ")?;
                    DebugChildren {
                        tree: self.tree,
                        parent: node,
                    }
                    .fmt(f)?;
                }
                Ok(())
            }
        }
    }
}

impl fmt::Debug for Emitter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        DebugChildren {
            tree: self,
            parent: *self.html.root_element(),
        }
        .fmt(f)
    }
}
