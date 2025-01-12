use ego_tree::NodeId;
use html5ever::{
    local_name,
    tendril::{format_tendril, StrTendril},
    tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink},
    Attribute, QualName,
};
use std::{
    borrow::Cow,
    cell::{Cell, Ref, RefCell},
};

use super::{
    node::{Element, HtmlElement, Node},
    Tree,
};

#[derive(Debug)]
pub struct HtmlTreeSink<'book> {
    pub tree: RefCell<Tree<'book>>,
    pub most_recently_created_element: Cell<Option<NodeId>>,
}

impl HtmlTreeSink<'_> {
    pub fn new() -> Self {
        Self {
            tree: RefCell::new(Tree::new()),
            most_recently_created_element: Cell::new(None),
        }
    }
}

impl<'book> TreeSink for HtmlTreeSink<'book> {
    type Handle = NodeId;
    type Output = Tree<'book>;
    type ElemName<'a>
        = Ref<'a, QualName>
    where
        Self: 'a;

    fn finish(self) -> Tree<'book> {
        self.tree.into_inner()
    }

    fn parse_error(&self, msg: Cow<'static, str>) {
        self.tree.borrow_mut().errors.push(msg);
    }

    fn get_document(&self) -> Self::Handle {
        self.tree.borrow().tree.root().id()
    }

    fn elem_name<'a>(&'a self, target: &Self::Handle) -> Ref<'a, QualName> {
        Ref::map(self.tree.borrow(), |this| {
            let node = this.tree.get(*target).unwrap().value();
            match node {
                Node::Element(element) => element.name(),
                _ => unreachable!(),
            }
        })
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        _flags: ElementFlags,
    ) -> Self::Handle {
        let mut this = self.tree.borrow_mut();
        let node = this
            .tree
            .orphan(Node::Element(Element::Html(HtmlElement::new(name, attrs))));
        let id = node.id();
        self.most_recently_created_element.set(Some(id));
        id
    }

    fn create_comment(&self, comment: StrTendril) -> Self::Handle {
        let mut this = self.tree.borrow_mut();
        this.tree.orphan(Node::HtmlComment(comment)).id()
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Self::Handle {
        let mut this = self.tree.borrow_mut();
        // https://developer.mozilla.org/en-US/docs/Web/API/ProcessingInstruction
        // says processing instructions are considered comments in HTML
        let comment = format_tendril!("<?{target} {data}?>");
        this.tree.orphan(Node::HtmlComment(comment)).id()
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        let mut this = self.tree.borrow_mut();
        let mut parent = this.tree.get_mut(*parent).unwrap();

        match child {
            NodeOrText::AppendNode(id) => {
                parent.append_id(id);
            }
            NodeOrText::AppendText(text) => {
                if let Some(mut child) = parent.last_child() {
                    if let Node::HtmlText(t) = child.value() {
                        t.push_tendril(&text);
                        return;
                    }
                }
                parent.append(Node::HtmlText(text));
            }
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        let has_parent = {
            let this = self.tree.borrow();
            let element = this.tree.get(*element).unwrap();
            element.parent().is_some()
        };

        if has_parent {
            self.append_before_sibling(element, child)
        } else {
            self.append(prev_element, child)
        }
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        let this = self.tree.borrow();
        let template = this.tree.get(*target).unwrap();
        template.first_child().unwrap().id()
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, sibling: &Self::Handle, new_node: NodeOrText<Self::Handle>) {
        let mut this = self.tree.borrow_mut();
        let mut sibling = this.tree.get_mut(*sibling).unwrap();

        match new_node {
            NodeOrText::AppendNode(id) => {
                sibling.insert_id_before(id);
            }
            NodeOrText::AppendText(text) => {
                if let Some(mut prev) = sibling.prev_sibling() {
                    if let Node::HtmlText(t) = prev.value() {
                        t.push_tendril(&text);
                        return;
                    }
                }
                sibling.insert_before(Node::HtmlText(text));
            }
        }
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attributes: Vec<Attribute>) {
        let mut this = self.tree.borrow_mut();
        let mut node = this.tree.get_mut(*target).unwrap();
        let Node::Element(element) = node.value() else {
            unreachable!()
        };
        match element {
            Element::Markdown(_) => {}
            Element::Html(element) => {
                let attrs = &mut element.attrs;
                for attr in attributes {
                    match attr.name.local {
                        local_name!("id") => {
                            if attrs.id.is_none() {
                                attrs.id = Some(attr.value);
                            }
                        }
                        local_name!("class") => {
                            if attrs.classes.is_empty() {
                                attrs.classes = attr.value;
                            }
                        }
                        _ => {
                            attrs.rest.entry(attr.name).or_insert(attr.value);
                        }
                    }
                }
            }
        }
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        let mut this = self.tree.borrow_mut();
        let mut node = this.tree.get_mut(*target).unwrap();
        node.detach();
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        let mut this = self.tree.borrow_mut();
        let mut new_parent = this.tree.get_mut(*new_parent).unwrap();
        new_parent.reparent_from_id_append(*node);
    }
}
