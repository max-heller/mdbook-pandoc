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
    let math = indoc! {"
        $$I(x)=I_0e^{-ax}\\\\another line$$

        inline $a^b$ math
    "};
    let latex = diff(math, Config::latex());
    insta::assert_snapshot!(latex, @r#"
    @@ -3,8 +3,8 @@
     │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
     │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
     ├─ latex/output.tex
    -│ \$\$I(x)=I\_0e\^{}\{-ax\}\textbackslash another line\$\$
    +│ \[I(x)=I_0e^{-ax}\\another line\]
     │ 
    -│ inline \$a\^{}b\$ math
    +│ inline \(a^b\) math
     ├─ latex/src/chapter.md
    -│ [Para [Str "$$I(x)=I_0e^{-ax}", Str "\\another line$$"], Para [Str "inline $a^b$ math"]]
    +│ [Para [Math DisplayMath "I(x)=I_0e^{-ax}\\\\another line"], Para [Str "inline ", Math InlineMath "a^b", Str " math"]]
    "#);
}
