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
                config.markdown.extensions.math = true;
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
    "};
    let cfg = indoc! {r#"
        [output.html]
        mathjax-support = true
    "#};
    let output = MDBook::init()
        .chapter(Chapter::new("", math, "chapter.md"))
        .mdbook_config(mdbook::Config::from_str(cfg).unwrap())
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
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
    ├─ latex/src/chapter.md
    │ [Para [Str "before ", Math InlineMath " \\int x dx = \\frac{x^2}{2} + C ", Str " middle ", Math InlineMath " 2 + 2 = 4 ", Str " after"], Para [Math InlineMath " \\begin{cases} \\frac 1 2 \\\\ \\frac 3 4 \\end{cases} ", Str "
    │ ", Math DisplayMath " \\begin{cases} \\frac 1 2 \\\\ \\frac 3 4 \\end{cases} "], Para [Math DisplayMath " \\mu = \\frac{1}{N} \\sum_{i=0} x_i ", Str "
    │ ", Math DisplayMath " \\mu = \\frac{1}{N} \\sum_{i=0} x_i "], Para [Math DisplayMath "
    │ \\begin{cases}
    │ \\frac 1 2 \\\\
    │ \\frac 3 4
    │ 5
    │ \\end{cases}
    │ "]]
    "#);
}
