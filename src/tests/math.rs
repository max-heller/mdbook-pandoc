use std::str::FromStr;

use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn math() {
    let diff = |source: &str, mut config: Config| {
        let chapter = Chapter::new("", source, "chapter.md");
        let without = MDBook::init()
            .chapter(chapter.clone())
            .config(config.clone())
            .build();
        let with = MDBook::init()
            .chapter(chapter)
            .config({
                config.common.markdown.extensions.math = true;
                config
            })
            .build();
        similar::TextDiff::from_lines(&without.to_string(), &with.to_string())
            .unified_diff()
            .to_string()
    };
    let math = indoc! {r"
        $$I(x)=I_0e^{-ax}\\another line$$

        $$
        \begin{cases}
            \frac 1 2 \\
            \frac 3 4
            5
        \end{cases}
        $$

        inline $a^b$ math
    "};
    let latex = diff(math, Config::latex());
    insta::assert_snapshot!(latex, @r#"
    @@ -3,12 +3,22 @@
     │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
     │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
     ├─ latex/output.tex
    -│ \$\$I(x)=I\_0e\^{}\{-ax\}\textbackslash another line\$\$
    +│ \[I(x)=I_0e^{-ax}\\another line\]
     │ 
    -│ \$\$ \textbackslash begin\{cases\}
    -│ \textbackslash frac 1 2 \textbackslash{} \textbackslash frac 3 4 5
    -│ \textbackslash end\{cases\} \$\$
    +│ \[
    +│ \begin{cases}
    +│     \frac 1 2 \\
    +│     \frac 3 4
    +│     5
    +│ \end{cases}
    +│ \]
     │ 
    -│ inline \$a\^{}b\$ math
    +│ inline \(a^b\) math
     ├─ latex/src/chapter.md
    -│ [Para [Str "$$I(x)=I_0e^{-ax}", Str "\\another line$$"], Para [Str "$$", SoftBreak, Str "\\begin{cases}", SoftBreak, Str "\\frac 1 2 ", Str "\\", SoftBreak, Str "\\frac 3 4", SoftBreak, Str "5", SoftBreak, Str "\\end{cases}", SoftBreak, Str "$$"], Para [Str "inline $a^b$ math"]]
    +│ [Para [Math DisplayMath "I(x)=I_0e^{-ax}\\\\another line"], Para [Math DisplayMath "
    +│ \\begin{cases}
    +│     \\frac 1 2 \\\\
    +│     \\frac 3 4
    +│     5
    +│ \\end{cases}
    +│ "], Para [Str "inline ", Math InlineMath "a^b", Str " math"]]
    "#);
}

#[test]
fn mathjax_compatibility() {
    let math = indoc! {r"
        before \\( \int x dx = \frac{x^2}{2} + C \\) middle \\( 2 + 2 = 4 \\) after

        \\( \begin{cases} \frac 1 2 \\\\ \frac 3 4 \end{cases} \\)
        \\[ \begin{cases} \frac 1 2 \\\\ \frac 3 4 \end{cases} \\]

        \\[ \mu = \frac{1}{N} \sum_{i=0} x_i \\]
        $$ \mu = \frac{1}{N} \sum_{i=0} x_i $$

        \\[
        \begin{cases}
            \frac 1 2 \\\\
            \frac 3 4
            5
        \end{cases}
        \\]

        $$
        \begin{cases}
            \frac 1 2 \\\\
            \frac 3 4
            5
        \end{cases}
        $$
    "};
    let cfg = indoc! {r#"
        [output.html]
        mathjax-support = true
    "#};
    let output = MDBook::init()
        .chapter(Chapter::new("", math, "chapter.md"))
        .mdbook_config(mdbook::config::Config::from_str(cfg).unwrap())
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ before \( \int x dx = \frac{x^2}{2} + C \) middle \( 2 + 2 = 4 \) after
    │ 
    │ \( \begin{cases} \frac 1 2 \\ \frac 3 4 \end{cases} \)
    │ \[ \begin{cases} \frac 1 2 \\ \frac 3 4 \end{cases} \]
    │ 
    │ \[ \mu = \frac{1}{N} \sum_{i=0} x_i \]
    │ \[ \mu = \frac{1}{N} \sum_{i=0} x_i \]
    │ 
    │ \[
    │ \begin{cases}
    │ \frac 1 2 \\
    │ \frac 3 4
    │ 5
    │ \end{cases}
    │ \]
    │ 
    │ \[
    │ \begin{cases}
    │ \frac 1 2 \\
    │ \frac 3 4
    │ 5
    │ \end{cases}
    │ \]
    ├─ latex/src/chapter.md
    │ [Para [Str "before ", Math InlineMath " \\int x dx = \\frac{x^2}{2} + C ", Str " middle ", Math InlineMath " 2 + 2 = 4 ", Str " after"], Para [Math InlineMath " \\begin{cases} \\frac 1 2 \\\\ \\frac 3 4 \\end{cases} ", Str "
    │ ", Math DisplayMath " \\begin{cases} \\frac 1 2 \\\\ \\frac 3 4 \\end{cases} "], Para [Math DisplayMath " \\mu = \\frac{1}{N} \\sum_{i=0} x_i ", Str "
    │ ", Math DisplayMath " \\mu = \\frac{1}{N} \\sum_{i=0} x_i "], Para [Math DisplayMath "
    │ \\begin{cases}
    │ \\frac 1 2 \\\\
    │ \\frac 3 4
    │ 5
    │ \\end{cases}
    │ "], Para [Math DisplayMath "
    │ \\begin{cases}
    │ \\frac 1 2 \\\\
    │ \\frac 3 4
    │ 5
    │ \\end{cases}
    │ "]]
    "#);
}

#[test]
fn tex_newcommand() {
    let chapter = indoc! {r"
        \\(
        \newcommand{\R}{\mathbb{R}}
        \renewcommand{\R}{\mathbb{R}}
        \newcommand{\plusbinomial}[3][2]{(#2 + #3)^#1}
        \newcommand \BAR {\mathrm{bar}}
        \\)

        \\( \R \plusbinomial{a}{b}{c} \\)
        \\( \R \BAR \\)
        \\( \R \\)
    "};
    let cfg = indoc! {r#"
        [output.html]
        mathjax-support = true
    "#};

    let output = MDBook::init()
        .chapter(Chapter::new("", chapter, "chapter.md"))
        .mdbook_config(mdbook::config::Config::from_str(cfg).unwrap())
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \providecommand{\R}{}\renewcommand{\R}{\mathbb{R}}
    │ \renewcommand{\R}{\mathbb{R}}
    │ \providecommand{\plusbinomial}{}\renewcommand{\plusbinomial}[3][2]{(#2 + #3)^#1}
    │ \providecommand\BAR{}\renewcommand\BAR{\mathrm{bar}}
    │ 
    │ \( \R \plusbinomial{a}{b}{c} \)
    │ \( \R \BAR \)
    │ \( \R \)
    ├─ latex/src/chapter.md
    │ [Para [RawInline (Format "latex") "\\providecommand{\\R}{}\\renewcommand{\\R}{\\mathbb{R}}
    │ \\renewcommand{\\R}{\\mathbb{R}}
    │ \\providecommand{\\plusbinomial}{}\\renewcommand{\\plusbinomial}[3][2]{(#2 + #3)^#1}
    │ \\providecommand\\BAR{}\\renewcommand\\BAR{\\mathrm{bar}}"], Para [Math InlineMath " \\R \\plusbinomial{a}{b}{c} ", Str "
    │ ", Math InlineMath " \\R \\BAR ", Str "
    │ ", Math InlineMath " \\R "]]
    "#);
}

#[test]
fn tex_def() {
    let chapter = indoc! {r"
        \\(
        \def\RR{{\bf R}}
        \\)

        \\( \RR \\)
    "};
    let cfg = indoc! {r#"
        [output.html]
        mathjax-support = true
    "#};

    let output = MDBook::init()
        .chapter(Chapter::new("", chapter, "chapter.md"))
        .mdbook_config(mdbook::config::Config::from_str(cfg).unwrap())
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \def\RR{{\bf R}}
    │ 
    │ \( \RR \)
    ├─ latex/src/chapter.md
    │ [Para [RawInline (Format "latex") "\\def\\RR{{\\bf R}}"], Para [Math InlineMath " \\RR "]]
    "#);
}

#[test]
fn tex_let() {
    // \let\bar = 50 seems to be correctly parsed as \let\bar = 5 followed by 0
    let chapter = indoc! {r"
        \\(
        \def\foo{5}
        \let\originalfoo\foo
        \let\bar = 50
        \\)

        \\( \foo \bar \\)
    "};
    let cfg = indoc! {r#"
        [output.html]
        mathjax-support = true
    "#};

    let output = MDBook::init()
        .chapter(Chapter::new("", chapter, "chapter.md"))
        .mdbook_config(mdbook::config::Config::from_str(cfg).unwrap())
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \def\foo{5}
    │ \let\originalfoo\foo
    │ \let\bar = 5 \(0\)
    │ 
    │ \( \foo \bar \)
    ├─ latex/src/chapter.md
    │ [Para [RawInline (Format "latex") "\\def\\foo{5}
    │ \\let\\originalfoo\\foo
    │ \\let\\bar = 5", SoftBreak, Math InlineMath "0"], Para [Math InlineMath " \\foo \\bar "]]
    "#);
}

#[test]
#[ignore = "slow"]
fn tex_macros_pdf() {
    let chapter = indoc! {r"
        \\(
        \newcommand{\R}{\mathbb{R}}
        \renewcommand{\R}{\mathbb{R2}}
        \newcommand{\plusbinomial}[3][2]{(#2 + #3)^#1}
        \newcommand \BAR {\mathrm{bar}}
        \def\foo{5}
        \let\originalfoo\foo
        \let\bar = 6
        \\)

        \\(
        \R
        \BAR
        \foo
        \bar
        \originalfoo
        \\)
        \\( \plusbinomial{a}{b}{c} \\)
        \\( \R \BAR \\)
    "};
    let cfg = indoc! {r#"
        [output.html]
        mathjax-support = true
    "#};

    let output = MDBook::init()
        .chapter(Chapter::new("", chapter, "chapter.md"))
        .mdbook_config(mdbook::config::Config::from_str(cfg).unwrap())
        .config(Config::pdf_and_latex())
        .build();
    insta::assert_snapshot!(output, @r"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/pdf/book.pdf
    ├─ latex/output.tex
    │ \providecommand{\R}{}\renewcommand{\R}{\mathbb{R}}
    │ \renewcommand{\R}{\mathbb{R2}}
    │ \providecommand{\plusbinomial}{}\renewcommand{\plusbinomial}[3][2]{(#2 + #3)^#1}
    │ \providecommand\BAR{}\renewcommand\BAR{\mathrm{bar}}
    │ \def\foo{5}
    │ \let\originalfoo\foo
    │ \let\bar = 6
    │ 
    │ \(
    │ \R
    │ \BAR
    │ \foo
    │ \bar
    │ \originalfoo
    │ \)
    │ \( \plusbinomial{a}{b}{c} \)
    │ \( \R \BAR \)
    ├─ pdf/book.pdf
    │ <INVALID UTF8>
    ");
}
