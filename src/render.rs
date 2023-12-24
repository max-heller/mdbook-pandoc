use std::{
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

    pub fn render(self) -> anyhow::Result<()> {
        let pandoc_version = {
            let output = Command::new("pandoc")
                .arg("-v")
                .output()
                .context("Unable to run `pandoc -v`")?;
            anyhow::ensure!(
                output.status.success(),
                "`pandoc -v` exited with error code {}",
                output.status
            );
            let output =
                String::from_utf8(output.stdout).context("`pandoc -v` output is not UTF8")?;
            match output.lines().next().and_then(|line| line.split_once(' ')) {
                Some(("pandoc", mut version)) => {
                    // Pandoc versions can contain more than three components (e.g. a.b.c.d).
                    // If this is the case, only consider the first three.
                    if let Some((idx, _)) = version.match_indices('.').nth(2) {
                        version = &version[..idx];
                    }
                    semver::Version::parse(version.trim()).unwrap()
                }
                _ => anyhow::bail!("`pandoc -v` output does not contain `pandoc VERSION`"),
            }
        };

        if !crate::PANDOC_VERSION_REQ.matches(&pandoc_version) {
            anyhow::bail!(
                "mdbook-pandoc is incompatible with detected Pandoc version (requires version {}, but using {})",
                *crate::PANDOC_VERSION_REQ, pandoc_version,
            );
        }

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
            fs::create_dir_all(self.destination).with_context(|| {
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
                        ("gfm_auto_identifiers", ">=2.0".parse().unwrap()),
                        // Enable inserting raw LaTeX
                        ("raw_attribute", ">=2.10.1".parse().unwrap()),
                        // TODO: pandoc's `rebase_relative_paths` extension works for Markdown links and images,
                        // but not for raw HTML links and images. Switch if/when pandoc supports HTML as well.
                        // Treat paths as relative to the chapter containing them
                        // ("rebase_relative_paths", ">=2.14".parse().unwrap()),
                    ]);
                for (extension, version_req) in extensions {
                    if version_req.matches(&pandoc_version) {
                        format.push('+');
                        format.push_str(extension);
                    } else {
                        log::warn!(
                            "Cannot use Pandoc extension `{}`, which may result in degraded output (requires version {}, but using {})",
                            extension, version_req, pandoc_version,
                        );
                    }
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
