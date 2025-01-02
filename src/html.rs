use std::{
    cell::{self, Cell},
    collections::HashMap,
    fmt::{self, Display, Write},
    io,
    ops::Range,
};

use html5ever::{
    interface::NodeOrText,
    local_name, namespace_url, ns,
    serialize::Serializer,
    tendril::{fmt::UTF8, Tendril, TendrilSink},
    tree_builder::TreeSink,
    LocalName, QualName,
};
use pulldown_cmark::{Event, LinkType};
use scraper::{
    node::{Attributes, Element},
    Node,
};

use crate::{
    latex,
    pandoc::{self, OutputFormat, RenderContext},
    preprocess::{PreprocessChapter, UnresolvableRemoteImage},
};

pub type NodeId = <scraper::HtmlTreeSink as TreeSink>::Handle;
pub type Parser = html5ever::Parser<scraper::HtmlTreeSink>;

type MarkdownEvent<'book> = (Event<'book>, Option<Range<usize>>);

pub struct TreeBuilder<'book> {
    events: HashMap<NodeId, Vec<MarkdownEvent<'book>>>,
    parent: NodeId,
    child: Option<NodeId>,
    parser: Parser,
    event_node_name: QualName,
}

pub struct Emitter<'book> {
    events: HashMap<NodeId, Vec<MarkdownEvent<'book>>>,
    event_node_name: QualName,
}

pub fn name(local: LocalName) -> QualName {
    QualName::new(None, ns!(), local)
}

impl<'book> TreeBuilder<'book> {
    pub fn new() -> Self {
        let html_parser = html5ever::driver::parse_fragment(
            scraper::HtmlTreeSink::new(scraper::Html::new_fragment()),
            html5ever::ParseOpts::default(),
            html5ever::QualName::new(None, html5ever::ns!(html), html5ever::local_name!("body")),
            Vec::new(),
        );
        let parent = most_recently_created_open_element(&html_parser);
        Self {
            parent,
            child: None,
            events: Default::default(),
            parser: html_parser,
            event_node_name: name(LocalName::from("mdbook-pandoc")),
        }
    }

    pub fn process_html(&mut self, html: Tendril<UTF8>) {
        self.parser.process(html);
        self.parent = crate::html::most_recently_created_open_element(&self.parser);
        self.child = None;
    }

    pub fn generate_event(&mut self, event: (Event<'book>, Option<Range<usize>>)) {
        let child = self.child.get_or_insert_with(|| {
            let sink = &self.parser.tokenizer.sink.sink;
            let child = sink.create_element(
                self.event_node_name.clone(),
                Default::default(),
                Default::default(),
            );
            sink.append(&self.parent, NodeOrText::AppendNode(child));
            child
        });
        self.events.entry(*child).or_default().push(event);
    }

    pub fn finish(self) -> (scraper::Html, Emitter<'book>) {
        let html = self.parser.finish();
        let emitter = Emitter {
            events: self.events,
            event_node_name: self.event_node_name,
        };
        (html, emitter)
    }
}

impl<'book> Emitter<'book> {
    pub async fn emit<'preprocessor>(
        mut self,
        mut html: scraper::Html,
        preprocessor: &mut PreprocessChapter<'book, 'preprocessor>,
        co: &genawaiter::stack::Co<'_, (Event<'book>, Option<Range<usize>>)>,
    ) {
        let mut in_html_block = false;
        let mut skip = false;
        let root = html.tree.root_mut().into_first_child().ok().unwrap();
        let (root, mut cursor) = (root.id(), root.into_first_child().ok());
        while let Some(mut node) = cursor.take() {
            let id = node.id();
            let has_children = node.has_children();
            match node.value() {
                Node::Comment(comment) => {
                    let html =
                        Self::serialize_html(|mut serializer| serializer.write_comment(comment));
                    self.emit_event(Event::Html(html.into()), co).await;
                }
                Node::Text(text) => {
                    let html = Self::serialize_html(|mut serializer| serializer.write_text(text));
                    self.emit_event(Event::Html(html.into()), co).await;
                }
                Node::Element(element) => 'element: {
                    let mut write_html = true;
                    if element.name == self.event_node_name {
                        debug_assert!(!has_children);
                        for (event, range) in self.events.remove(&id).into_iter().flatten() {
                            match &event {
                                Event::Start(pulldown_cmark::Tag::HtmlBlock) => {
                                    in_html_block = true
                                }
                                Event::End(pulldown_cmark::TagEnd::HtmlBlock) => {
                                    in_html_block = false
                                }
                                _ => {}
                            }
                            co.yield_((event, range)).await;
                        }
                        break 'element;
                    }
                    match &element.name.local {
                        &local_name!("a") | &local_name!("span")
                            if (preprocessor.preprocessor.ctx.pandoc)
                                .enable_extension(pandoc::Extension::BracketedSpans)
                                .is_available() =>
                        {
                            self.emit_event(Event::InlineHtml("[".into()), co).await;
                            write_html = false;
                        }
                        &local_name!("div")
                            if (preprocessor.preprocessor.ctx.pandoc)
                                .enable_extension(pandoc::Extension::FencedDivs)
                                .is_available() =>
                        {
                            write_html = false;
                            let attrs = element.attrs.clone();
                            self.emit_event(
                                Event::Html(
                                    self.open_div(attrs, &mut preprocessor.preprocessor.ctx)
                                        .to_string()
                                        .into(),
                                ),
                                co,
                            )
                            .await;
                        }
                        &local_name!("img") => {
                            let mut attrs = element.attrs.clone();
                            let [src, alt, title] =
                                [local_name!("src"), local_name!("alt"), local_name!("title")]
                                    .map(|attr| attrs.swap_remove(&name(attr)));
                            let Some(src) = src else { break 'element };
                            match preprocessor
                                .resolve_image_url(src.to_string().into(), LinkType::Inline)
                            {
                                Err(UnresolvableRemoteImage) => break 'element,
                                Ok(src) => {
                                    let mut md = String::new();
                                    // TODO: if/when pulldown_cmark supports attributes on images,
                                    // use Tag::Image instead of embedding raw markdown
                                    md.push_str("![");
                                    if let Some(alt) = alt {
                                        md.push_str(&alt);
                                    }
                                    md.push_str("](");
                                    md.push_str(&src);
                                    if let Some(title) = title {
                                        md.push(' ');
                                        md.push('"');
                                        md.push_str(&title);
                                        md.push('"');
                                    }
                                    md.push(')');
                                    self.write_attributes(
                                        attrs,
                                        &mut md,
                                        &mut preprocessor.preprocessor.ctx,
                                    );
                                    self.emit_event(Event::InlineHtml(md.into()), co).await;
                                    break 'element;
                                }
                            }
                        }
                        &local_name!("i") => {
                            let mut attrs = element.attrs.iter();
                            match attrs.next() {
                                Some((attr, val))
                                    if matches!(attr.local, local_name!("class"))
                                        && attrs.next().is_none() =>
                                {
                                    if let Some(icon) = val.strip_prefix("fa fa-") {
                                        if let OutputFormat::Latex { packages } =
                                            &mut preprocessor.preprocessor.ctx.output
                                        {
                                            if (preprocessor.preprocessor.ctx.pandoc)
                                                .enable_extension(pandoc::Extension::RawAttribute)
                                                .is_available()
                                                && !has_children
                                            {
                                                packages.need(latex::Package::FontAwesome);
                                                let md = format!(r"`\faicon{{{icon}}}`{{=latex}}");
                                                self.emit_event(Event::InlineHtml(md.into()), co)
                                                    .await;
                                                skip = true;
                                                break 'element;
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    if write_html {
                        let html = Self::serialize_html(|mut serializer| {
                            serializer.start_elem(
                                element.name.clone(),
                                element.attrs.iter().map(|(attr, val)| (attr, val.as_ref())),
                            )
                        });
                        self.emit_event(Event::Html(html.into()), co).await;
                    }
                    if in_html_block
                        && Self::should_wrap_in_div(&element.name)
                        && (preprocessor.preprocessor.ctx.pandoc)
                            .enable_extension(pandoc::Extension::FencedDivs)
                            .is_available()
                    {
                        // If the format strips raw HTML and the tag contains an `id`, move the
                        // `id` to the wrapper div so links to it don't break
                        let attrs = (!matches!(
                            preprocessor.preprocessor.ctx.output,
                            OutputFormat::HtmlLike
                        ))
                        .then(|| element.attrs.get_key_value(&name(local_name!("id"))))
                        .into_iter()
                        .flatten()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                        self.emit_event(
                            Event::Html(
                                self.open_div(attrs, &mut preprocessor.preprocessor.ctx)
                                    .to_string()
                                    .into(),
                            ),
                            co,
                        )
                        .await;
                    }
                }
                Node::Doctype(_)
                | Node::Document
                | Node::Fragment
                | Node::ProcessingInstruction(_) => {}
            }
            if !skip {
                match node.into_first_child() {
                    Ok(child) => {
                        cursor = Some(child);
                        continue;
                    }
                    Err(no_children) => {
                        node = no_children;
                        if let Node::Element(element) = node.value() {
                            self.finish_element(
                                element,
                                in_html_block,
                                &mut preprocessor.preprocessor.ctx,
                                co,
                            )
                            .await;
                        }
                    }
                }
            }
            cursor = match node.into_next_sibling() {
                Ok(sibling) => Some(sibling),
                Err(mut node) => loop {
                    match node.into_parent() {
                        Err(_) => break None,
                        Ok(parent) if parent.id() == root => break None,
                        Ok(mut parent) => {
                            if let Node::Element(element) = parent.value() {
                                self.finish_element(
                                    element,
                                    in_html_block,
                                    &mut preprocessor.preprocessor.ctx,
                                    co,
                                )
                                .await;
                            }
                            match parent.into_next_sibling() {
                                Ok(sibling) => break Some(sibling),
                                Err(parent) => node = parent,
                            }
                        }
                    }
                },
            };
        }
    }

    fn should_wrap_in_div(name: &QualName) -> bool {
        !matches!(
            name.local,
            local_name!("div")
                | local_name!("code")
                | local_name!("a")
                | local_name!("span")
                | local_name!("em")
                | local_name!("i")
                | local_name!("b")
                | local_name!("strong")
        )
    }

    async fn emit_event(
        &self,
        event: Event<'book>,
        co: &genawaiter::stack::Co<'_, (Event<'book>, Option<Range<usize>>)>,
    ) {
        co.yield_((event, None)).await
    }

    fn serialize_html(
        f: impl FnOnce(html5ever::serialize::HtmlSerializer<&mut Vec<u8>>) -> io::Result<()>,
    ) -> String {
        let mut buf = Vec::new();
        let serializer = html5ever::serialize::HtmlSerializer::new(
            &mut buf,
            html5ever::serialize::SerializeOpts {
                scripting_enabled: false,
                traversal_scope: html5ever::serialize::TraversalScope::IncludeNode,
                create_missing_parent: false,
            },
        );
        f(serializer).unwrap();
        String::from_utf8(buf).unwrap()
    }

    async fn finish_element(
        &mut self,
        element: &mut Element,
        in_html_block: bool,
        ctx: &mut RenderContext<'book>,
        co: &genawaiter::stack::Co<'_, (Event<'book>, Option<Range<usize>>)>,
    ) {
        if element.name == self.event_node_name {
            return;
        }
        println!("Finishing {element:?}");
        match &element.name.local {
            tag @ &(local_name!("a") | local_name!("span"))
                if (ctx.pandoc)
                    .enable_extension(pandoc::Extension::BracketedSpans)
                    .is_available() =>
            {
                let mut attrs = std::mem::take(&mut element.attrs);
                let mut md = String::from("]");
                if matches!(tag, &local_name!("a")) {
                    if let Some(href) = attrs.swap_remove(&name(local_name!("href"))) {
                        write!(&mut md, "({})", &href).unwrap();
                    }
                }
                self.write_attributes(attrs, &mut md, ctx);
                self.emit_event(Event::InlineHtml(md.into()), co).await;
                return;
            }
            &local_name!("div")
                if (ctx.pandoc)
                    .enable_extension(pandoc::Extension::FencedDivs)
                    .is_available() =>
            {
                self.emit_event(Event::Html(self.close_div().to_string().into()), co)
                    .await;
                return;
            }
            &local_name!("img") => return,
            _ => {}
        }
        if in_html_block
            && Self::should_wrap_in_div(&element.name)
            && (ctx.pandoc)
                .enable_extension(pandoc::Extension::FencedDivs)
                .is_available()
        {
            self.emit_event(Event::Html(self.close_div().to_string().into()), co)
                .await;
        }
        let html = Self::serialize_html(|mut serializer| serializer.end_elem(element.name.clone()));
        self.emit_event(Event::Html(html.into()), co).await;
    }

    fn open_div<'a>(
        &'a self,
        attrs: Attributes,
        ctx: &'a mut RenderContext<'book>,
    ) -> OpenDiv<'a, 'book> {
        OpenDiv {
            tree: self,
            attrs: Cell::new(attrs),
            ctx,
        }
    }

    fn close_div(&self) -> CloseDiv {
        CloseDiv {}
    }

    /// Writes [pandoc attributes](https://pandoc.org/MANUAL.html#extension-attributes).
    fn write_attributes(
        &mut self,
        attrs: Attributes,
        writer: &mut String,
        ctx: &mut RenderContext<'book>,
    ) {
        if (ctx.pandoc)
            .enable_extension(pandoc::Extension::Attributes)
            .is_available()
        {
            self.write_attributes_unchecked(attrs, writer, ctx).unwrap();
        }
    }

    /// Writes [pandoc attributes](https://pandoc.org/MANUAL.html#extension-attributes) assuming
    /// the extension is available and enabled.
    fn write_attributes_unchecked<W: fmt::Write>(
        &self,
        mut attrs: Attributes,
        writer: &mut W,
        ctx: &RenderContext<'book>,
    ) -> fmt::Result {
        // Pandoc doesn't parse `{}` as attributes, so add a dummy class
        if attrs.is_empty() {
            return write!(writer, r"{{.mdbook-pandoc}}");
        }

        let [class] = [local_name!("class")].map(|attr| attrs.swap_remove(&name(attr)));

        writer.write_char('{')?;
        let mut write_separator = {
            let mut first = true;
            move |writer: &mut W| {
                if first {
                    first = false;
                } else {
                    writer.write_char(' ')?;
                }
                Ok(())
            }
        };

        let class = class.as_ref().filter(|c| !c.is_empty());
        let classes = || class.into_iter().flat_map(|class| class.split(' '));
        for class in classes() {
            write_separator(writer)?;
            write!(writer, ".{class}")?;
        }

        let mut write_attr = |attr: &_, val: &_| {
            write_separator(writer)?;
            write!(writer, r#"{attr}="{val}""#)
        };

        if !matches!(ctx.output, OutputFormat::HtmlLike) {
            let style = attrs.swap_remove(&name(local_name!("style")));
            let style = style
                .as_ref()
                .into_iter()
                .flat_map(|style| style.split(';'))
                .flat_map(|decl| decl.split_once(':'))
                .map(|(attr, val)| (attr.trim(), val.trim()))
                .chain(
                    classes()
                        .flat_map(|class| ctx.css.styles.classes.get(class).into_iter().flatten())
                        .map(|(prop, val)| (prop.as_ref(), *val)),
                );
            for (prop, val) in style {
                let prop = match prop {
                    "width" => local_name!("width"),
                    "height" => local_name!("height"),
                    _ => continue,
                };
                if !attrs.contains_key(&name(prop.clone())) {
                    write_attr(&prop, val)?;
                }
            }
        }

        for (attr, val) in attrs {
            write_attr(&attr.local, &val)?;
        }

        writer.write_char('}')
    }
}

fn most_recently_created_open_element(parser: &Parser) -> NodeId {
    struct Tracer<'a> {
        html: &'a scraper::Html,
        prev: cell::Cell<Option<NodeId>>,
        next: cell::Cell<Option<NodeId>>,
    }

    impl html5ever::interface::Tracer for Tracer<'_> {
        type Handle = NodeId;

        fn trace_handle(&self, handle: &Self::Handle) {
            if let Some(node) = self.html.tree.get(*handle) {
                if node.value().is_element() {
                    self.prev.swap(&self.next);
                    self.next.set(Some(*handle))
                }
            }
        }
    }

    let sink = &parser.tokenizer.sink;
    let html = sink.sink.0.borrow();
    let tracer = Tracer {
        html: &html,
        prev: Default::default(),
        next: Default::default(),
    };
    sink.trace_handles(&tracer);
    tracer.prev.into_inner().unwrap()
}

impl fmt::Debug for TreeBuilder<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{:#?}", self.parser.tokenizer.sink.sink)?;
        let html = self.parser.tokenizer.sink.sink.0.borrow();
        for (parent, children) in &self.events {
            writeln!(f, "{:?} => [", html.tree.get(*parent).unwrap().value())?;
            for (event, _) in children {
                writeln!(f, "\t{event:?},")?;
            }
            writeln!(f, "]")?;
        }
        Ok(())
    }
}

struct OpenDiv<'a, 'book> {
    tree: &'a Emitter<'book>,
    attrs: Cell<Attributes>,
    ctx: &'a mut RenderContext<'book>,
}

impl Display for OpenDiv<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\n\n::: ")?;
        let attrs = self.attrs.take();
        self.tree.write_attributes_unchecked(attrs, f, self.ctx)?;
        writeln!(f)
    }
}

struct CloseDiv {}

impl Display for CloseDiv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\n:::\n\n")
    }
}
