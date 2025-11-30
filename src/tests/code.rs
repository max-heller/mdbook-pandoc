use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn code_escaping() {
    let book = MDBook::init()
        .config(Config::pandoc())
        .chapter(Chapter::new(
            "",
            indoc! {r###"
                ```rust
                "foo"; r"foo";                     // foo
                "\"foo\""; r#""foo""#;             // "foo"

                "foo #\"# bar";
                r##"foo #"# bar"##;                // foo #"# bar

                "\x52"; "R"; r"R";                 // R
                "\\x52"; r"\x52";                  // \x52
                ```
                `"foo"; r"foo";                     // foo`
                `"\"foo\""; r#""foo""#;             // "foo"`
                `"foo #\"# bar";`
                `r##"foo #"# bar"##;                // foo #"# bar`
                `"\x52"; "R"; r"R";                 // R`
                `"\\x52"; r"\x52";                  // \x52`
            "###},
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r###"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir
    ├─ markdown/pandoc-ir
    │ [ CodeBlock
    │     ( "" , [ "rust" ] , [] )
    │     "\"foo\"; r\"foo\";                     // foo\n\"\\\"foo\\\"\"; r#\"\"foo\"\"#;             // \"foo\"\n\n\"foo #\\\"# bar\";\nr##\"foo #\"# bar\"##;                // foo #\"# bar\n\n\"\\x52\"; \"R\"; r\"R\";                 // R\n\"\\\\x52\"; r\"\\x52\";                  // \\x52\n"
    │ , Para
    │     [ Code
    │         ( "" , [] , [] )
    │         "\"foo\"; r\"foo\";                     // foo"
    │     , SoftBreak
    │     , Code
    │         ( "" , [] , [] )
    │         "\"\\\"foo\\\"\"; r#\"\"foo\"\"#;             // \"foo\""
    │     , SoftBreak
    │     , Code ( "" , [] , [] ) "\"foo #\\\"# bar\";"
    │     , SoftBreak
    │     , Code
    │         ( "" , [] , [] )
    │         "r##\"foo #\"# bar\"##;                // foo #\"# bar"
    │     , SoftBreak
    │     , Code
    │         ( "" , [] , [] )
    │         "\"\\x52\"; \"R\"; r\"R\";                 // R"
    │     , SoftBreak
    │     , Code
    │         ( "" , [] , [] )
    │         "\"\\\\x52\"; r\"\\x52\";                  // \\x52"
    │     ]
    │ ]
    "###);
}

#[test]
fn code_block_with_hidden_lines() {
    let content = indoc! {r#"
        ```rust
        # fn main() {
            # // another hidden line
        println!("Hello, world!");
            #foo
            # foo
            ##foo
            ## foo
            # # foo
            #[test]
            #![test]
            #
        # }
        ```
    "#};
    let book = MDBook::init()
        .config(Config::markdown())
        .chapter(Chapter::new("", content, "chapter.md"))
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md
    ├─ markdown/book.md
    │ ``` rust
    │ println!("Hello, world!");
    │     #foo
    │     #foo
    │     # foo
    │     #[test]
    │     #![test]
    │ ```
    "#);
    let book = MDBook::init()
        .config({
            let mut config = Config::markdown();
            config.common.code.show_hidden_lines = true;
            config
        })
        .chapter(Chapter::new("", content, "chapter.md"))
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md
    ├─ markdown/book.md
    │ ``` rust
    │ fn main() {
    │     // another hidden line
    │ println!("Hello, world!");
    │     #foo
    │     foo
    │     #foo
    │     # foo
    │     # foo
    │     #[test]
    │     #![test]
    │     
    │ }
    │ ```
    "#);
}

#[test]
fn non_rust_code_block_with_hidden_lines() {
    let content = indoc! {r#"
        ```python
        ~hidden()
        nothidden():
        ~    hidden()
            ~hidden()
            nothidden()
        ```
    "#};
    let cfg = indoc! {r#"
        [output.html.code.hidelines]
        python = "~"
    "#};
    let book = MDBook::init()
        .mdbook_config(cfg.parse().unwrap())
        .config(Config::markdown())
        .chapter(Chapter::new("", content, "chapter.md"))
        .build();
    insta::assert_snapshot!(book, @r"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md
    ├─ markdown/book.md
    │ ``` python
    │ nothidden():
    │     nothidden()
    │ ```
    ");
    let book = MDBook::init()
        .mdbook_config(cfg.parse().unwrap())
        .config({
            let mut config = Config::markdown();
            config.common.code.show_hidden_lines = true;
            config
        })
        .chapter(Chapter::new("", content, "chapter.md"))
        .build();
    insta::assert_snapshot!(book, @r"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md
    ├─ markdown/book.md
    │ ``` python
    │ hidden()
    │ nothidden():
    │     hidden()
    │     hidden()
    │     nothidden()
    │ ```
    ");
}

#[test]
fn code_block_hidelines_override() {
    let content = indoc! {r#"
        ```python,hidelines=!!!
        !!!hidden()
        nothidden():
        !!!    hidden()
            !!!hidden()
            nothidden()
        ```
    "#};
    let book = MDBook::init()
        .config(Config::markdown())
        .chapter(Chapter::new("", content, "chapter.md"))
        .build();
    insta::assert_snapshot!(book, @r"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/book.md
    ├─ markdown/book.md
    │ ``` python
    │ nothidden():
    │     nothidden()
    │ ```
    ");
}

#[test]
#[ignore]
fn code_block_with_very_long_line() {
    let long_line = str::repeat("long ", 1000);
    let content = indoc::formatdoc! {"
        ```java
        {long_line}
        ```
    "};
    let book = MDBook::init()
        .config(Config::pdf())
        .chapter(Chapter::new("", content, "chapter.md"))
        .build();
    insta::assert_snapshot!(book, @r"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/pdf/book.pdf
    ├─ pdf/book.pdf
    │ <INVALID UTF8>
    ");
}

#[test]
#[ignore]
fn code_block_with_very_long_line_with_special_characters() {
    let content = indoc! {r#"""
        ```console
        $ rustc json_error_demo.rs --error-format json
        {"message":"cannot add `&str` to `{integer}`","code":{"code":"E0277","explanation":"\nYou tried to use a type which doesn't implement some trait in a place which\nexpected that trait. Erroneous code example:\n\n```compile_fail,E0277\n// here we declare the Foo trait with a bar method\ntrait Foo {\n    fn bar(&self);\n}\n\n// we now declare a function which takes an object implementing the Foo trait\nfn some_func<T: Foo>(foo: T) {\n    foo.bar();\n}\n\nfn main() {\n    // we now call the method with the i32 type, which doesn't implement\n    // the Foo trait\n    some_func(5i32); // error: the trait bound `i32 : Foo` is not satisfied\n}\n```\n\nIn order to fix this error, verify that the type you're using does implement\nthe trait. Example:\n\n```\ntrait Foo {\n    fn bar(&self);\n}\n\nfn some_func<T: Foo>(foo: T) {\n    foo.bar(); // we can now use this method since i32 implements the\n               // Foo trait\n}\n\n// we implement the trait on the i32 type\nimpl Foo for i32 {\n    fn bar(&self) {}\n}\n\nfn main() {\n    some_func(5i32); // ok!\n}\n```\n\nOr in a generic context, an erroneous code example would look like:\n\n```compile_fail,E0277\nfn some_func<T>(foo: T) {\n    println!(\"{:?}\", foo); // error: the trait `core::fmt::Debug` is not\n                           //        implemented for the type `T`\n}\n\nfn main() {\n    // We now call the method with the i32 type,\n    // which *does* implement the Debug trait.\n    some_func(5i32);\n}\n```\n\nNote that the error here is in the definition of the generic function: Although\nwe only call it with a parameter that does implement `Debug`, the compiler\nstill rejects the function: It must work with all possible input types. In\norder to make this example compile, we need to restrict the generic type we're\naccepting:\n\n```\nuse std::fmt;\n\n// Restrict the input type to types that implement Debug.\nfn some_func<T: fmt::Debug>(foo: T) {\n    println!(\"{:?}\", foo);\n}\n\nfn main() {\n    // Calling the method is still fine, as i32 implements Debug.\n    some_func(5i32);\n\n    // This would fail to compile now:\n    // struct WithoutDebug;\n    // some_func(WithoutDebug);\n}\n```\n\nRust only looks at the signature of the called function, as such it must\nalready specify all requirements that will be used for every type parameter.\n"},"level":"error","spans":[{"file_name":"json_error_demo.rs","byte_start":50,"byte_end":51,"line_start":4,"line_end":4,"column_start":7,"column_end":8,"is_primary":true,"text":[{"text":"    a + b","highlight_start":7,"highlight_end":8}],"label":"no implementation for `{integer} + &str`","suggested_replacement":null,"suggestion_applicability":null,"expansion":null}],"children":[{"message":"the trait `std::ops::Add<&str>` is not implemented for `{integer}`","code":null,"level":"help","spans":[],"children":[],"rendered":null}],"rendered":"error[E0277]: cannot add `&str` to `{integer}`\n --> json_error_demo.rs:4:7\n  |\n4 |     a + b\n  |       ^ no implementation for `{integer} + &str`\n  |\n  = help: the trait `std::ops::Add<&str>` is not implemented for `{integer}`\n\n"}
        ```
    """#};
    let book = MDBook::init()
        .config(Config::pdf())
        .chapter(Chapter::new("", content, "chapter.md"))
        .build();
    insta::assert_snapshot!(book, @r"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/pdf/book.pdf
    ├─ pdf/book.pdf
    │ <INVALID UTF8>
    ");
}

#[test]
fn mdbook_rust_code_block_attributes() {
    let code = indoc! {r#"
        ```rust
        fn main() {}
        ```
        ```rust,ignore
        fn main() {}
        ```
        ```rust,compile_fail
        fn main() {}
        ```
    "#};

    let book = MDBook::init()
        .config(Config::latex())
        .chapter(Chapter::new("", code, "chapter.md"))
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/latex/output.tex
    ├─ latex/output.tex
    │ \begin{Shaded}
    │ \begin{Highlighting}[]
    │ \KeywordTok{fn}\NormalTok{ main() }\OperatorTok{\{\}}
    │ \end{Highlighting}
    │ \end{Shaded}
    │ 
    │ \begin{Shaded}
    │ \begin{Highlighting}[]
    │ \KeywordTok{fn}\NormalTok{ main() }\OperatorTok{\{\}}
    │ \end{Highlighting}
    │ \end{Shaded}
    │ 
    │ \begin{Shaded}
    │ \begin{Highlighting}[]
    │ \KeywordTok{fn}\NormalTok{ main() }\OperatorTok{\{\}}
    │ \end{Highlighting}
    │ \end{Shaded}
    ├─ latex/src/chapter.md
    │ [CodeBlock ("", ["rust"], []) "fn main() {}
    │ ", CodeBlock ("", ["rust", "ignore"], []) "fn main() {}
    │ ", CodeBlock ("", ["rust", "compile_fail"], []) "fn main() {}
    │ "]
    "#);

    let book = MDBook::init()
        .config(Config::html())
        .chapter(Chapter::new("", code, "chapter.md"))
        .build();
    insta::assert_snapshot!(book, @r##"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/html/book.html
    ├─ html/book.html
    │ <div class="sourceCode" id="cb1"><pre
    │ class="sourceCode rust"><code class="sourceCode rust"><span id="cb1-1"><a href="#cb1-1" aria-hidden="true" tabindex="-1"></a><span class="kw">fn</span> main() <span class="op">{}</span></span></code></pre></div>
    │ <div class="sourceCode" id="cb2"><pre
    │ class="sourceCode rust ignore"><code class="sourceCode rust"><span id="cb2-1"><a href="#cb2-1" aria-hidden="true" tabindex="-1"></a><span class="kw">fn</span> main() <span class="op">{}</span></span></code></pre></div>
    │ <div class="sourceCode" id="cb3"><pre
    │ class="sourceCode rust compile_fail"><code class="sourceCode rust"><span id="cb3-1"><a href="#cb3-1" aria-hidden="true" tabindex="-1"></a><span class="kw">fn</span> main() <span class="op">{}</span></span></code></pre></div>
    "##);
}

#[test]
fn regression_inline_code_newline() {
    let book = MDBook::init()
        .config(Config::pandoc())
        .chapter(Chapter::new(
            "",
            // Important for inline code to be in the same inline container as the rest of the item
            "- Writing a program that prints `Hello, world!`",
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir
    ├─ markdown/pandoc-ir
    │ [ BulletList
    │     [ [ Plain
    │           [ Str "Writing a program that prints "
    │           , Code ( "" , [] , [] ) "Hello, world!"
    │           ]
    │       ]
    │     ]
    │ ]
    "#);
}
