use std::{
    borrow::Cow,
    fmt::{self, Write},
    fs, iter,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::Context as _;

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

    pub fn render(self, profile: Profile, ctx: &mut Context) -> anyhow::Result<()> {
        let Profile {
            columns,
            file_scope,
            number_sections,
            output,
            pdf_engine,
            standalone,
            to,
            table_of_contents,
            toc_depth,
            rest,
            mut variables,
        } = profile;

        let mut pandoc = self.pandoc;

        let outfile = {
            fs::create_dir_all(&ctx.destination).with_context(|| {
                format!("Unable to create directory: {}", ctx.destination.display())
            })?;
            ctx.destination.join(output)
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

        pandoc
            .args(["-f", &format])
            .arg("-o")
            .arg(&outfile)
            .args(to.iter().flat_map(|to| ["-t", to]))
            .args(file_scope.then_some("--file-scope"))
            .args(number_sections.then_some("-N"))
            .args(standalone.then_some("-s"))
            .args(table_of_contents.then_some("--toc"));

        if let Some(columns) = columns {
            pandoc.arg("--columns").arg(format!("{columns}"));
        }

        if let Some(engine) = pdf_engine {
            pandoc.arg("--pdf-engine").arg(engine);
        }

        if let Some(depth) = toc_depth {
            pandoc.arg("--toc-depth").arg(format!("{depth}"));
        }

        let mut additional_variables: Vec<(_, Cow<str>)> = vec![];
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
                        iter::repeat([r"\arabic*", r"\Roman*", r"\Alph*", r"\roman*", r"\alph*"])
                            .flatten();
                    for (idx, label) in enumerate_labels.take(ctx.max_list_depth).enumerate() {
                        writeln!(
                            include_before,
                            r"\setlist[enumerate,{}]{{label=({label})}}",
                            idx + 1,
                        )
                        .unwrap();
                    }
                    additional_variables.push(("include-before", include_before.into()))
                }

                let include_packages = packages
                    .needed()
                    .map(|package| format!(r"\usepackage{{{}}}", package.name()))
                    .collect::<Vec<_>>()
                    .join("\n");
                additional_variables.push(("header-includes", include_packages.into()));
            }
            OutputFormat::Other => {}
        };
        for (key, val) in additional_variables {
            pandoc.arg("-V").arg(format!("{key}={val}"));
        }

        let default_variables = match ctx.output {
            OutputFormat::Latex { .. } => [("documentclass", "report")].as_slice(),
            OutputFormat::Other => [].as_slice(),
        };
        for &(key, val) in default_variables {
            if !variables.contains_key(key) {
                variables.insert(key.into(), val.into());
            }
        }

        fn for_each_key_val(key: String, val: toml::Value, mut f: impl FnMut(fmt::Arguments)) {
            let mut f = |val| match val {
                toml::Value::Boolean(true) => f(format_args!("{key}")),
                toml::Value::Boolean(false) => {}
                toml::Value::String(val) => f(format_args!("{key}={val}")),
                val => f(format_args!("{key}={val}")),
            };
            match val {
                toml::Value::Array(vals) => {
                    for val in vals {
                        f(val)
                    }
                }
                val => f(val),
            }
        }

        for (key, val) in variables {
            for_each_key_val(key, val, |arg| {
                pandoc.arg("-V").arg(arg.to_string());
            })
        }

        for (key, val) in rest {
            for_each_key_val(key, val, |arg| {
                pandoc.arg(format!("--{arg}"));
            })
        }

        struct DisplayCommand<'a>(&'a Command);
        impl fmt::Display for DisplayCommand<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0.get_program().to_string_lossy())?;
                for arg in self.0.get_args() {
                    write!(f, " {}", arg.to_string_lossy())?;
                }
                Ok(())
            }
        }
        log::debug!("Running: {}", DisplayCommand(&pandoc));

        let status = pandoc
            .stdin(Stdio::null())
            .status()
            .context("Unable to run `pandoc`")?;
        anyhow::ensure!(status.success(), "pandoc exited unsuccessfully");

        let outfile = outfile.strip_prefix(&ctx.book.root).unwrap_or(&outfile);
        log::info!("Wrote output to {}", outfile.display());

        Ok(())
    }
}
