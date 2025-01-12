use std::{
    fmt::Write as _,
    fs,
    io::Write as _,
    mem,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::Context as _;
use mdbook::config::TextDirection;
use normpath::PathExt;
use tempfile::NamedTempFile;

use crate::{book::Book, css, latex, pandoc::Profile, CodeConfig};

pub struct Renderer {
    pandoc: Command,
    num_inputs: usize,
}

pub struct Context<'book> {
    pub output: OutputFormat,
    pub destination: PathBuf,
    pub book: &'book Book<'book>,
    pub mdbook_cfg: &'book mdbook::Config,
    pub columns: usize,
    pub cur_list_depth: usize,
    pub max_list_depth: usize,
    pub html: Option<&'book mdbook::config::HtmlConfig>,
    pub(crate) code: &'book CodeConfig,
    pub css: &'book css::Css<'book>,
}

#[derive(Debug)]
pub enum OutputFormat {
    Latex { packages: latex::Packages },
    HtmlLike,
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

    pub fn render(self, mut profile: Profile, ctx: &mut Context) -> anyhow::Result<()> {
        let mut pandoc = self.pandoc;

        profile.output_file = {
            fs::create_dir_all(&ctx.destination).with_context(|| {
                format!("Unable to create directory: {}", ctx.destination.display())
            })?;
            ctx.destination.join(&profile.output_file)
        };

        pandoc.args(["-f", "native"]);

        let mut default_metadata = vec![];
        if let Some(title) = ctx.mdbook_cfg.book.title.as_deref() {
            default_metadata.push(("title", title.into()));
        }
        if let Some(description) = ctx.mdbook_cfg.book.description.as_deref() {
            default_metadata.push(("description", description.into()));
        }
        if !ctx.mdbook_cfg.book.authors.is_empty() {
            default_metadata.push(("author", ctx.mdbook_cfg.book.authors.clone().into()));
        }
        for (key, val) in default_metadata {
            if !profile.metadata.contains_key(key) {
                profile.metadata.insert(key.into(), val);
            }
        }

        let mut default_variables = vec![];
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
        match ctx.output {
            OutputFormat::Latex { .. } => {
                default_variables.push(("documentclass", "report".into()));
            }
            OutputFormat::HtmlLike | OutputFormat::Other => {}
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

                if ctx.mdbook_cfg.book.realized_text_direction() == TextDirection::RightToLeft {
                    // Without this, LuaTeX errors on left-to-right text because the \LR command isn't defined, e.g.:
                    //   Error producing PDF.
                    //   ! Undefined control sequence.
                    //   l.279 ...اوقات زبان Rust را با C و \LR
                    // (see https://github.com/google/comprehensive-rust/pull/2531#issuecomment-2567445055)
                    // Using luabidi was suggested in
                    // https://github.com/jgm/pandoc/issues/8460#issuecomment-1344881107
                    additional_variables.push((
                        "header-includes",
                        r"\ifLuaTeX\usepackage{luabidi}\fi".into(),
                    ));
                }

                let include_packages = packages
                    .needed()
                    .map(|package| format!(r"\usepackage{{{}}}", package.name()))
                    .collect::<Vec<_>>()
                    .join("\n");
                additional_variables.push(("header-includes", include_packages));
            }
            OutputFormat::HtmlLike => {
                for stylesheet in &ctx.css.stylesheets {
                    additional_variables.push(("css", stylesheet.to_string_lossy().into_owned()));
                }
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

        let defaults_file = {
            let mut file = NamedTempFile::new()?;
            serde_yaml::to_writer(&mut file, &profile)?;
            file
        };
        pandoc.arg("-d").arg(defaults_file.path());

        // --file-scope only works if there are at least two files, so if there is only one file,
        // add an additionaly empty file to convince Pandoc to perform its link adjustment pass
        let _dummy_tempfile_guard: tempfile::TempPath;
        if self.num_inputs < 2 {
            let mut dummy = tempfile::Builder::new()
                .prefix("dummy")
                .rand_bytes(0)
                .tempfile_in(&ctx.destination)?;
            write!(dummy, "[]")?;
            let path = dummy
                .path()
                .normalize()
                .context("failed to normalize dummy file path")?;
            pandoc.arg(path.as_path().strip_prefix(&ctx.book.root).unwrap());
            _dummy_tempfile_guard = dummy.into_temp_path();
        }

        if log::log_enabled!(log::Level::Trace) {
            log::trace!("Running pandoc with profile: {profile:#?}");
        } else {
            log::info!("Running pandoc");
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
