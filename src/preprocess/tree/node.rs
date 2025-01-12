use std::fmt;

use html5ever::{local_name, namespace_url, ns, tendril::StrTendril, Attribute, QualName};
use indexmap::IndexMap;
use pulldown_cmark::{Alignment, CodeBlockKind, CowStr, HeadingLevel, LinkType};

use crate::html;

/// A node in the tree.
pub enum Node<'book> {
    /// The document root.
    Document,

    /// An HTML comment.
    HtmlComment(StrTendril),

    /// Text in raw HTML.
    HtmlText(StrTendril),

    /// An element.
    Element(Element<'book>),
}

#[derive(Clone)]
pub struct Attributes {
    pub id: Option<StrTendril>,
    pub classes: StrTendril,
    pub rest: IndexMap<QualName, StrTendril>,
}

pub enum Element<'book> {
    Html(HtmlElement),
    Markdown(MdElement<'book>),
}

/// An HTML element.
pub struct HtmlElement {
    /// The element name.
    pub name: QualName,
    /// The element attributes.
    pub attrs: Attributes,
}

#[derive(Debug)]
pub enum MdElement<'a> {
    Paragraph,
    Text(CowStr<'a>),
    SoftBreak,
    Heading {
        level: HeadingLevel,
        id: Option<CowStr<'a>>,
        classes: Vec<CowStr<'a>>,
        attrs: Vec<(CowStr<'a>, Option<CowStr<'a>>)>,
    },
    BlockQuote,
    InlineCode(CowStr<'a>),
    CodeBlock(CodeBlockKind<'a>),
    List(Option<u64>),
    Item,
    TaskListMarker(bool),
    FootnoteDefinition,
    FootnoteReference(CowStr<'a>),
    Table {
        alignment: Vec<Alignment>,
        source: &'a str,
    },
    Emphasis,
    Strong,
    Strikethrough,
    Link {
        dest_url: CowStr<'a>,
        title: CowStr<'a>,
    },
    Image {
        link_type: LinkType,
        dest_url: CowStr<'a>,
        title: CowStr<'a>,
        id: CowStr<'a>,
    },
}

pub trait QualNameExt {
    /// Is this the name of a [void element](https://developer.mozilla.org/en-US/docs/Glossary/Void_element)?
    fn is_void_element(&self) -> bool;

    /// Does this element default to `display: block`?
    fn is_display_block(&self) -> bool;
}

impl QualNameExt for QualName {
    fn is_void_element(&self) -> bool {
        self.ns == ns!(html)
            && matches!(
                self.local,
                local_name!("area")
                    | local_name!("base")
                    | local_name!("basefont")
                    | local_name!("bgsound")
                    | local_name!("br")
                    | local_name!("col")
                    | local_name!("embed")
                    | local_name!("frame")
                    | local_name!("hr")
                    | local_name!("img")
                    | local_name!("input")
                    | local_name!("keygen")
                    | local_name!("link")
                    | local_name!("meta")
                    | local_name!("param")
                    | local_name!("source")
                    | local_name!("track")
                    | local_name!("wbr")
            )
    }

    // Taken from https://www.w3schools.com/cssref/css_default_values.php and filtered down to
    // those that are likely to appear in mdbooks.
    fn is_display_block(&self) -> bool {
        self.ns == ns!(html)
            && matches!(
                self.local,
                local_name!("address")
                    | local_name!("article")
                    | local_name!("aside")
                    | local_name!("blockquote")
                    | local_name!("dd")
                    | local_name!("details")
                    | local_name!("div")
                    | local_name!("dl")
                    | local_name!("dt")
                    | local_name!("figcaption")
                    | local_name!("figure")
                    | local_name!("h1")
                    | local_name!("h2")
                    | local_name!("h3")
                    | local_name!("h4")
                    | local_name!("h5")
                    | local_name!("h6")
                    | local_name!("hr")
                    | local_name!("legend")
                    | local_name!("ol")
                    | local_name!("p")
                    | local_name!("pre")
                    | local_name!("section")
                    | local_name!("summary")
                    | local_name!("ul")
            )
    }
}

impl Element<'_> {
    pub fn name(&self) -> &QualName {
        match self {
            Self::Html(element) => &element.name,
            Self::Markdown(element) => element.name(),
        }
    }
}

impl MdElement<'_> {
    pub fn name(&self) -> &QualName {
        match self {
            MdElement::Paragraph => {
                const P: &QualName = &html::name!(html "p");
                P
            }
            MdElement::Text(_) => {
                const SPAN: &QualName = &html::name!(html "span");
                SPAN
            }
            MdElement::SoftBreak => {
                const BR: &QualName = &html::name!(html "br");
                BR
            }
            MdElement::List(None) => {
                const UL: &QualName = &html::name!(html "ul");
                UL
            }
            MdElement::List(Some(_)) => {
                const OL: &QualName = &html::name!(html "ol");
                OL
            }
            MdElement::Item => {
                const LI: &QualName = &html::name!(html "li");
                LI
            }
            MdElement::Table { .. } => {
                const TABLE: &QualName = &html::name!(html "table");
                TABLE
            }
            MdElement::Link { .. } => {
                const A: &QualName = &html::name!(html "a");
                A
            }
            MdElement::FootnoteDefinition => {
                // Pretend footnote definitions are <span>s to fit them
                // into the HTML parser's view of the world.
                const SPAN: &QualName = &html::name!(html "span");
                SPAN
            }
            MdElement::FootnoteReference(_) => {
                const SUP: &QualName = &html::name!(html "sup");
                SUP
            }
            MdElement::Heading { level, .. } => {
                const H1: &QualName = &html::name!(html "h1");
                const H2: &QualName = &html::name!(html "h2");
                const H3: &QualName = &html::name!(html "h3");
                const H4: &QualName = &html::name!(html "h4");
                const H5: &QualName = &html::name!(html "h5");
                const H6: &QualName = &html::name!(html "h6");
                match level {
                    HeadingLevel::H1 => H1,
                    HeadingLevel::H2 => H2,
                    HeadingLevel::H3 => H3,
                    HeadingLevel::H4 => H4,
                    HeadingLevel::H5 => H5,
                    HeadingLevel::H6 => H6,
                }
            }
            MdElement::BlockQuote => {
                const BLOCKQUOTE: &QualName = &html::name!(html "blockquote");
                BLOCKQUOTE
            }
            MdElement::CodeBlock(_) => {
                const PRE: &QualName = &html::name!(html "pre");
                PRE
            }
            MdElement::Emphasis => {
                const EM: &QualName = &html::name!(html "em");
                EM
            }
            MdElement::Strong => {
                const STRONG: &QualName = &html::name!(html "strong");
                STRONG
            }
            MdElement::Strikethrough => {
                const S: &QualName = &html::name!(html "s");
                S
            }
            MdElement::Image { .. } => {
                // <img> is a void element in HTML (can have no children),
                // but in Markdown the "alt text" *can* contain children.
                // Therefore, we pretend images are <span>s so the parser
                // lets us add children.
                const SPAN: &QualName = &html::name!(html "span");
                SPAN
            }
            MdElement::InlineCode(_) => {
                const CODE: &QualName = &html::name!(html "code");
                CODE
            }
            MdElement::TaskListMarker(_) => {
                const INPUT: &QualName = &html::name!(html "input");
                INPUT
            }
        }
    }
}

impl HtmlElement {
    pub fn new(name: QualName, attributes: Vec<Attribute>) -> Self {
        let mut attrs = Attributes {
            id: None,
            classes: StrTendril::new(),
            rest: IndexMap::with_capacity(attributes.len()),
        };
        for attr in attributes {
            match attr.name.local {
                local_name!("id") => {
                    attrs.id = Some(attr.value);
                }
                local_name!("class") => {
                    attrs.classes = attr.value;
                }
                _ => {
                    attrs.rest.insert(attr.name, attr.value);
                }
            }
        }
        HtmlElement { name, attrs }
    }
}

impl Attributes {
    pub fn iter(&self) -> impl Iterator<Item = (&QualName, &StrTendril)> {
        const ID: &QualName = &html::name!("id");
        const CLASS: &QualName = &html::name!("class");
        (self.id.as_ref().map(|id| (ID, id)).into_iter())
            .chain((!self.classes.is_empty()).then_some((CLASS, &self.classes)))
            .chain(&self.rest)
    }
}

impl fmt::Debug for Node<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::Document => write!(f, "Document"),
            Node::HtmlComment(comment) => write!(f, "<!-- {comment} -->"),
            Node::HtmlText(text) => write!(f, "Text({text})"),
            Node::Element(element) => write!(f, "{element:?}"),
        }
    }
}

impl fmt::Debug for Element<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Element::Html(element) => write!(f, "{element:?}"),
            Element::Markdown(element) => write!(f, "{element:?}"),
        }
    }
}

impl fmt::Debug for HtmlElement {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "<{}", self.name.local)?;
        if !self.attrs.classes.is_empty() {
            write!(f, r#" class="{}""#, self.attrs.classes)?;
        }
        for (name, value) in &self.attrs.rest {
            write!(f, r#" {}="{value}""#, name.local)?;
        }
        write!(f, ">")
    }
}
