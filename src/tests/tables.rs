use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn empty() {
    let book = MDBook::init()
        .chapter(Chapter::new(
            "",
            indoc! {"
                | Header1 | Header2 |
                |---------|---------|
            "},
            "chapter.md",
        ))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \begin{longtable}[]{@{}ll@{}}
    │ \toprule\noalign{}
    │ Header1 & Header2 \\
    │ \midrule\noalign{}
    │ \endhead
    │ \bottomrule\noalign{}
    │ \endlastfoot
    │ \end{longtable}
    ├─ latex/src/chapter.md
    │ [Table ("", [], []) (Caption Nothing []) [(AlignDefault, ColWidthDefault), (AlignDefault, ColWidthDefault)] (TableHead ("", [], []) [Row ("", [], []) [Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "Header1"]], Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "Header2"]]]]) [] (TableFoot ("", [], []) [])]
    "#);
}

#[test]
fn basic() {
    let book = MDBook::init()
        .chapter(Chapter::new(
            "",
            indoc! {"
                | Header1 | Header2 |
                |---------|---------|
                | abc     | def     |
            "},
            "chapter.md",
        ))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \begin{longtable}[]{@{}ll@{}}
    │ \toprule\noalign{}
    │ Header1 & Header2 \\
    │ \midrule\noalign{}
    │ \endhead
    │ \bottomrule\noalign{}
    │ \endlastfoot
    │ abc & def \\
    │ \end{longtable}
    ├─ latex/src/chapter.md
    │ [Table ("", [], []) (Caption Nothing []) [(AlignDefault, ColWidthDefault), (AlignDefault, ColWidthDefault)] (TableHead ("", [], []) [Row ("", [], []) [Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "Header1"]], Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "Header2"]]]]) [(TableBody ("", [], []) (RowHeadColumns 0) [] [Row ("", [], []) [Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "abc"]], Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "def"]]]])] (TableFoot ("", [], []) [])]
    "#);
}

#[test]
fn wide() {
    let book = MDBook::init()
        .chapter(Chapter::new(
            "",
            indoc! {"
                | Header1 | Header2 |
                | ------- | :--------------------------------------------------------------- |
                | abc     | long long long long long long long long long long long long long |
            "},
            "chapter.md",
        ))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \begin{longtable}[]{@{}
    │   >{\raggedright\arraybackslash}p{(\linewidth - 2\tabcolsep) * \real{0.0986}}
    │   >{\raggedright\arraybackslash}p{(\linewidth - 2\tabcolsep) * \real{0.9014}}@{}}
    │ \toprule\noalign{}
    │ \begin{minipage}[b]{\linewidth}\raggedright
    │ Header1
    │ \end{minipage} & \begin{minipage}[b]{\linewidth}\raggedright
    │ Header2
    │ \end{minipage} \\
    │ \midrule\noalign{}
    │ \endhead
    │ \bottomrule\noalign{}
    │ \endlastfoot
    │ abc &
    │ long long long long long long long long long long long long long \\
    │ \end{longtable}
    ├─ latex/src/chapter.md
    │ [Table ("", [], []) (Caption Nothing []) [(AlignDefault, (ColWidth 0.09859154929577464)), (AlignLeft, (ColWidth 0.9014084507042254))] (TableHead ("", [], []) [Row ("", [], []) [Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "Header1"]], Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "Header2"]]]]) [(TableBody ("", [], []) (RowHeadColumns 0) [] [Row ("", [], []) [Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "abc"]], Cell ("", [], []) AlignDefault (RowSpan 0) (ColSpan 0) [Plain [Str "long long long long long long long long long long long long long"]]]])] (TableFoot ("", [], []) [])]
    "#);
}
