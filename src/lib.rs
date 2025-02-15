use std::{
    collections::HashMap,
    fs::{self, File},
};

use anyhow::{anyhow, Context as _};
use mdbook::config::HtmlConfig;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

mod book;
use book::Book;

mod css;
mod html;
mod latex;
mod pandoc;

mod preprocess;
use preprocess::Preprocessor;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Config {
    #[serde(rename = "profile", default = "Default::default")]
    pub profiles: HashMap<String, pandoc::Profile>,
    #[serde(default = "defaults::enabled")]
    pub keep_preprocessed: bool,
    pub hosted_html: Option<String>,
    /// Code block related configuration.
    #[serde(default = "Default::default")]
    pub code: CodeConfig,
    /// Skip running the renderer.
    #[serde(default = "Default::default")]
    pub disabled: bool,
    /// Markdown-related configuration.
    #[serde(default = "Default::default")]
    pub markdown: MarkdownConfig,
}

/// Configuration for customizing how Markdown is parsed.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct MarkdownConfig {
    /// Enable additional Markdown extensions.
    pub extensions: MarkdownExtensionConfig,
}

/// [`pulldown_cmark`] Markdown extensions not enabled by default by [`mdbook`].
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct MarkdownExtensionConfig {
    /// Enable [`pulldown_cmark::Options::ENABLE_GFM`].
    #[serde(default = "defaults::disabled")]
    pub gfm: bool,
    /// Enable [`pulldown_cmark::Options::ENABLE_MATH`].
    #[serde(default = "defaults::disabled")]
    pub math: bool,
    /// Enable [`pulldown_cmark::Options::ENABLE_DEFINITION_LIST`].
    #[serde(default = "defaults::disabled")]
    pub definition_lists: bool,
    /// Enable [`pulldown_cmark::Options::ENABLE_SUPERSCRIPT`].
    #[serde(default = "defaults::disabled")]
    pub superscript: bool,
    /// Enable [`pulldown_cmark::Options::ENABLE_SUBSCRIPT`].
    #[serde(default = "defaults::disabled")]
    pub subscript: bool,
}

/// Configuration for tweaking how code blocks are rendered.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CodeConfig {
    pub show_hidden_lines: bool,
}

mod defaults {
    pub fn enabled() -> bool {
        true
    }

    pub fn disabled() -> bool {
        false
    }
}

/// A [`mdbook`] backend supporting many output formats by relying on [`pandoc`](https://pandoc.org).
#[derive(Default)]
pub struct Renderer {
    logfile: Option<File>,
}

impl Renderer {
    pub fn new() -> Self {
        Self { logfile: None }
    }

    const NAME: &'static str = "pandoc";
    const CONFIG_KEY: &'static str = "output.pandoc";
}

impl mdbook::Renderer for Renderer {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn render(&self, ctx: &mdbook::renderer::RenderContext) -> anyhow::Result<()> {
        // If we're compiled against mdbook version I.J.K, require ^I.J
        // This allows using a version of mdbook with an earlier patch version as a server
        static MDBOOK_VERSION_REQ: Lazy<semver::VersionReq> = Lazy::new(|| {
            let compiled_mdbook_version = semver::Version::parse(mdbook::MDBOOK_VERSION).unwrap();
            semver::VersionReq {
                comparators: vec![semver::Comparator {
                    op: semver::Op::Caret,
                    major: compiled_mdbook_version.major,
                    minor: Some(compiled_mdbook_version.minor),
                    patch: None,
                    pre: Default::default(),
                }],
            }
        });
        let mdbook_server_version = semver::Version::parse(&ctx.version).unwrap();
        if !MDBOOK_VERSION_REQ.matches(&mdbook_server_version) {
            log::warn!(
                "{} is semver-incompatible with mdbook {} (requires {})",
                env!("CARGO_PKG_NAME"),
                mdbook_server_version,
                *MDBOOK_VERSION_REQ,
            );
        }

        let cfg: Config = ctx
            .config
            .get_deserialized_opt(Self::CONFIG_KEY)
            .with_context(|| format!("Unable to deserialize {}", Self::CONFIG_KEY))?
            .ok_or(anyhow!("No {} table found", Self::CONFIG_KEY))?;

        if cfg.disabled {
            log::info!("Skipping rendering since `disabled` is set");
            return Ok(());
        }

        pandoc::check_compatibility()?;

        let html_cfg: Option<HtmlConfig> = ctx
            .config
            .get_deserialized_opt("output.html")
            .unwrap_or_default();

        let book = Book::new(ctx)?;

        let stylesheets;
        let mut css = css::Css::default();
        if let Some(cfg) = &html_cfg {
            stylesheets = css::read_stylesheets(cfg, &book).collect::<Vec<_>>();
            for (stylesheet, stylesheet_css) in &stylesheets {
                css.load(stylesheet, stylesheet_css);
            }
        }

        for (name, profile) in cfg.profiles {
            let ctx = pandoc::RenderContext {
                book: &book,
                mdbook_cfg: &ctx.config,
                destination: book.destination.join(name),
                output: profile.output_format(),
                columns: profile.columns,
                cur_list_depth: 0,
                max_list_depth: 0,
                code: &cfg.code,
                html: html_cfg.as_ref(),
                css: &css,
            };

            // Preprocess book
            let mut preprocessor = Preprocessor::new(ctx, &cfg.markdown)?;

            if let Some(uri) = cfg.hosted_html.as_deref() {
                preprocessor.hosted_html(uri);
            }

            if let Some(redirects) = html_cfg.as_ref().map(|cfg| &cfg.redirect) {
                if !redirects.is_empty() {
                    log::debug!("Processing redirects in [output.html.redirect]");
                    let redirects = redirects
                        .iter()
                        .map(|(src, dst)| (src.as_str(), dst.as_str()));
                    // In tests, sort redirect map to ensure stable log output
                    #[cfg(test)]
                    let redirects = redirects
                        .collect::<std::collections::BTreeMap<_, _>>()
                        .into_iter();
                    preprocessor.add_redirects(redirects);
                }
            }

            let mut preprocessed = preprocessor.preprocess();

            // Initialize renderer
            let mut renderer = pandoc::Renderer::new();

            // Add preprocessed book chapters to renderer
            renderer.current_dir(&book.root);
            for input in &mut preprocessed {
                renderer.input(input?);
            }

            if preprocessed.unresolved_links() {
                log::warn!(
                    "Unable to resolve one or more relative links within the book, \
                    consider setting the `hosted-html` option in `[output.pandoc]`"
                );
            }

            if let Some(logfile) = &self.logfile {
                renderer.stderr(logfile.try_clone()?);
            }

            // Render final output
            renderer.render(profile, preprocessed.render_context())?;

            if !cfg.keep_preprocessed {
                fs::remove_dir_all(preprocessed.output_dir())?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
