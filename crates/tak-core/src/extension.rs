//! Format-neutral representation of vendor extensions.
//!
//! TAK clients attach arbitrary `<detail>` children to CoT events (and the
//! same data travels as `xmlDetail` inside TAK protobuf). TAK-RS maps the
//! extensions it understands onto typed domain fields; everything else is kept
//! verbatim as a [`DetailNode`] tree so it survives storage and re-emission.
//!
//! A `DetailNode` is *not* an XML node: it is a generic ordered tree of
//! `(name, attributes, text, children)` that both the XML and protobuf
//! adapters can produce and consume.

use std::fmt;

/// One element of an extension tree.
#[derive(Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DetailNode {
    /// Element name, e.g. `__group`, `takv`, `com.example.meshtastic`.
    pub name: String,
    /// Ordered attribute list. Duplicates are not permitted by producers.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub attributes: Vec<(String, String)>,
    /// Concatenated character data directly inside this element.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub text: Option<String>,
    /// Child elements in document order.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub children: Vec<DetailNode>,
}

impl DetailNode {
    /// Create an empty element.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Builder: add an attribute.
    pub fn with_attr(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.set_attr(key, value);
        self
    }

    /// Builder: set text content.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Builder: append a child.
    pub fn with_child(mut self, child: DetailNode) -> Self {
        self.children.push(child);
        self
    }

    /// Look up an attribute by name.
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Insert or replace an attribute.
    pub fn set_attr(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if let Some(slot) = self.attributes.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            self.attributes.push((key, value));
        }
    }

    /// First child with the given name.
    pub fn child(&self, name: &str) -> Option<&DetailNode> {
        self.children.iter().find(|c| c.name == name)
    }

    /// All children with the given name.
    pub fn children_named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a DetailNode> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// Total number of nodes in this subtree including `self`.
    pub fn node_count(&self) -> usize {
        1 + self
            .children
            .iter()
            .map(DetailNode::node_count)
            .sum::<usize>()
    }

    /// Depth of the subtree rooted at `self` (a leaf has depth 1).
    pub fn depth(&self) -> usize {
        1 + self
            .children
            .iter()
            .map(DetailNode::depth)
            .max()
            .unwrap_or(0)
    }
}

impl fmt::Debug for DetailNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("DetailNode");
        s.field("name", &self.name);
        if !self.attributes.is_empty() {
            s.field("attributes", &self.attributes);
        }
        if let Some(t) = &self.text {
            s.field("text", t);
        }
        if !self.children.is_empty() {
            s.field("children", &self.children);
        }
        s.finish()
    }
}

/// Ordered collection of top-level extension elements attached to an object.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Extensions(Vec<DetailNode>);

impl Extensions {
    /// No extensions.
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Wrap a list of nodes.
    pub fn from_nodes(nodes: Vec<DetailNode>) -> Self {
        Self(nodes)
    }

    /// Whether there are no extensions.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of top-level extension elements.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// First extension with the given element name.
    pub fn get(&self, name: &str) -> Option<&DetailNode> {
        self.0.iter().find(|n| n.name == name)
    }

    /// Mutable access to the first extension with the given element name.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut DetailNode> {
        self.0.iter_mut().find(|n| n.name == name)
    }

    /// Whether an extension with this name is present.
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Append an extension element.
    pub fn push(&mut self, node: DetailNode) {
        self.0.push(node);
    }

    /// Replace all extensions with this name by `node` (or append if absent).
    pub fn replace(&mut self, node: DetailNode) {
        self.0.retain(|n| n.name != node.name);
        self.0.push(node);
    }

    /// Remove and return all extensions with this name.
    pub fn remove(&mut self, name: &str) -> Vec<DetailNode> {
        let (removed, kept) = std::mem::take(&mut self.0)
            .into_iter()
            .partition(|n| n.name == name);
        self.0 = kept;
        removed
    }

    /// Iterate over the top-level extension elements.
    pub fn iter(&self) -> impl Iterator<Item = &DetailNode> {
        self.0.iter()
    }

    /// Consume, returning the nodes.
    pub fn into_nodes(self) -> Vec<DetailNode> {
        self.0
    }
}

impl IntoIterator for Extensions {
    type Item = DetailNode;
    type IntoIter = std::vec::IntoIter<DetailNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Extensions {
    type Item = &'a DetailNode;
    type IntoIter = std::slice::Iter<'a, DetailNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl FromIterator<DetailNode> for Extensions {
    fn from_iter<I: IntoIterator<Item = DetailNode>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_and_children() {
        let node = DetailNode::new("takv")
            .with_attr("device", "Pixel")
            .with_attr("os", "34")
            .with_child(DetailNode::new("x").with_text("y"));
        assert_eq!(node.attr("device"), Some("Pixel"));
        assert_eq!(node.attr("missing"), None);
        assert_eq!(node.child("x").and_then(|c| c.text.as_deref()), Some("y"));
        assert_eq!(node.node_count(), 2);
        assert_eq!(node.depth(), 2);

        let mut node = node;
        node.set_attr("os", "35");
        assert_eq!(node.attr("os"), Some("35"));
        assert_eq!(node.attributes.len(), 2);
    }

    #[test]
    fn extensions_replace_and_remove() {
        let mut ext = Extensions::new();
        ext.push(DetailNode::new("a").with_attr("v", "1"));
        ext.push(DetailNode::new("b"));
        ext.push(DetailNode::new("a").with_attr("v", "2"));
        assert_eq!(ext.len(), 3);
        ext.replace(DetailNode::new("a").with_attr("v", "3"));
        assert_eq!(ext.len(), 2);
        assert_eq!(ext.get("a").and_then(|n| n.attr("v")), Some("3"));
        assert_eq!(ext.remove("a").len(), 1);
        assert!(!ext.contains("a"));
        assert!(ext.contains("b"));
    }
}
