use std::{
    borrow::Cow,
    fmt, fs,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::Context as _;

use crate::{
    capabilities::{Availability, Context, OutputFormat},
    PandocProfile,
};

pub struct PandocRenderer<'a> {
    pandoc: Command,
    profile: PandocProfile,
    root: &'a Path,
    destination: &'a Path,
}

impl<'a> PandocRenderer<'a> {
    pub(crate) fn new(profile: PandocProfile, root: &'a Path, destination: &'a Path) -> Self {
        Self {
            pandoc: Command::new("pandoc"),
            profile,
            root,
            destination,
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

    pub fn render(self, context: &Context) -> anyhow::Result<()> {
        let PandocProfile {
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
        } = self.profile;

        let mut pandoc = self.pandoc;

        let outfile = {
            fs::create_dir_all(self.destination).with_context(|| {
                format!("Unable to create directory: {}", self.destination.display())
            })?;
            self.destination.join(output)
        };

        let format = {
            let mut format = String::from("commonmark");
            for (extension, availability) in context.pandoc.enabled_extensions() {
                match availability {
                    Availability::Available => {
                        format.push('+');
                        format.push_str(extension.name());
                    }
                    Availability::Unavailable(version_req) => {
                        log::warn!(
                            "Cannot use Pandoc extension `{}`, which may result in degraded output (requires version {}, but using {})",
                            extension.name(), version_req, context.pandoc.version,
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
        match &context.output {
            OutputFormat::Latex { packages } => {
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

        let default_variables = match context.output {
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

        let outfile = outfile.strip_prefix(self.root).unwrap_or(&outfile);
        log::info!("Wrote output to {}", outfile.display());

        Ok(())
    }
}
