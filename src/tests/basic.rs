use super::{Chapter, Config, MDBook};

#[test]
fn basic() {
    let book = MDBook::init()
        .chapter(Chapter::new(
            "Getting Started",
            "# Getting Started",
            "getting-started.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md    
    ├─ markdown/book.md
    │ # Getting Started {#book__markdown__src__getting-started.md__getting-started}
    ");
}

#[test]
fn strikethrough() {
    let book = MDBook::init()
        .chapter(Chapter::new("", "~test1~ ~~test2~~", "chapter.md"))
        .config(Config::latex())
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook::book: Running the pandoc backend    
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc    
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex    
    ├─ latex/output.tex
    │ \st{test1} \st{test2}
    ├─ latex/src/chapter.md
    │ [Para [Strikeout [Str "test1"], Str " ", Strikeout [Str "test2"]]]
    "#);
}

#[test]
fn task_lists() {
    let book = MDBook::init()
        .chapter(Chapter::new(
            "",
            "- [x] Complete task\n- [ ] Incomplete task",
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
    │ \begin{itemize}
    │ \tightlist
    │ \item[$\boxtimes$]
    │   Complete task
    │ \item[$\square$]
    │   Incomplete task
    │ \end{itemize}
    ├─ latex/src/chapter.md
    │ [BulletList [[Plain [Str "\9746", Space, Str "Complete task"]], [Plain [Str "\9744", Space, Str "Incomplete task"]]]]
    "#);
}
