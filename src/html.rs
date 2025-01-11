use std::cell::Cell;

use html5ever::{interface::TreeSink, namespace_url, ns, LocalName, QualName};

pub type NodeId = <scraper::HtmlTreeSink as TreeSink>::Handle;
pub type Parser = html5ever::Parser<scraper::HtmlTreeSink>;

pub fn name(local: LocalName) -> QualName {
    QualName::new(None, ns!(), local)
}

/// Determines the HTML element to which child events should be appended based on the state of the parser.
pub fn most_recently_created_open_element(parser: &Parser) -> NodeId {
    struct Tracer<'a> {
        html: &'a scraper::Html,
        prev: Cell<Option<NodeId>>,
        next: Cell<Option<NodeId>>,
    }

    impl html5ever::interface::Tracer for Tracer<'_> {
        type Handle = NodeId;

        fn trace_handle(&self, handle: &Self::Handle) {
            if let Some(node) = self.html.tree.get(*handle) {
                if node.value().is_element() {
                    self.prev.swap(&self.next);
                    self.next.set(Some(*handle))
                }
            }
        }
    }

    let sink = &parser.tokenizer.sink;
    let html = sink.sink.0.borrow();
    let tracer = Tracer {
        html: &html,
        prev: Default::default(),
        next: Default::default(),
    };
    sink.trace_handles(&tracer);
    tracer.prev.into_inner().unwrap()
}
