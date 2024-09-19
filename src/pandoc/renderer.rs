use std::{
    collections::HashSet,
    fmt::Write as _,
    fs,
    io::Write as _,
    mem,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::Context as _;
use normpath::PathExt;
use tempfile::NamedTempFile;

use crate::{
    book::Book,
    latex,
    pandoc::{self, extension, Profile, Version},
    CodeConfig,
};

pub struct Renderer {
    pandoc: Command,
    num_inputs: usize,
}

pub struct Context<'book> {
    pub output: OutputFormat,
    pub pandoc: pandoc::Context,
    pub destination: PathBuf,
    pub book: &'book Book<'book>,
    pub mdbook_cfg: &'book mdbook::Config,
    pub columns: usize,
    pub cur_list_depth: usize,
    pub max_list_depth: usize,
    pub html: Option<&'book mdbook::config::HtmlConfig>,
    pub(crate) code: &'book CodeConfig,
}

pub enum OutputFormat {
    Latex { packages: latex::Packages },
    Other,
}

impl Renderer {
    pub(crate) fn new() -> Self {
        Self {
            pandoc: Command::new("pandoc"),
            num_inputs: 0,
        }
    }

    pub fn stderr(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.pandoc.stderr(cfg);
        self
    }

    pub fn current_dir(&mut self, working_dir: impl AsRef<Path>) -> &mut Self {
        self.pandoc.current_dir(working_dir);
        self
    }

    pub fn input(&mut self, input: impl AsRef<Path>) -> &mut Self {
        self.pandoc.arg(input.as_ref());
        self.num_inputs += 1;
        self
    }

    /// Parses a Pandoc format string possibly containing extension modifiers into the format name
    /// and an iterator of extensions.
    ///
    /// For example, splits "commonmark+foo-bar" into "commonmark" and ["foo", "bar"].
    fn parse_format(format: &str) -> (&str, impl Iterator<Item = &str>) {
        const ENABLED: char = '+';
        const DISABLED: char = '-';
        let mut parts = format.split([ENABLED, DISABLED]);
        let format = parts
            .next()
            .expect("str::split() always returns at least one item");
        let extensions = parts;
        (format, extensions)
    }

    pub fn render(self, mut profile: Profile, ctx: &mut Context) -> anyhow::Result<()> {
        let mut pandoc = self.pandoc;

        profile.output_file = {
            fs::create_dir_all(&ctx.destination).with_context(|| {
                format!("Unable to create directory: {}", ctx.destination.display())
            })?;
            ctx.destination.join(&profile.output_file)
        };

        profile.from = Some({
            let mut format;
            let mut explicitly_configured_extensions = HashSet::new();
            // Check if the profile has specified an explicit source format.
            // If so, respect its extension configuration
            match profile.from {
                None => format = String::from("commonmark"),
                Some(from) => {
                    format = from;
                    let (_, extensions) = Self::parse_format(&format);
                    explicitly_configured_extensions.extend(extensions);
                }
            };
            // Don't redundantly enable extensions or enable an explicitly disabled extension
            ctx.pandoc.retain_extensions(|extension| {
                !explicitly_configured_extensions.contains(extension.name())
            });
            // Enable additional extensions
            for (extension, availability) in ctx.pandoc.enabled_extensions() {
                match availability {
                    extension::Availability::Available => {
                        format.push('+');
                        format.push_str(extension.name());
                    }
                    extension::Availability::Unavailable { introduced_in } => {
                        log::warn!(
                            "Cannot use Pandoc extension `{}`, which may result in degraded output \
                            (introduced in version {}, but using {})",
                            extension.name(), introduced_in, ctx.pandoc.version,
                        );
                    }
                }
            }
            format
        });

        let mut default_variables = vec![];
        match ctx.output {
            OutputFormat::Latex { .. } => {
                default_variables.push(("documentclass", "report".into()));
                if let Some(title) = ctx.mdbook_cfg.book.title.as_deref() {
                    default_variables.push(("title", title.into()));
                }
                if let Some(description) = ctx.mdbook_cfg.book.description.as_deref() {
                    default_variables.push(("description", description.into()));
                }
                default_variables.push(("author", ctx.mdbook_cfg.book.authors.clone().into()));
                if let Some(language) = ctx.mdbook_cfg.book.language.as_deref() {
                    default_variables.push(("lang", language.into()));
                }
                if let Some(text_direction) = ctx.mdbook_cfg.book.text_direction {
                    let dir = match text_direction {
                        mdbook::config::TextDirection::LeftToRight => "ltr",
                        mdbook::config::TextDirection::RightToLeft => "rtl",
                    };
                    default_variables.push(("dir", dir.into()));
                }
            }
            OutputFormat::Other => {}
        };
        for (key, val) in default_variables {
            if !profile.variables.contains_key(key) {
                profile.variables.insert(key.into(), val);
            }
        }

        // Additional items to include in array-valued variables
        let mut additional_variables = vec![];
        match &mut ctx.output {
            OutputFormat::Latex { packages } => {
                // Enable line breaking in code blocks
                additional_variables.push((
                    "header-includes",
                    r"
\IfFileExists{fvextra.sty}{% use fvextra if available to break long lines in code blocks
  \usepackage{fvextra}
  \fvset{breaklines}
}{}
"
                    .into(),
                ));

                // https://www.overleaf.com/learn/latex/Lists#Lists_for_lawyers:_nesting_lists_to_an_arbitrary_depth
                const LATEX_DEFAULT_LIST_DEPTH_LIMIT: usize = 4;

                // If necessary, extend the max list depth
                if ctx.max_list_depth > LATEX_DEFAULT_LIST_DEPTH_LIMIT {
                    packages.need(latex::Package::EnumItem);

                    let mut include_before = format!(
                        // Source: https://tex.stackexchange.com/a/41409 and https://tex.stackexchange.com/a/304515
                        r"
\setlistdepth{{{depth}}}
\renewlist{{itemize}}{{itemize}}{{{depth}}}

% initially, use dots for all levels
\setlist[itemize]{{label=$\cdot$}}

% customize the first 3 levels
\setlist[itemize,1]{{label=\textbullet}}
\setlist[itemize,2]{{label=--}}
\setlist[itemize,3]{{label=*}}

\renewlist{{enumerate}}{{enumerate}}{{{depth}}}
",
                        depth = ctx.max_list_depth,
                    );

                    let enumerate_labels =
                        [r"\arabic*", r"\alph*", r"\roman*", r"\Alph*", r"\Roman*"]
                            .into_iter()
                            .cycle();
                    for (idx, label) in enumerate_labels.take(ctx.max_list_depth).enumerate() {
                        writeln!(
                            include_before,
                            r"\setlist[enumerate,{}]{{label=({label})}}",
                            idx + 1,
                        )
                        .unwrap();
                    }
                    additional_variables.push(("include-before", include_before))
                }

                let include_packages = packages
                    .needed()
                    .map(|package| format!(r"\usepackage{{{}}}", package.name()))
                    .collect::<Vec<_>>()
                    .join("\n");
                additional_variables.push(("header-includes", include_packages));
            }
            OutputFormat::Other => {}
        };
        // Prepend additional variables to existing variables
        for (key, val) in additional_variables.into_iter().rev() {
            match profile.variables.get_mut(key) {
                None => {
                    profile.variables.insert(key.into(), val.into());
                }
                Some(toml::Value::Array(arr)) => arr.insert(0, val.into()),
                Some(existing) => {
                    *existing = {
                        let existing = mem::replace(existing, toml::Value::Array(vec![]));
                        toml::Value::Array(vec![val.into(), existing])
                    };
                }
            }
        }

        let _filter_tempfile_guard: tempfile::TempPath;
        if (ctx.pandoc.enabled_extensions).contains_key(&pandoc::Extension::PipeTables) {
            let introduced_in = Version {
                major: 2,
                minor: 9,
                patch: 2,
            };
            if ctx.pandoc.version >= introduced_in {
                let mut filter = NamedTempFile::new()?;
                write!(
                    filter,
                    "{}",
                    include_str!("filters/annotate-tables-with-column-widths.lua")
                )?;
                pandoc.arg("--lua-filter");
                pandoc.arg(filter.path().normalize()?.as_path());
                _filter_tempfile_guard = filter.into_temp_path();
            } else {
                log::warn!(
                    "Cannot wrap cell contents of tables, which may result in tables overflowing the page (requires Pandoc version {}, but using {})",
                    introduced_in, ctx.pandoc.version,
                );
            }
        }

        let defaults_file = {
            let mut file = NamedTempFile::new()?;
            serde_yaml::to_writer(&mut file, &profile)?;
            file
        };
        pandoc.arg("-d").arg(defaults_file.path());

        // --file-scope only works if there are at least two files, so if there is only one file,
        // add an additionaly empty file to convince Pandoc to perform its link adjustment pass
        let _dummy_tempfile_guard: tempfile::TempPath;
        if self.num_inputs == 1 {
            let dummy = tempfile::Builder::new()
                .prefix("dummy")
                .rand_bytes(0)
                .tempfile_in(&ctx.destination)?;
            let path = dummy
                .path()
                .normalize()
                .context("failed to normalize dummy file path")?;
            pandoc.arg(path.as_path().strip_prefix(&ctx.book.root).unwrap());
            _dummy_tempfile_guard = dummy.into_temp_path();
        }

        if log::log_enabled!(log::Level::Trace) {
            log::trace!("Running pandoc with profile: {profile:#?} - {pandoc:#?}");
        } else {
            log::debug!("Running pandoc");
        }
        let status = pandoc
            .stdin(Stdio::null())
            .status()
            .context("Unable to run `pandoc`")?;
        anyhow::ensure!(status.success(), "pandoc exited unsuccessfully");

        let outfile = &profile.output_file;
        let outfile = outfile.strip_prefix(&ctx.book.root).unwrap_or(outfile);
        log::info!("Wrote output to {}", outfile.display());

        Ok(())
    }
}
