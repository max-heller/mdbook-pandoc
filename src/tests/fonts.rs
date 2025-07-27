use indoc::indoc;

use super::{Chapter, Config, MDBook};

#[test]
fn font_awesome_icons() {
    let book = MDBook::init()
        .config(Config::pandoc())
        .chapter(Chapter::new(
            "",
            indoc! {r#"
                <i class="fa fa-heart"></i>
                <i class = "fa fa-heart"/></i>

                before <i class="fas fa-print"></i> after

                <i id="example1" class="fas fa-heart extra-class"></i>
                <i class="fa fa-user"></i>
                <i class="fab fa-font-awesome"></i>
                <i class="fas fa-heart">Text prevents translation.</i>

                <i class="fa fa-does-not-exist"></i>

                <i class="fa-solid fa-cat"></i>
            "#},
            "chapter.md",
        ))
        .build();
    insta::assert_snapshot!(book, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/markdown/pandoc-ir
    ├─ markdown/pandoc-ir
    │ [ Para
    │     [ Image
    │         ( "" , [] , [ ( "height" , "1em" ) ] )
    │         []
    │         ( "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCA1MTIgNTEyIj48IS0tISBGb250IEF3ZXNvbWUgRnJlZSA2LjIuMCBieSBAZm9udGF3ZXNvbWUgLSBodHRwczovL2ZvbnRhd2Vzb21lLmNvbSBMaWNlbnNlIC0gaHR0cHM6Ly9mb250YXdlc29tZS5jb20vbGljZW5zZS9mcmVlIChJY29uczogQ0MgQlkgNC4wLCBGb250czogU0lMIE9GTCAxLjEsIENvZGU6IE1JVCBMaWNlbnNlKSBDb3B5cmlnaHQgMjAyMiBGb250aWNvbnMsIEluYy4gLS0+PHBhdGggZD0iTTI0NCA4NEwyNTUuMSA5NkwyNjcuMSA4NC4wMkMzMDAuNiA1MS4zNyAzNDcgMzYuNTEgMzkyLjYgNDQuMUM0NjEuNSA1NS41OCA1MTIgMTE1LjIgNTEyIDE4NS4xVjE5MC45QzUxMiAyMzIuNCA0OTQuOCAyNzIuMSA0NjQuNCAzMDAuNEwyODMuNyA0NjkuMUMyNzYuMiA0NzYuMSAyNjYuMyA0ODAgMjU2IDQ4MEMyNDUuNyA0ODAgMjM1LjggNDc2LjEgMjI4LjMgNDY5LjFMNDcuNTkgMzAwLjRDMTcuMjMgMjcyLjEgMCAyMzIuNCAwIDE5MC45VjE4NS4xQzAgMTE1LjIgNTAuNTIgNTUuNTggMTE5LjQgNDQuMUMxNjQuMSAzNi41MSAyMTEuNCA1MS4zNyAyNDQgODRDMjQzLjEgODQgMjQ0IDg0LjAxIDI0NCA4NEwyNDQgODR6TTI1NS4xIDE2My45TDIxMC4xIDExNy4xQzE4OC40IDk2LjI4IDE1Ny42IDg2LjQgMTI3LjMgOTEuNDRDODEuNTUgOTkuMDcgNDggMTM4LjcgNDggMTg1LjFWMTkwLjlDNDggMjE5LjEgNTkuNzEgMjQ2LjEgODAuMzQgMjY1LjNMMjU2IDQyOS4zTDQzMS43IDI2NS4zQzQ1Mi4zIDI0Ni4xIDQ2NCAyMTkuMSA0NjQgMTkwLjlWMTg1LjFDNDY0IDEzOC43IDQzMC40IDk5LjA3IDM4NC43IDkxLjQ0QzM1NC40IDg2LjQgMzIzLjYgOTYuMjggMzAxLjkgMTE3LjFMMjU1LjEgMTYzLjl6Ii8+PC9zdmc+"
    │         , ""
    │         )
    │     , SoftBreak
    │     , Image
    │         ( "" , [] , [ ( "height" , "1em" ) ] )
    │         []
    │         ( "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCA1MTIgNTEyIj48IS0tISBGb250IEF3ZXNvbWUgRnJlZSA2LjIuMCBieSBAZm9udGF3ZXNvbWUgLSBodHRwczovL2ZvbnRhd2Vzb21lLmNvbSBMaWNlbnNlIC0gaHR0cHM6Ly9mb250YXdlc29tZS5jb20vbGljZW5zZS9mcmVlIChJY29uczogQ0MgQlkgNC4wLCBGb250czogU0lMIE9GTCAxLjEsIENvZGU6IE1JVCBMaWNlbnNlKSBDb3B5cmlnaHQgMjAyMiBGb250aWNvbnMsIEluYy4gLS0+PHBhdGggZD0iTTI0NCA4NEwyNTUuMSA5NkwyNjcuMSA4NC4wMkMzMDAuNiA1MS4zNyAzNDcgMzYuNTEgMzkyLjYgNDQuMUM0NjEuNSA1NS41OCA1MTIgMTE1LjIgNTEyIDE4NS4xVjE5MC45QzUxMiAyMzIuNCA0OTQuOCAyNzIuMSA0NjQuNCAzMDAuNEwyODMuNyA0NjkuMUMyNzYuMiA0NzYuMSAyNjYuMyA0ODAgMjU2IDQ4MEMyNDUuNyA0ODAgMjM1LjggNDc2LjEgMjI4LjMgNDY5LjFMNDcuNTkgMzAwLjRDMTcuMjMgMjcyLjEgMCAyMzIuNCAwIDE5MC45VjE4NS4xQzAgMTE1LjIgNTAuNTIgNTUuNTggMTE5LjQgNDQuMUMxNjQuMSAzNi41MSAyMTEuNCA1MS4zNyAyNDQgODRDMjQzLjEgODQgMjQ0IDg0LjAxIDI0NCA4NEwyNDQgODR6TTI1NS4xIDE2My45TDIxMC4xIDExNy4xQzE4OC40IDk2LjI4IDE1Ny42IDg2LjQgMTI3LjMgOTEuNDRDODEuNTUgOTkuMDcgNDggMTM4LjcgNDggMTg1LjFWMTkwLjlDNDggMjE5LjEgNTkuNzEgMjQ2LjEgODAuMzQgMjY1LjNMMjU2IDQyOS4zTDQzMS43IDI2NS4zQzQ1Mi4zIDI0Ni4xIDQ2NCAyMTkuMSA0NjQgMTkwLjlWMTg1LjFDNDY0IDEzOC43IDQzMC40IDk5LjA3IDM4NC43IDkxLjQ0QzM1NC40IDg2LjQgMzIzLjYgOTYuMjggMzAxLjkgMTE3LjFMMjU1LjEgMTYzLjl6Ii8+PC9zdmc+"
    │         , ""
    │         )
    │     ]
    │ , Para
    │     [ Str "before "
    │     , Image
    │         ( "" , [] , [ ( "height" , "1em" ) ] )
    │         []
    │         ( "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCA1MTIgNTEyIj48IS0tISBGb250IEF3ZXNvbWUgRnJlZSA2LjIuMCBieSBAZm9udGF3ZXNvbWUgLSBodHRwczovL2ZvbnRhd2Vzb21lLmNvbSBMaWNlbnNlIC0gaHR0cHM6Ly9mb250YXdlc29tZS5jb20vbGljZW5zZS9mcmVlIChJY29uczogQ0MgQlkgNC4wLCBGb250czogU0lMIE9GTCAxLjEsIENvZGU6IE1JVCBMaWNlbnNlKSBDb3B5cmlnaHQgMjAyMiBGb250aWNvbnMsIEluYy4gLS0+PHBhdGggZD0iTTEyOCAwQzkyLjcgMCA2NCAyOC43IDY0IDY0djk2aDY0VjY0SDM1NC43TDM4NCA5My4zVjE2MGg2NFY5My4zYzAtMTctNi43LTMzLjMtMTguNy00NS4zTDQwMCAxOC43QzM4OCA2LjcgMzcxLjcgMCAzNTQuNyAwSDEyOHpNMzg0IDM1MnYzMiA2NEgxMjhWMzg0IDM2OCAzNTJIMzg0em02NCAzMmgzMmMxNy43IDAgMzItMTQuMyAzMi0zMlYyNTZjMC0zNS4zLTI4LjctNjQtNjQtNjRINjRjLTM1LjMgMC02NCAyOC43LTY0IDY0djk2YzAgMTcuNyAxNC4zIDMyIDMyIDMySDY0djY0YzAgMzUuMyAyOC43IDY0IDY0IDY0SDM4NGMzNS4zIDAgNjQtMjguNyA2NC02NFYzODR6bS0xNi04OGMtMTMuMyAwLTI0LTEwLjctMjQtMjRzMTAuNy0yNCAyNC0yNHMyNCAxMC43IDI0IDI0cy0xMC43IDI0LTI0IDI0eiIvPjwvc3ZnPg=="
    │         , ""
    │         )
    │     , Str " after"
    │     ]
    │ , Para
    │     [ Image
    │         ( "book__markdown__src__chapter.md__example1"
    │         , [ "extra-class" ]
    │         , [ ( "height" , "1em" ) ]
    │         )
    │         []
    │         ( "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCA1MTIgNTEyIj48IS0tISBGb250IEF3ZXNvbWUgRnJlZSA2LjIuMCBieSBAZm9udGF3ZXNvbWUgLSBodHRwczovL2ZvbnRhd2Vzb21lLmNvbSBMaWNlbnNlIC0gaHR0cHM6Ly9mb250YXdlc29tZS5jb20vbGljZW5zZS9mcmVlIChJY29uczogQ0MgQlkgNC4wLCBGb250czogU0lMIE9GTCAxLjEsIENvZGU6IE1JVCBMaWNlbnNlKSBDb3B5cmlnaHQgMjAyMiBGb250aWNvbnMsIEluYy4gLS0+PHBhdGggZD0iTTQ3LjYgMzAwLjRMMjI4LjMgNDY5LjFjNy41IDcgMTcuNCAxMC45IDI3LjcgMTAuOXMyMC4yLTMuOSAyNy43LTEwLjlMNDY0LjQgMzAwLjRjMzAuNC0yOC4zIDQ3LjYtNjggNDcuNi0xMDkuNXYtNS44YzAtNjkuOS01MC41LTEyOS41LTExOS40LTE0MUMzNDcgMzYuNSAzMDAuNiA1MS40IDI2OCA4NEwyNTYgOTYgMjQ0IDg0Yy0zMi42LTMyLjYtNzktNDcuNS0xMjQuNi0zOS45QzUwLjUgNTUuNiAwIDExNS4yIDAgMTg1LjF2NS44YzAgNDEuNSAxNy4yIDgxLjIgNDcuNiAxMDkuNXoiLz48L3N2Zz4="
    │         , ""
    │         )
    │     , SoftBreak
    │     , Image
    │         ( "" , [] , [ ( "height" , "1em" ) ] )
    │         []
    │         ( "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCA0NDggNTEyIj48IS0tISBGb250IEF3ZXNvbWUgRnJlZSA2LjIuMCBieSBAZm9udGF3ZXNvbWUgLSBodHRwczovL2ZvbnRhd2Vzb21lLmNvbSBMaWNlbnNlIC0gaHR0cHM6Ly9mb250YXdlc29tZS5jb20vbGljZW5zZS9mcmVlIChJY29uczogQ0MgQlkgNC4wLCBGb250czogU0lMIE9GTCAxLjEsIENvZGU6IE1JVCBMaWNlbnNlKSBDb3B5cmlnaHQgMjAyMiBGb250aWNvbnMsIEluYy4gLS0+PHBhdGggZD0iTTI3MiAzMDRoLTk2Qzc4LjggMzA0IDAgMzgyLjggMCA0ODBjMCAxNy42NyAxNC4zMyAzMiAzMiAzMmgzODRjMTcuNjcgMCAzMi0xNC4zMyAzMi0zMkM0NDggMzgyLjggMzY5LjIgMzA0IDI3MiAzMDR6TTQ4Ljk5IDQ2NEM1Ni44OSA0MDAuOSAxMTAuOCAzNTIgMTc2IDM1Mmg5NmM2NS4xNiAwIDExOS4xIDQ4Ljk1IDEyNyAxMTJINDguOTl6TTIyNCAyNTZjNzAuNjkgMCAxMjgtNTcuMzEgMTI4LTEyOGMwLTcwLjY5LTU3LjMxLTEyOC0xMjgtMTI4Uzk2IDU3LjMxIDk2IDEyOEM5NiAxOTguNyAxNTMuMyAyNTYgMjI0IDI1NnpNMjI0IDQ4YzQ0LjExIDAgODAgMzUuODkgODAgODBjMCA0NC4xMS0zNS44OSA4MC04MCA4MFMxNDQgMTcyLjEgMTQ0IDEyOEMxNDQgODMuODkgMTc5LjkgNDggMjI0IDQ4eiIvPjwvc3ZnPg=="
    │         , ""
    │         )
    │     , SoftBreak
    │     , Image
    │         ( "" , [] , [ ( "height" , "1em" ) ] )
    │         []
    │         ( "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCA0NDggNTEyIj48IS0tISBGb250IEF3ZXNvbWUgRnJlZSA2LjIuMCBieSBAZm9udGF3ZXNvbWUgLSBodHRwczovL2ZvbnRhd2Vzb21lLmNvbSBMaWNlbnNlIC0gaHR0cHM6Ly9mb250YXdlc29tZS5jb20vbGljZW5zZS9mcmVlIChJY29uczogQ0MgQlkgNC4wLCBGb250czogU0lMIE9GTCAxLjEsIENvZGU6IE1JVCBMaWNlbnNlKSBDb3B5cmlnaHQgMjAyMiBGb250aWNvbnMsIEluYy4gLS0+PHBhdGggZD0iTTQ0OCA0OFYzODRDMzg1IDQwNyAzNjYgNDE2IDMyOSA0MTZDMjY2IDQxNiAyNDIgMzg0IDE3OSAzODRDMTU5IDM4NCAxNDMgMzg4IDEyOCAzOTJWMzI4QzE0MyAzMjQgMTU5IDMyMCAxNzkgMzIwQzI0MiAzMjAgMjY2IDM1MiAzMjkgMzUyQzM0OSAzNTIgMzY0IDM0OSAzODQgMzQzVjEzNUMzNjQgMTQxIDM0OSAxNDQgMzI5IDE0NEMyNjYgMTQ0IDI0MiAxMTIgMTc5IDExMkMxMjggMTEyIDEwNCAxMzMgNjQgMTQxVjQ0OEM2NCA0NjYgNTAgNDgwIDMyIDQ4MFMwIDQ2NiAwIDQ0OFY2NEMwIDQ2IDE0IDMyIDMyIDMyUzY0IDQ2IDY0IDY0Vjc3QzEwNCA2OSAxMjggNDggMTc5IDQ4QzI0MiA0OCAyNjYgODAgMzI5IDgwQzM2NiA4MCAzODUgNzEgNDQ4IDQ4WiIvPjwvc3ZnPg=="
    │         , ""
    │         )
    │     , SoftBreak
    │     , RawInline (Format "html") "<i class=\"fas fa-heart\">"
    │     , Span ( "" , [] , [] ) [ Str "Text prevents translation." ]
    │     , RawInline (Format "html") "</i>"
    │     ]
    │ , Para
    │     [ RawInline
    │         (Format "html") "<i class=\"fa fa-does-not-exist\">"
    │     , RawInline (Format "html") "</i>"
    │     ]
    │ , Para
    │     [ Image
    │         ( "" , [] , [ ( "height" , "1em" ) ] )
    │         []
    │         ( "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCA1MTIgNTEyIj48IS0tISBGb250IEF3ZXNvbWUgRnJlZSA2LjIuMCBieSBAZm9udGF3ZXNvbWUgLSBodHRwczovL2ZvbnRhd2Vzb21lLmNvbSBMaWNlbnNlIC0gaHR0cHM6Ly9mb250YXdlc29tZS5jb20vbGljZW5zZS9mcmVlIChJY29uczogQ0MgQlkgNC4wLCBGb250czogU0lMIE9GTCAxLjEsIENvZGU6IE1JVCBMaWNlbnNlKSBDb3B5cmlnaHQgMjAyMiBGb250aWNvbnMsIEluYy4gLS0+PHBhdGggZD0iTTI4OCAxOTJoMTcuMWMyMi4xIDM4LjMgNjMuNSA2NCAxMTAuOSA2NGMxMSAwIDIxLjgtMS40IDMyLTR2NCAzMlY0ODBjMCAxNy43LTE0LjMgMzItMzIgMzJzLTMyLTE0LjMtMzItMzJWMzM5LjJMMjQ4IDQ0OGg1NmMxNy43IDAgMzIgMTQuMyAzMiAzMnMtMTQuMyAzMi0zMiAzMkgxNjBjLTUzIDAtOTYtNDMtOTYtOTZWMTkyLjVjMC0xNi4xLTEyLTI5LjgtMjgtMzEuOGwtNy45LTFDMTAuNSAxNTcuNi0xLjkgMTQxLjYgLjIgMTI0czE4LjItMzAgMzUuNy0yNy44bDcuOSAxYzQ4IDYgODQuMSA0Ni44IDg0LjEgOTUuM3Y4NS4zYzM0LjQtNTEuNyA5My4yLTg1LjggMTYwLTg1Ljh6bTE2MCAyNi41djBjLTEwIDMuNS0yMC44IDUuNS0zMiA1LjVjLTI4LjQgMC01NC0xMi40LTcxLjYtMzJoMGMtMy43LTQuMS03LTguNS05LjktMTMuMkMzMjUuMyAxNjQgMzIwIDE0Ni42IDMyMCAxMjh2MFYzMiAxMiAxMC43QzMyMCA0LjggMzI0LjcgLjEgMzMwLjYgMGguMmMzLjMgMCA2LjQgMS42IDguNCA0LjJsMCAuMUwzNTIgMjEuM2wyNy4yIDM2LjNMMzg0IDY0aDY0bDQuOC02LjRMNDgwIDIxLjMgNDkyLjggNC4zbDAtLjFjMi0yLjYgNS4xLTQuMiA4LjQtNC4yaC4yQzUwNy4zIC4xIDUxMiA0LjggNTEyIDEwLjdWMTIgMzJ2OTZjMCAxNy4zLTQuNiAzMy42LTEyLjYgNDcuNmMtMTEuMyAxOS44LTI5LjYgMzUuMi01MS40IDQyLjl6TTQwMCAxMjhjMC04LjgtNy4yLTE2LTE2LTE2cy0xNiA3LjItMTYgMTZzNy4yIDE2IDE2IDE2czE2LTcuMiAxNi0xNnptNDggMTZjOC44IDAgMTYtNy4yIDE2LTE2cy03LjItMTYtMTYtMTZzLTE2IDcuMi0xNiAxNnM3LjIgMTYgMTYgMTZ6Ii8+PC9zdmc+"
    │         , ""
    │         )
    │     ]
    │ ]
    "#);
}

#[test]
#[ignore]
fn right_to_left_fonts_lualatex() {
    let cfg = indoc! {r#"
        [book]
        language = "fa"

        [output.pandoc.profile.pdf]
        output-file = "book.pdf"
        pdf-engine = "lualatex"

        [output.pandoc.profile.pdf.variables]
        mainfont = "Noto Naskh Arabic"
        mainfontfallback = [
          "NotoSerif:",
        ]
    "#};
    let output = MDBook::init()
        .mdbook_config(cfg.parse().unwrap())
        .chapter(Chapter::new("", "<span dir=ltr>C++</span>", "chapter.md"))
        .build();
    insta::assert_snapshot!(output, @r#"
    ├─ log output
    │  INFO mdbook_driver::mdbook: Running the pandoc backend
    │  INFO mdbook_pandoc::pandoc::renderer: Running pandoc
    │  INFO mdbook_pandoc::pandoc::renderer: Wrote output to book/pdf/book.pdf
    ├─ pdf/book.pdf
    │ <INVALID UTF8>
    ├─ pdf/src/chapter.md
    │ [Para [Span ("", [], [("dir", "ltr")]) [Str "C++"]]]
    "#);
}
