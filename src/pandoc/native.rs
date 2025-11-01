use std::{
    io::{self, Write},
    iter,
};

use anyhow::anyhow;
use escape::Escape;
use html5ever::serialize::HtmlSerializer;
use indexmap::IndexSet;
use pulldown_cmark::CowStr;

use crate::{
    css,
    latex::MathType,
    preprocess::{self, PreprocessChapter},
};

use super::OutputFormat;

pub mod escape;

/// Alignment of a table column.
pub enum Alignment {
    Default,
    Left,
    Center,
    Right,
}

/// The width of a table column, as a percentage of the text width.
pub struct ColWidth(pub f64);

pub trait Attributes {
    fn id(&self) -> Option<&str>;
    fn classes(&self) -> impl Iterator<Item = &str>;
    fn attrs(&self) -> impl Iterator<Item = (&str, &str)>;

    fn css_properties<'a>(
        &'a self,
        css: &'a css::Styles,
    ) -> impl Iterator<Item = (&'a str, &'a str)> {
        self.attrs()
            .filter_map(|(attr, val)| {
                (attr == "style").then_some(
                    val.split(';')
                        .flat_map(|decl| decl.split_once(':'))
                        .map(|(attr, val)| (attr.trim(), val.trim())),
                )
            })
            .flatten()
            .chain(
                self.classes()
                    .filter_map(|class| css.classes.get(class))
                    .flat_map(|props| props.iter().map(|(k, v)| (k.as_ref(), *v))),
            )
    }
}

impl Attributes for () {
    fn id(&self) -> Option<&str> {
        None
    }

    fn classes(&self) -> impl Iterator<Item = &str> {
        iter::empty()
    }

    fn attrs(&self) -> impl Iterator<Item = (&str, &str)> {
        iter::empty()
    }
}

impl<'a, Classes, Attrs> Attributes for (Option<&str>, &'a Classes, &'a Attrs)
where
    Classes: ?Sized,
    Attrs: ?Sized,
    &'a Classes: AsRef<[CowStr<'a>]>,
    &'a Attrs: AsRef<[(CowStr<'a>, Option<CowStr<'a>>)]>,
{
    fn id(&self) -> Option<&str> {
        self.0
    }

    fn classes(&self) -> impl Iterator<Item = &str> {
        self.1.as_ref().iter().map(|s| s.as_ref())
    }

    fn attrs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.2
            .as_ref()
            .iter()
            .map(|(k, v)| (k.as_ref(), v.as_ref().map_or("", |s| s.as_ref())))
    }
}

impl Attributes for preprocess::tree::Attributes {
    fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn classes(&self) -> impl Iterator<Item = &str> {
        (!self.classes.is_empty())
            .then_some(self.classes.split_ascii_whitespace())
            .into_iter()
            .flatten()
    }

    fn attrs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.rest
            .iter()
            .map(|(name, value)| (name.local.as_ref(), value.as_ref()))
    }
}

impl<T: Attributes> Attributes for &T {
    fn id(&self) -> Option<&str> {
        (*self).id()
    }

    fn classes(&self) -> impl Iterator<Item = &str> {
        (*self).classes()
    }

    fn attrs(&self) -> impl Iterator<Item = (&str, &str)> {
        (*self).attrs()
    }
}

pub struct Serializer<'p, 'book, W: io::Write> {
    html: HtmlSerializer<escape::Writer<W>>,
    pub preprocessor: PreprocessChapter<'p, 'book>,
    /// Footnotes currently being serialized.
    pub footnotes: IndexSet<String>,
}

pub enum SerializeNested<'a, 'serializer, 'book, 'p, W: io::Write> {
    Blocks(&'a mut SerializeBlocks<'serializer, 'book, 'p, W>),
    BlocksSerializingInlines {
        serializer: &'a mut SerializeBlocks<'serializer, 'book, 'p, W>,
        first: bool,
    },
    Inlines(&'a mut SerializeInlines<'serializer, 'book, 'p, W>),
}

impl<'p, 'book, W: io::Write> Serializer<'p, 'book, W> {
    pub fn serialize(
        writer: W,
        preprocessor: PreprocessChapter<'p, 'book>,
        blocks: impl FnOnce(&mut SerializeBlocks<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut serializer = Self {
            preprocessor,
            footnotes: Default::default(),
            html: html5ever::serialize::HtmlSerializer::new(
                escape::Writer::new(writer),
                html5ever::serialize::SerializeOpts {
                    scripting_enabled: false,
                    traversal_scope: html5ever::serialize::TraversalScope::IncludeNode,
                    create_missing_parent: false,
                },
            ),
        };
        let mut block_serializer = SerializeList::new(&mut serializer, Block)?;
        blocks(&mut block_serializer)?;
        block_serializer.finish()
    }

    pub fn escaped(&mut self) -> &mut escape::Writer<W> {
        &mut self.html.writer
    }

    pub fn unescaped(&mut self) -> &mut W {
        self.html.writer.unescaped()
    }

    pub fn write_attributes(&mut self, attrs: impl Attributes) -> anyhow::Result<()> {
        write!(
            self.unescaped(),
            r#"("{}", "#,
            attrs.id().unwrap_or("").escape_quotes()
        )?;

        let mut attributes = SerializeList::new(self, Text)?;
        for class in attrs.classes() {
            attributes.serialize_element()?.serialize_text(class)?;
        }
        attributes.finish()?;

        write!(self.unescaped(), ", ")?;

        let mut attributes = SerializeList::new(self, Attribute)?;

        if matches!(
            attributes.serializer.preprocessor.preprocessor.ctx.output,
            OutputFormat::HtmlLike
        ) {
            for (attr, val) in attrs.attrs() {
                attributes
                    .serialize_element()?
                    .serialize_attribute(attr, val)?;
            }
        } else {
            let css = attributes.serializer.preprocessor.preprocessor.ctx.css;
            for (prop, val) in attrs.css_properties(&css.styles) {
                if matches!(prop, "width" | "height") {
                    attributes
                        .serialize_element()?
                        .serialize_attribute(prop, val)?;
                }
            }
            for (attr, val) in attrs.attrs() {
                if attr != "style" {
                    attributes
                        .serialize_element()?
                        .serialize_attribute(attr, val)?;
                }
            }
        }

        attributes.finish()?;
        write!(self.unescaped(), ")")?;
        Ok(())
    }
}

impl<'serializer, 'book, 'p, W: io::Write> SerializeBlocks<'serializer, 'book, 'p, W> {
    pub fn serialize_nested(
        &mut self,
        f: impl FnOnce(&mut SerializeNested<'_, 'serializer, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut nested = SerializeNested::Blocks(self);
        f(&mut nested)?;
        nested.finish()
    }
}

impl<'serializer, 'book, 'p, W: io::Write> SerializeInlines<'serializer, 'book, 'p, W> {
    pub fn serialize_nested(
        &mut self,
        f: impl FnOnce(&mut SerializeNested<'_, 'serializer, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut nested = SerializeNested::Inlines(self);
        f(&mut nested)?;
        nested.finish()
    }
}

impl<'serializer, 'book, 'p, W: io::Write> SerializeNested<'_, 'serializer, 'book, 'p, W> {
    pub fn serializer(&mut self) -> &mut Serializer<'p, 'book, W> {
        match self {
            Self::Blocks(serializer) => serializer.serializer,
            Self::BlocksSerializingInlines { serializer, .. } => serializer.serializer,
            Self::Inlines(serializer) => serializer.serializer,
        }
    }

    pub fn preprocessor(&mut self) -> &mut PreprocessChapter<'p, 'book> {
        &mut self.serializer().preprocessor
    }

    pub fn is_blocks(&self) -> bool {
        matches!(
            self,
            Self::Blocks(_) | Self::BlocksSerializingInlines { .. }
        )
    }

    pub fn blocks(&mut self) -> anyhow::Result<&mut SerializeBlocks<'serializer, 'book, 'p, W>> {
        replace_with::replace_with_or_abort_and_return(self, |nested| match nested {
            Self::BlocksSerializingInlines {
                serializer,
                first: _,
            } => {
                let ret = write!(serializer.serializer.unescaped(), "]");
                (ret, Self::Blocks(serializer))
            }
            nested => (Ok(()), nested),
        })?;
        match self {
            Self::Blocks(blocks) => Ok(blocks),
            Self::BlocksSerializingInlines { .. } => unreachable!(),
            Self::Inlines(_) => Err(anyhow!("block content in an inline context")),
        }
    }

    pub fn serialize_raw_html(
        &mut self,
        f: impl FnOnce(&mut html5ever::serialize::HtmlSerializer<escape::Writer<W>>) -> io::Result<()>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Blocks(serializer) => serializer.serialize_element()?.serialize_raw_html(f),
            Self::BlocksSerializingInlines { serializer, first } => {
                if *first {
                    *first = false;
                } else {
                    write!(serializer.serializer.unescaped(), ", ")?;
                }
                SerializeInline {
                    serializer: serializer.serializer,
                }
                .serialize_raw_html(f)
            }
            Self::Inlines(serializer) => serializer.serialize_element()?.serialize_raw_html(f),
        }
    }

    pub fn serialize_inlines(
        &mut self,
        inlines: impl FnOnce(&mut SerializeInlines<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        replace_with::replace_with_or_abort_and_return(self, |nested| match nested {
            Self::Blocks(serializer) => {
                let ret = serializer.serialize_element().and_then(|serializer| {
                    write!(serializer.serializer.unescaped(), "Plain [")?;
                    Ok(())
                });
                let serializer = Self::BlocksSerializingInlines {
                    serializer,
                    first: true,
                };
                (ret, serializer)
            }
            nested => (Ok(()), nested),
        })?;
        match self {
            SerializeNested::Blocks(_) => unreachable!(),
            SerializeNested::BlocksSerializingInlines { serializer, first } => {
                let mut serializer = SerializeList {
                    serializer: serializer.serializer,
                    first: *first,
                    element: Inline,
                };
                inlines(&mut serializer)?;
                *first = serializer.first;
                Ok(())
            }
            SerializeNested::Inlines(serializer) => inlines(serializer),
        }
    }

    fn finish(self) -> anyhow::Result<()> {
        match self {
            SerializeNested::BlocksSerializingInlines {
                serializer,
                first: _,
            } => {
                write!(serializer.serializer.unescaped(), "]")?;
                Ok(())
            }
            SerializeNested::Blocks(_) | SerializeNested::Inlines(_) => Ok(()),
        }
    }
}

pub trait SerializeElement {
    type Serializer<'a, 'book: 'a, 'p: 'a + 'book, W: io::Write + 'a>;

    fn serializer<'a, 'book, 'p, W: io::Write>(
        &mut self,
        serializer: &'a mut Serializer<'p, 'book, W>,
    ) -> Self::Serializer<'a, 'book, 'p, W>;
}

impl SerializeElement for Block {
    type Serializer<'a, 'book: 'a, 'p: 'a + 'book, W: io::Write + 'a> =
        SerializeBlock<'a, 'book, 'p, W>;

    fn serializer<'a, 'book, 'p, W: io::Write>(
        &mut self,
        serializer: &'a mut Serializer<'p, 'book, W>,
    ) -> Self::Serializer<'a, 'book, 'p, W> {
        SerializeBlock { serializer }
    }
}

impl SerializeElement for Inline {
    type Serializer<'a, 'book: 'a, 'p: 'a + 'book, W: io::Write + 'a> =
        SerializeInline<'a, 'book, 'p, W>;

    fn serializer<'a, 'book, 'p, W: io::Write>(
        &mut self,
        serializer: &'a mut Serializer<'p, 'book, W>,
    ) -> Self::Serializer<'a, 'book, 'p, W> {
        SerializeInline { serializer }
    }
}

impl SerializeElement for Attribute {
    type Serializer<'a, 'book: 'a, 'p: 'a + 'book, W: io::Write + 'a> =
        SerializeAttribute<'a, 'book, 'p, W>;

    fn serializer<'a, 'book, 'p, W: io::Write>(
        &mut self,
        serializer: &'a mut Serializer<'p, 'book, W>,
    ) -> Self::Serializer<'a, 'book, 'p, W> {
        SerializeAttribute { serializer }
    }
}

impl SerializeElement for Text {
    type Serializer<'a, 'book: 'a, 'p: 'a + 'book, W: io::Write + 'a> =
        SerializeText<'a, 'book, 'p, W>;

    fn serializer<'a, 'book, 'p, W: io::Write>(
        &mut self,
        serializer: &'a mut Serializer<'p, 'book, W>,
    ) -> Self::Serializer<'a, 'book, 'p, W> {
        SerializeText { serializer }
    }
}

impl SerializeElement for TableBody {
    type Serializer<'a, 'book: 'a, 'p: 'a + 'book, W: io::Write + 'a> =
        SerializeTableBody<'a, 'book, 'p, W>;

    fn serializer<'a, 'book, 'p, W: io::Write>(
        &mut self,
        serializer: &'a mut Serializer<'p, 'book, W>,
    ) -> Self::Serializer<'a, 'book, 'p, W> {
        SerializeTableBody { serializer }
    }
}
impl SerializeElement for Row {
    type Serializer<'a, 'book: 'a, 'p: 'a + 'book, W: io::Write + 'a> =
        SerializeRow<'a, 'book, 'p, W>;

    fn serializer<'a, 'book, 'p, W: io::Write>(
        &mut self,
        serializer: &'a mut Serializer<'p, 'book, W>,
    ) -> Self::Serializer<'a, 'book, 'p, W> {
        SerializeRow { serializer }
    }
}
impl SerializeElement for Cell {
    type Serializer<'a, 'book: 'a, 'p: 'a + 'book, W: io::Write + 'a> =
        SerializeCell<'a, 'book, 'p, W>;

    fn serializer<'a, 'book, 'p, W: io::Write>(
        &mut self,
        serializer: &'a mut Serializer<'p, 'book, W>,
    ) -> Self::Serializer<'a, 'book, 'p, W> {
        SerializeCell { serializer }
    }
}

impl SerializeElement for DefinitionListItem {
    type Serializer<'a, 'book: 'a, 'p: 'a + 'book, W: io::Write + 'a> =
        SerializeDefinitionListItem<'a, 'book, 'p, W>;

    fn serializer<'a, 'book, 'p, W: io::Write>(
        &mut self,
        serializer: &'a mut Serializer<'p, 'book, W>,
    ) -> Self::Serializer<'a, 'book, 'p, W> {
        SerializeDefinitionListItem { serializer }
    }
}

impl<Item: SerializeElement + Copy> SerializeElement for List<Item> {
    type Serializer<'a, 'book: 'a, 'p: 'a + 'book, W: io::Write + 'a> =
        anyhow::Result<SerializeList<'a, 'book, 'p, W, Item>>;

    fn serializer<'a, 'book, 'p, W: io::Write>(
        &mut self,
        serializer: &'a mut Serializer<'p, 'book, W>,
    ) -> Self::Serializer<'a, 'book, 'p, W> {
        SerializeList::new(serializer, self.0)
    }
}

#[must_use]
pub struct SerializeList<'a, 'book, 'p, W: io::Write, Element> {
    pub serializer: &'a mut Serializer<'p, 'book, W>,
    first: bool,
    element: Element,
}

impl<'a, 'book, 'p, W: io::Write, Element> SerializeList<'a, 'book, 'p, W, Element> {
    fn new(serializer: &'a mut Serializer<'p, 'book, W>, element: Element) -> anyhow::Result<Self> {
        write!(serializer.unescaped(), "[")?;
        Ok(Self {
            serializer,
            first: true,
            element,
        })
    }

    pub fn serialize_element(&mut self) -> anyhow::Result<Element::Serializer<'_, 'book, 'p, W>>
    where
        Element: SerializeElement,
    {
        if self.first {
            self.first = false;
        } else {
            write!(self.serializer.unescaped(), ", ")?;
        }
        Ok(self.element.serializer(self.serializer))
    }

    pub fn finish(self) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "]")?;
        Ok(())
    }
}

#[derive(Copy, Clone)]
pub struct Block;
#[derive(Copy, Clone)]
pub struct Inline;
#[derive(Copy, Clone)]
pub struct Attribute;
#[derive(Copy, Clone)]
pub struct Text;
#[derive(Copy, Clone)]
pub struct List<T>(T);
#[derive(Copy, Clone)]
pub struct TableBody;
#[derive(Copy, Clone)]
pub struct Row;
#[derive(Copy, Clone)]
pub struct Cell;
#[derive(Copy, Clone)]
pub struct DefinitionListItem;

pub type SerializeBlocks<'a, 'book, 'p, W> = SerializeList<'a, 'book, 'p, W, Block>;
pub type SerializeInlines<'a, 'book, 'p, W> = SerializeList<'a, 'book, 'p, W, Inline>;
pub type SerializeTableBodies<'a, 'book, 'p, W> = SerializeList<'a, 'book, 'p, W, TableBody>;
pub type SerializeRows<'a, 'book, 'p, W> = SerializeList<'a, 'book, 'p, W, Row>;
pub type SerializeCells<'a, 'book, 'p, W> = SerializeList<'a, 'book, 'p, W, Cell>;

#[must_use]
pub struct SerializeBlock<'a, 'book, 'p, W: io::Write> {
    serializer: &'a mut Serializer<'p, 'book, W>,
}

#[must_use]
pub struct SerializeInline<'a, 'book, 'p, W: io::Write> {
    serializer: &'a mut Serializer<'p, 'book, W>,
}

#[must_use]
pub struct SerializeAttribute<'a, 'book, 'p, W: io::Write> {
    serializer: &'a mut Serializer<'p, 'book, W>,
}

#[must_use]
pub struct SerializeText<'a, 'book, 'p, W: io::Write> {
    serializer: &'a mut Serializer<'p, 'book, W>,
}

#[must_use]
pub struct SerializeCode<'a, 'book, 'p, W: io::Write> {
    serializer: &'a mut Serializer<'p, 'book, W>,
}

#[must_use]
pub struct SerializeTableBody<'a, 'book, 'p, W: io::Write> {
    serializer: &'a mut Serializer<'p, 'book, W>,
}

#[must_use]
pub struct SerializeRow<'a, 'book, 'p, W: io::Write> {
    serializer: &'a mut Serializer<'p, 'book, W>,
}

#[must_use]
pub struct SerializeCell<'a, 'book, 'p, W: io::Write> {
    serializer: &'a mut Serializer<'p, 'book, W>,
}

#[must_use]
pub struct SerializeDefinitionListItem<'a, 'book, 'p, W: io::Write> {
    serializer: &'a mut Serializer<'p, 'book, W>,
}

impl<'book, 'p, W: io::Write> SerializeInline<'_, 'book, 'p, W> {
    /// Text (string)
    pub fn serialize_str(self, s: &str) -> anyhow::Result<()> {
        write!(
            self.serializer.unescaped(),
            r#"Str "{}""#,
            s.escape_quotes()
        )?;
        Ok(())
    }

    /// Unescaped text (string)
    pub fn serialize_str_unescaped(self, s: &str) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), r#"Str "{s}""#)?;
        Ok(())
    }

    /// Emphasized text (list of inlines)
    pub fn serialize_emph(
        self,
        inlines: impl FnOnce(&mut SerializeInlines<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Emph ")?;
        let mut serializer = SerializeList::new(self.serializer, Inline)?;
        inlines(&mut serializer)?;
        serializer.finish()
    }

    /// Strongly emphasized text (list of inlines)
    pub fn serialize_strong(
        self,
        inlines: impl FnOnce(&mut SerializeInlines<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Strong ")?;
        let mut serializer = SerializeList::new(self.serializer, Inline)?;
        inlines(&mut serializer)?;
        serializer.finish()
    }

    /// Strikeout text (list of inlines)
    pub fn serialize_strikeout(
        self,
        inlines: impl FnOnce(&mut SerializeInlines<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Strikeout ")?;
        let mut serializer = SerializeList::new(self.serializer, Inline)?;
        inlines(&mut serializer)?;
        serializer.finish()
    }

    /// Superscripted text (list of inlines)
    pub fn serialize_superscript(
        self,
        inlines: impl FnOnce(&mut SerializeInlines<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Superscript ")?;
        let mut serializer = SerializeList::new(self.serializer, Inline)?;
        inlines(&mut serializer)?;
        serializer.finish()
    }

    /// Subscripted text (list of inlines)
    pub fn serialize_subscript(
        self,
        inlines: impl FnOnce(&mut SerializeInlines<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Subscript ")?;
        let mut serializer = SerializeList::new(self.serializer, Inline)?;
        inlines(&mut serializer)?;
        serializer.finish()
    }

    /// Inline code (literal)
    pub fn serialize_code(self, attrs: impl Attributes, code: &str) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Code ")?;
        self.serializer.write_attributes(attrs)?;
        write!(
            self.serializer.unescaped(),
            r#" "{}""#,
            code.escape_quotes_verbatim()
        )?;
        Ok(())
    }

    /// Inter-word space
    pub fn serialize_space(self) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Space")?;
        Ok(())
    }

    /// Soft line break
    pub fn serialize_soft_break(self) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "SoftBreak")?;
        Ok(())
    }

    /// Hard line break
    pub fn serialize_line_break(self) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "LineBreak")?;
        Ok(())
    }

    /// TeX math (literal)
    pub fn serialize_math(self, ty: MathType, math: &str) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Math ")?;
        let ty = match ty {
            MathType::Display => "DisplayMath",
            MathType::Inline => "InlineMath",
        };
        write!(self.serializer.unescaped(), "{ty}")?;
        write!(
            self.serializer.unescaped(),
            r#" "{}""#,
            math.escape_quotes_verbatim()
        )?;
        Ok(())
    }

    /// Raw inline
    pub fn serialize_raw_inline(
        self,
        format: &str,
        raw: impl FnOnce(&mut escape::Writer<W>) -> io::Result<()>,
    ) -> anyhow::Result<()> {
        write!(
            self.serializer.unescaped(),
            r#"RawInline (Format "{}") "#,
            format.escape_quotes()
        )?;
        let writer = self.serializer.escaped();
        writer.start_text()?;
        raw(writer)?;
        writer.end_text()?;
        Ok(())
    }

    pub fn serialize_raw_html(
        self,
        f: impl FnOnce(&mut html5ever::serialize::HtmlSerializer<escape::Writer<W>>) -> io::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), r#"RawInline (Format "html") "#)?;
        self.serializer.escaped().start_text()?;
        f(&mut self.serializer.html)?;
        self.serializer.escaped().end_text()?;
        Ok(())
    }

    /// Hyperlink: alt text (list of inlines), target
    pub fn serialize_link(
        self,
        attrs: impl Attributes,
        alt: impl FnOnce(&mut SerializeInlines<'_, 'book, 'p, W>) -> anyhow::Result<()>,
        target: &str,
        title: &str,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Link ")?;
        self.serializer.write_attributes(attrs)?;
        write!(self.serializer.unescaped(), " ")?;
        let mut serializer = SerializeList::new(self.serializer, Inline)?;
        alt(&mut serializer)?;
        serializer.finish()?;
        write!(
            self.serializer.unescaped(),
            r#" ("{}", "{}")"#,
            target.escape_quotes(),
            title.escape_quotes()
        )?;
        Ok(())
    }

    /// Image: alt text (list of inlines), target
    pub fn serialize_image(
        self,
        attrs: impl Attributes,
        alt: impl FnOnce(&mut SerializeInlines<'_, 'book, 'p, W>) -> anyhow::Result<()>,
        target: &str,
        title: &str,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Image ")?;
        self.serializer.write_attributes(attrs)?;
        write!(self.serializer.unescaped(), " ")?;
        let mut serializer = SerializeList::new(self.serializer, Inline)?;
        alt(&mut serializer)?;
        serializer.finish()?;
        write!(
            self.serializer.unescaped(),
            r#" ("{}", "{}")"#,
            target.escape_quotes(),
            title.escape_quotes()
        )?;
        Ok(())
    }

    /// Footnote or endnote
    pub fn serialize_note(
        self,
        note: impl FnOnce(&mut SerializeBlocks<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Note ")?;
        let mut serializer = SerializeList::new(self.serializer, Block)?;
        note(&mut serializer)?;
        serializer.finish()
    }

    /// Generic inline container with attributes
    pub fn serialize_span(
        self,
        attrs: impl Attributes,
        inlines: impl FnOnce(&mut SerializeInlines<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Span ")?;
        self.serializer.write_attributes(attrs)?;
        write!(self.serializer.unescaped(), " ")?;
        let mut serializer = SerializeList::new(self.serializer, Inline)?;
        inlines(&mut serializer)?;
        serializer.finish()
    }
}

impl<'a, 'book, 'p, W: io::Write> SerializeBlock<'a, 'book, 'p, W> {
    /// Paragraph
    pub fn serialize_para(
        self,
        inlines: impl FnOnce(&mut SerializeInlines<'a, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Para ")?;
        let mut serializer = SerializeList::new(self.serializer, Inline)?;
        inlines(&mut serializer)?;
        serializer.finish()
    }

    /// Code block (literal) with attributes
    pub fn serialize_code_block(
        self,
        attrs: impl Attributes,
        code: impl FnOnce(&mut SerializeCode<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "CodeBlock ")?;
        self.serializer.write_attributes(attrs)?;
        write!(self.serializer.unescaped(), r#" ""#)?;
        code(&mut SerializeCode {
            serializer: self.serializer,
        })?;
        write!(self.serializer.unescaped(), r#"""#)?;
        Ok(())
    }

    /// Raw block
    pub fn serialize_raw_block(
        self,
        format: &str,
        raw: impl FnOnce(&mut SerializeCode<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(
            self.serializer.unescaped(),
            r#"RawBlock (Format "{format}") ""#
        )?;
        raw(&mut SerializeCode {
            serializer: self.serializer,
        })?;
        write!(self.serializer.unescaped(), r#"""#)?;
        Ok(())
    }

    /// Raw HTML block
    pub fn serialize_raw_html(
        self,
        f: impl FnOnce(&mut html5ever::serialize::HtmlSerializer<escape::Writer<W>>) -> io::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), r#"RawBlock (Format "html") "#)?;
        self.serializer.escaped().start_text()?;
        f(&mut self.serializer.html)?;
        self.serializer.escaped().end_text()?;
        Ok(())
    }

    /// Block quote (list of blocks)
    pub fn serialize_block_quote(
        self,
        blocks: impl FnOnce(&mut SerializeBlocks<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "BlockQuote ")?;
        let mut serializer = SerializeList::new(self.serializer, Block)?;
        blocks(&mut serializer)?;
        serializer.finish()
    }

    /// Ordered list (attributes and a list of items, each a list of blocks)
    pub fn serialize_ordered_list(
        self,
        start: u64,
        items: impl FnOnce(&mut SerializeList<'_, 'book, 'p, W, List<Block>>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(
            self.serializer.unescaped(),
            "OrderedList ({start}, DefaultStyle, DefaultDelim) "
        )?;
        let mut serializer = SerializeList::new(self.serializer, List(Block))?;
        items(&mut serializer)?;
        serializer.finish()
    }

    /// Bullet list (list of items, each a list of blocks)
    pub fn serialize_bullet_list(
        self,
        items: impl FnOnce(&mut SerializeList<'_, 'book, 'p, W, List<Block>>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "BulletList ")?;
        let mut serializer = SerializeList::new(self.serializer, List(Block))?;
        items(&mut serializer)?;
        serializer.finish()
    }

    /// Definition list. Each list item is a pair consisting of a term (a list of inlines)
    /// and one or more definitions (each a list of blocks)
    pub fn serialize_definition_list(
        self,
        items: impl FnOnce(
            &mut SerializeList<'_, 'book, 'p, W, DefinitionListItem>,
        ) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "DefinitionList ")?;
        let mut serializer = SerializeList::new(self.serializer, DefinitionListItem)?;
        items(&mut serializer)?;
        serializer.finish()
    }

    /// Horizontal rule
    pub fn serialize_horizontal_rule(self) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "HorizontalRule")?;
        Ok(())
    }

    /// Header - level (integer) and text (inlines)
    pub fn serialize_header(
        self,
        level: u16,
        attrs: impl Attributes,
        inlines: impl FnOnce(&mut SerializeInlines<'a, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Header {level} ")?;
        self.serializer.write_attributes(attrs)?;
        self.serializer.unescaped().write_all(b" ")?;
        let mut serializer = SerializeList::new(self.serializer, Inline)?;
        inlines(&mut serializer)?;
        serializer.finish()
    }

    /// Table, with attributes, caption, optional short caption, column alignments and widths
    /// (required), table head, table bodies, and table foot
    pub fn serialize_table(
        self,
        attrs: impl Attributes,
        cols: impl IntoIterator<Item = (Alignment, Option<ColWidth>)>,
        header: (
            impl Attributes,
            impl FnOnce(&mut SerializeRows<'_, 'book, 'p, W>) -> anyhow::Result<()>,
        ),
        bodies: impl FnOnce(&mut SerializeTableBodies<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Table ")?;

        // Attributes
        self.serializer.write_attributes(attrs)?;

        // Caption: (Caption (Maybe ShortCaption) [Block])
        write!(self.serializer.unescaped(), " (Caption Nothing [])")?;

        // Column specs
        write!(self.serializer.unescaped(), " [")?;
        let mut cols = cols.into_iter();
        let write_col = |(align, width): (Alignment, Option<ColWidth>), writer: &mut W| {
            write!(writer, "({}, ", align.to_native())?;
            match width {
                Some(ColWidth(width)) => write!(writer, "(ColWidth {width})"),
                None => write!(writer, "ColWidthDefault"),
            }?;
            write!(writer, ")")
        };
        if let Some(col) = cols.next() {
            write_col(col, self.serializer.unescaped())?;
        }
        for col in cols {
            write!(self.serializer.unescaped(), ", ")?;
            write_col(col, self.serializer.unescaped())?;
        }
        write!(self.serializer.unescaped(), "]")?;

        // Head: (TableHead Attr [Row])
        {
            let (attrs, rows) = header;
            write!(self.serializer.unescaped(), " (TableHead ")?;
            self.serializer.write_attributes(attrs)?;
            write!(self.serializer.unescaped(), " ")?;
            let mut serializer = SerializeList::new(self.serializer, Row)?;
            rows(&mut serializer)?;
            serializer.finish()?;
            write!(self.serializer.unescaped(), ")")?;
        }

        // Bodies: [TableBody Attr RowHeadColumns [Row] [Row]]
        {
            write!(self.serializer.unescaped(), " ")?;
            let mut serializer = SerializeList::new(self.serializer, TableBody)?;
            bodies(&mut serializer)?;
            serializer.finish()?;
        }

        // Foot
        write!(
            self.serializer.unescaped(),
            r#" (TableFoot ("", [], []) [])"#
        )?;
        Ok(())
    }

    /// Figure, with attributes, caption, and content (list of blocks)
    pub fn serialize_figure(
        self,
        attrs: impl Attributes,
        caption: impl FnOnce(&mut SerializeBlocks<'_, 'book, 'p, W>) -> anyhow::Result<()>,
        content: impl FnOnce(&mut SerializeBlocks<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Figure ")?;
        self.serializer.write_attributes(attrs)?;
        write!(self.serializer.unescaped(), " (Caption Nothing ")?;
        let mut caption_serializer = SerializeList::new(self.serializer, Block)?;
        caption(&mut caption_serializer)?;
        caption_serializer.finish()?;
        write!(self.serializer.unescaped(), ") ")?;
        let mut content_serializer = SerializeList::new(self.serializer, Block)?;
        content(&mut content_serializer)?;
        content_serializer.finish()
    }

    /// Generic block container with attributes
    pub fn serialize_div(
        self,
        attrs: impl Attributes,
        blocks: impl FnOnce(&mut SerializeBlocks<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Div ")?;
        self.serializer.write_attributes(attrs)?;
        write!(self.serializer.unescaped(), " ")?;
        let mut serializer = SerializeList::new(self.serializer, Block)?;
        blocks(&mut serializer)?;
        serializer.finish()
    }
}

impl<W: io::Write> SerializeAttribute<'_, '_, '_, W> {
    pub fn serialize_attribute(&mut self, key: &str, val: &str) -> anyhow::Result<()> {
        self.serializer.unescaped().write_all(b"(")?;
        let writer = self.serializer.escaped();
        writer.start_text()?;
        writer.write_all(key.as_bytes())?;
        writer.end_text()?;
        writer.write_all(b", ")?;
        writer.start_text()?;
        writer.write_all(val.as_bytes())?;
        writer.end_text()?;
        self.serializer.unescaped().write_all(b")")?;
        Ok(())
    }
}

impl<'book, 'p, W: io::Write> SerializeTableBody<'_, 'book, 'p, W> {
    /// TableBody Attr RowHeadColumns [Row] [Row]
    pub fn serialize_body(
        &mut self,
        attrs: impl Attributes,
        rows: impl FnOnce(&mut SerializeRows<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "(TableBody ")?;
        self.serializer.write_attributes(attrs)?;
        write!(self.serializer.unescaped(), " (RowHeadColumns 0) [] ")?;
        let mut serializer = SerializeList::new(self.serializer, Row)?;
        rows(&mut serializer)?;
        serializer.finish()?;
        write!(self.serializer.unescaped(), ")")?;
        Ok(())
    }
}

impl<'book, 'p, W: io::Write> SerializeRow<'_, 'book, 'p, W> {
    /// Row Attr [Cell]
    pub fn serialize_row(
        &mut self,
        attrs: impl Attributes,
        cells: impl FnOnce(&mut SerializeCells<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Row ")?;
        self.serializer.write_attributes(attrs)?;
        write!(self.serializer.unescaped(), " ")?;
        let mut serializer = SerializeList::new(self.serializer, Cell)?;
        cells(&mut serializer)?;
        serializer.finish()
    }
}

impl<'book, 'p, W: io::Write> SerializeCell<'_, 'book, 'p, W> {
    /// Cell Attr Alignment RowSpan ColSpan [Block]
    pub fn serialize_cell(
        self,
        attrs: impl Attributes,
        blocks: impl FnOnce(&mut SerializeBlocks<'_, 'book, 'p, W>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "Cell ")?;
        self.serializer.write_attributes(attrs)?;
        write!(
            self.serializer.unescaped(),
            " {} (RowSpan 0) (ColSpan 0) ",
            Alignment::Default.to_native()
        )?;
        let mut serializer = SerializeList::new(self.serializer, Block)?;
        blocks(&mut serializer)?;
        serializer.finish()
    }
}

impl<'book, 'p, W: io::Write> SerializeDefinitionListItem<'_, 'book, 'p, W> {
    /// A term (a list of inlines) and one or more definitions (each a list of blocks)
    pub fn serialize_item(
        self,
        term: impl FnOnce(&mut SerializeInlines<'_, 'book, 'p, W>) -> anyhow::Result<()>,
        definitions: impl FnOnce(
            &mut SerializeList<'_, 'book, 'p, W, List<Block>>,
        ) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        write!(self.serializer.unescaped(), "(")?;
        let mut serializer = SerializeList::new(self.serializer, Inline)?;
        term(&mut serializer)?;
        serializer.finish()?;
        write!(self.serializer.unescaped(), ", ")?;
        let mut serializer = SerializeList::new(self.serializer, List(Block))?;
        definitions(&mut serializer)?;
        serializer.finish()?;
        write!(self.serializer.unescaped(), ")")?;
        Ok(())
    }
}

impl<W: io::Write> SerializeText<'_, '_, '_, W> {
    pub fn serialize_text(self, s: &str) -> anyhow::Result<()> {
        let writer = self.serializer.escaped();
        writer.start_text()?;
        writer.write_all(s.as_bytes())?;
        writer.end_text()?;
        Ok(())
    }
}

impl<W: io::Write> SerializeCode<'_, '_, '_, W> {
    pub fn serialize_code(&mut self, s: &str) -> anyhow::Result<()> {
        write!(
            self.serializer.unescaped(),
            "{}",
            s.escape_quotes_verbatim()
        )?;
        Ok(())
    }
}

impl From<pulldown_cmark::Alignment> for Alignment {
    fn from(align: pulldown_cmark::Alignment) -> Self {
        match align {
            pulldown_cmark::Alignment::None => Self::Default,
            pulldown_cmark::Alignment::Left => Self::Left,
            pulldown_cmark::Alignment::Center => Self::Center,
            pulldown_cmark::Alignment::Right => Self::Right,
        }
    }
}

impl Alignment {
    fn to_native(&self) -> &str {
        match self {
            Self::Default => "AlignDefault",
            Self::Left => "AlignLeft",
            Self::Center => "AlignCenter",
            Self::Right => "AlignRight",
        }
    }
}
