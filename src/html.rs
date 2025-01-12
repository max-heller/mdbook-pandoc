use crate::preprocess;

pub type Parser<'book> = html5ever::Parser<preprocess::tree::HtmlTreeSink<'book>>;

#[macro_export]
macro_rules! html_name {
    (html $name:tt) => {{
        use html5ever::namespace_url;
        html5ever::QualName {
            prefix: None,
            ns: html5ever::ns!(html),
            local: html5ever::local_name!($name),
        }
    }};
    ($name:tt) => {{
        use html5ever::namespace_url;
        html5ever::QualName {
            prefix: None,
            ns: html5ever::ns!(),
            local: html5ever::local_name!($name),
        }
    }};
}
pub use crate::html_name as name;
