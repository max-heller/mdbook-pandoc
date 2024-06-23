use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::OutputFormat;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Profile {
    #[serde(default = "defaults::columns")]
    pub columns: usize,
    #[serde(default = "defaults::enabled")]
    pub file_scope: bool,
    #[serde(default = "defaults::enabled")]
    pub number_sections: bool,
    pub output_file: PathBuf,
    pub pdf_engine: Option<PathBuf>,
    #[serde(default = "defaults::enabled")]
    pub standalone: bool,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "defaults::enabled")]
    pub table_of_contents: bool,
    #[serde(default)]
    pub variables: BTreeMap<String, toml::Value>,
    #[serde(flatten)]
    pub rest: BTreeMap<String, toml::Value>,
}

mod defaults {
    pub fn enabled() -> bool {
        true
    }

    pub fn columns() -> usize {
        // https://pandoc.org/MANUAL.html#option--wrap
        72
    }
}

impl Profile {
    pub fn output_format(&self) -> OutputFormat {
        if self.uses_latex() {
            OutputFormat::Latex {
                packages: Default::default(),
            }
        } else {
            OutputFormat::Other
        }
    }

    /// Determines whether the profile uses LaTeX, either by outputting it directory or rendering it to PDF.
    fn uses_latex(&self) -> bool {
        let pdf_engine_is_latex = || {
            // Source: https://pandoc.org/MANUAL.html#option--pdf-engine
            const LATEX_ENGINES: &[&str] =
                &["pdflatex", "lualatex", "xelatex", "latexmk", "tectonic"];
            const NON_LATEX_ENGINES: &[&str] = &[
                "wkhtmltopdf",
                "weasyprint",
                "pagedjs-cli",
                "prince",
                "context",
                "pdfroff",
                "typst",
            ];
            match &self.pdf_engine {
                Some(engine) => {
                    if LATEX_ENGINES
                        .iter()
                        .any(|&latex_engine| engine.as_os_str() == latex_engine)
                    {
                        true
                    } else if NON_LATEX_ENGINES
                        .iter()
                        .any(|&non_latex_engine| engine.as_os_str() == non_latex_engine)
                    {
                        false
                    } else {
                        log::warn!(
                            "Assuming pdf-engine '{}' uses LaTeX; if it doesn't, specify the output format explicitly",
                            engine.display()
                        );
                        true
                    }
                }
                None => true,
            }
        };
        match (self.to.as_deref(), self.output_file.extension()) {
            (Some("latex"), _) => true,
            (Some("pdf"), _) => pdf_engine_is_latex(),
            (Some(_), _) => false,
            (None, None) => false,
            (None, Some(extension)) => {
                extension == "tex" || (extension == "pdf" && pdf_engine_is_latex())
            }
        }
    }
}
