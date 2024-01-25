use std::{
    fmt::Write as _,
    fs,
    io::Write as _,
    mem,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::Context as _;
use tempfile::NamedTempFile;

use crate::{
    book::Book,
    latex,
    pandoc::{self, extension, Profile},
};

pub struct Renderer {
    pandoc: Command,
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
}

pub enum OutputFormat {
    Latex { packages: latex::Packages },
    Other,
}

impl Renderer {
    pub(crate) fn new() -> Self {
        Self {
            pandoc: Command::new("pandoc"),
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
        self
    }

    pub fn render(self, mut profile: Profile, ctx: &mut Context) -> anyhow::Result<()> {
        let mut pandoc = self.pandoc;

        profile.output_file = {
            fs::create_dir_all(&ctx.destination).with_context(|| {
                format!("Unable to create directory: {}", ctx.destination.display())
            })?;
            ctx.destination.join(&profile.output_file)
        };

        let format = {
            let mut format = String::from("commonmark");
            for (extension, availability) in ctx.pandoc.enabled_extensions() {
                match availability {
                    extension::Availability::Available => {
                        format.push('+');
                        format.push_str(extension.name());
                    }
                    extension::Availability::Unavailable(version_req) => {
                        log::warn!(
                            "Cannot use Pandoc extension `{}`, which may result in degraded output (requires version {}, but using {})",
                            extension.name(), version_req, ctx.pandoc.version,
                        );
                    }
                }
            }
            format
        };
        pandoc.args(["-f", &format]);

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
            let version_req = semver::VersionReq::parse(">=2.9.2").unwrap();
            if version_req.matches(&ctx.pandoc.version) {
                let mut filter = NamedTempFile::new()?;
                write!(
                    filter,
                    "{}",
                    include_str!("filters/annotate-tables-with-column-widths.lua")
                )?;
                pandoc.arg("--lua-filter");
                pandoc.arg(filter.path().canonicalize()?);
                _filter_tempfile_guard = filter.into_temp_path();
            } else {
                log::warn!(
                    "Cannot wrap cell contents of tables, which may result in tables overflowing the page (requires Pandoc version {}, but using {})",
                    version_req, ctx.pandoc.version,
                );
            }
        }

        let defaults_file = {
            let mut file = NamedTempFile::new()?;
            serde_yaml::to_writer(&mut file, &profile)?;
            file
        };
        pandoc.arg("-d").arg(defaults_file.path());

        log::debug!("Running pandoc");
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
