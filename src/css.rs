use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};

use cssparser::CowRcStr;

#[derive(Debug, Default)]
pub struct Css<'i> {
    pub stylesheets: Vec<&'i Path>,
    pub styles: Styles<'i>,
}

#[derive(Debug, Default)]
pub struct Styles<'i> {
    pub classes: HashMap<CowRcStr<'i>, BTreeMap<CowRcStr<'i>, &'i str>>,
}

pub fn read_stylesheets<'a>(
    config: &'a mdbook_core::config::HtmlConfig,
    book: &'a crate::Book,
) -> impl Iterator<Item = (&'a Path, String)> {
    config.additional_css.iter().flat_map(|stylesheet| {
        match fs::read_to_string(book.root.join(stylesheet)) {
            Ok(css) => Some((stylesheet.as_path(), css)),
            Err(err) => {
                log::warn!(
                    "Failed to read CSS stylesheet '{}': {err}",
                    stylesheet.display()
                );
                None
            }
        }
    })
}

impl<'i> Css<'i> {
    pub fn load(&mut self, stylesheet: &'i Path, css: &'i str) {
        self.stylesheets.push(stylesheet);
        let parser = Parser { stylesheet };
        for res in cssparser::StyleSheetParser::new(
            &mut cssparser::Parser::new(&mut cssparser::ParserInput::new(css)),
            &mut &parser,
        ) {
            match res {
                Err((err, css)) => parser.warn_invalid_css(err, css),
                Ok(DeclOrRule::Decl(..)) => {}
                Ok(DeclOrRule::Rule(prelude, decls)) => {
                    for selector in prelude {
                        match selector {
                            Selector::Class(class) => {
                                let props = self.styles.classes.entry(class).or_default();
                                for (prop, val) in &decls {
                                    props.insert(prop.clone(), val);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

struct Parser<'a> {
    stylesheet: &'a Path,
}

impl Parser<'_> {
    fn warn_invalid_css(&self, err: cssparser::ParseError<'_, anyhow::Error>, css: &str) {
        log::warn!(
            "Failed to parse CSS from '{stylesheet}': {err}: {css}",
            stylesheet = self.stylesheet.display()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Selector<'i> {
    Class(CowRcStr<'i>),
}

type Selectors<'i> = Vec<Selector<'i>>;

#[derive(Debug)]
enum DeclOrRule<'i> {
    Decl(CowRcStr<'i>, &'i str),
    Rule(Selectors<'i>, HashMap<CowRcStr<'i>, &'i str>),
}

impl<'i> cssparser::QualifiedRuleParser<'i> for &Parser<'_> {
    type Prelude = Selectors<'i>;
    type QualifiedRule = DeclOrRule<'i>;
    type Error = anyhow::Error;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut cssparser::Parser<'i, 't>,
    ) -> Result<Self::Prelude, cssparser::ParseError<'i, Self::Error>> {
        Ok(
            input.parse_comma_separated_ignoring_errors::<_, _, Self::Error>(|parser| {
                parser.expect_delim('.')?;
                parser
                    .expect_ident_cloned()
                    .map(|class| Selector::Class(class.clone()))
                    .map_err(Into::into)
            }),
        )
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        _start: &cssparser::ParserState,
        input: &mut cssparser::Parser<'i, 't>,
    ) -> Result<Self::QualifiedRule, cssparser::ParseError<'i, Self::Error>> {
        let mut parser: Self = *self;
        let decls = cssparser::RuleBodyParser::new(input, &mut parser)
            .flat_map(|res| match res {
                Ok(DeclOrRule::Decl(prop, val)) => Some((prop, val)),
                Ok(DeclOrRule::Rule(..)) => None,
                Err((err, css)) => {
                    self.warn_invalid_css(err, css);
                    None
                }
            })
            .collect();
        Ok(DeclOrRule::Rule(prelude, decls))
    }
}

impl<'i> cssparser::RuleBodyItemParser<'i, DeclOrRule<'i>, anyhow::Error> for &Parser<'_> {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}

impl<'i> cssparser::DeclarationParser<'i> for &Parser<'_> {
    type Declaration = DeclOrRule<'i>;
    type Error = anyhow::Error;

    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut cssparser::Parser<'i, 't>,
        _declaration_start: &cssparser::ParserState,
    ) -> Result<Self::Declaration, cssparser::ParseError<'i, Self::Error>> {
        input.skip_whitespace();
        let start = input.position();
        loop {
            match input.next() {
                Ok(_) => {}
                Err(err) if err.kind == cssparser::BasicParseErrorKind::EndOfInput => break,
                Err(err) => return Err(err.into()),
            }
        }
        Ok(DeclOrRule::Decl(name, input.slice_from(start)))
    }
}

/// Disregard [at-rules](https://developer.mozilla.org/en-US/docs/Web/CSS/At-rule)
impl<'i> cssparser::AtRuleParser<'i> for &Parser<'_> {
    type Prelude = Selectors<'i>;
    type AtRule = DeclOrRule<'i>;
    type Error = anyhow::Error;

    fn parse_prelude<'t>(
        &mut self,
        _name: CowRcStr<'i>,
        input: &mut cssparser::Parser<'i, 't>,
    ) -> Result<Self::Prelude, cssparser::ParseError<'i, Self::Error>> {
        while !input.is_exhausted() {
            input.next()?;
        }
        Ok(Vec::new())
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        _start: &cssparser::ParserState,
        input: &mut cssparser::Parser<'i, 't>,
    ) -> Result<Self::AtRule, cssparser::ParseError<'i, Self::Error>> {
        while !input.is_exhausted() {
            input.next()?;
        }
        Ok(DeclOrRule::Rule(prelude, HashMap::new()))
    }
}
