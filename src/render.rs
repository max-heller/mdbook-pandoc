use std::{
    borrow::Cow,
    fmt, fs,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::Context;

use crate::PandocProfile;

pub struct PandocRenderer<'a> {
    pandoc: Command,
    profile: PandocProfile,
    root: &'a Path,
    destination: Cow<'a, Path>,
}

impl<'a> PandocRenderer<'a> {
    pub fn new(profile: PandocProfile, root: &'a Path, destination: Cow<'a, Path>) -> Self {
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

    pub fn render(self) -> anyhow::Result<()> {
        let profile_is_latex = self.profile.is_latex();

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
            fs::create_dir_all(&self.destination).with_context(|| {
                format!("Unable to create directory: {}", self.destination.display())
            })?;
            self.destination.join(output)
        };

        pandoc
            .arg("-f")
            .arg({
                let mut format = String::from("commonmark");
                let extensions = crate::markdown_extensions()
                    .map(|extension| extension.pandoc)
                    .chain([
                        // Automatically generate section labels according to GitHub's method to
                        // align with behavior of mdbook's HTML renderer
                        "gfm_auto_identifiers",
                        // Enable inserting raw LaTeX
                        "raw_attribute",
                        // TODO: pandoc's `rebase_relative_paths` extension works for Markdown links and images,
                        // but not for raw HTML links and images. Switch if/when pandoc supports HTML as well.
                        // Treat paths as relative to the chapter containing them
                        // "rebase_relative_paths",
                    ]);
                for extension in extensions {
                    format.push('+');
                    format.push_str(extension);
                }
                format
            })
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

        let additional_variables = profile_is_latex
            .then_some([
                // FontAwesome icons
                ("header-includes", r"\usepackage{fontawesome}"),
            ])
            .into_iter()
            .flatten();
        for (key, val) in additional_variables {
            pandoc.arg("-V").arg(format!("{key}={val}"));
        }

        let default_variables = profile_is_latex
            .then_some([("documentclass", "report")])
            .into_iter()
            .flatten();
        for (key, val) in default_variables {
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

        log::debug!("Running: {pandoc:#?}");

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
