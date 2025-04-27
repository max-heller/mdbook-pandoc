use crate::preprocess;

pub type Parser<'book> = html5ever::Parser<preprocess::tree::HtmlTreeSink<'book>>;

macro_rules! name {
    (html $name:tt) => {{
        html5ever::QualName {
            prefix: None,
            ns: html5ever::ns!(html),
            local: html5ever::local_name!($name),
        }
    }};
    ($name:tt) => {{
        html5ever::QualName {
            prefix: None,
            ns: html5ever::ns!(),
            local: html5ever::local_name!($name),
        }
    }};
}
pub(crate) use name;
