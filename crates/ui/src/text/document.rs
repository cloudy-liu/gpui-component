use gpui::{
    App, InteractiveElement as _, IntoElement, ListState, ParentElement as _, SharedString,
    Styled as _, Window, div,
};

use crate::ElementExt as _;
use crate::text::node::{BlockNode, NodeContext};

/// The parsed document AST.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ParsedDocument {
    pub(crate) source: SharedString,
    pub(crate) blocks: Vec<BlockNode>,
}

#[derive(Default, Clone, Copy)]
pub(crate) struct NodeRenderOptions {
    pub(crate) ix: usize,
    pub(crate) in_list: bool,
    pub(crate) todo: bool,
    pub(crate) ordered: bool,
    pub(crate) depth: usize,
    pub(crate) is_last: bool,
}

impl NodeRenderOptions {
    pub(crate) fn is_last(mut self, is_last: bool) -> Self {
        self.is_last = is_last;
        self
    }
}

impl ParsedDocument {
    /// GitHub-style stable heading slugs, including duplicate-title suffixes.
    /// Called only when content changes, never for a style update.
    pub(super) fn assign_anchors(&mut self) {
        fn visit(nodes: &mut [BlockNode], used: &mut std::collections::HashSet<String>) {
            for node in nodes {
                match node {
                    BlockNode::Heading {
                        children, anchor, ..
                    } => {
                        let base: String = children
                            .text()
                            .trim()
                            .to_lowercase()
                            .chars()
                            .filter_map(|c| {
                                if c.is_whitespace() {
                                    Some('-')
                                } else if c.is_alphanumeric() || c == '-' || c == '_' {
                                    Some(c)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        let mut id = base.clone();
                        let mut suffix = 1;
                        while !used.insert(id.clone()) {
                            id = format!("{base}-{suffix}");
                            suffix += 1;
                        }
                        *anchor = id.into();
                    }
                    BlockNode::Root { children, .. }
                    | BlockNode::Blockquote { children, .. }
                    | BlockNode::List { children, .. }
                    | BlockNode::ListItem { children, .. } => visit(children, used),
                    _ => {}
                }
            }
        }
        visit(&mut self.blocks, &mut Default::default());
    }

    pub(super) fn text(&self) -> String {
        let mut text = String::new();
        for block in self.blocks.iter() {
            text.push_str(&block.text());
        }
        text
    }

    pub(super) fn selected_text(&self) -> String {
        let mut text = String::new();
        for block in self.blocks.iter() {
            text.push_str(&block.selected_text());
        }
        text
    }

    /// Synchronously clear the selection stored in every inline state.
    ///
    /// This mirrors the [`selected_text`](Self::selected_text) traversal so the
    /// stored selection can be cleared without relying on a repaint. Offscreen
    /// (virtualized) views do not repaint, so their `InlineState.selection`
    /// would otherwise retain stale values from the last painted frame.
    pub(super) fn clear_selection(&self) {
        for block in self.blocks.iter() {
            block.clear_selection();
        }
    }

    /// Converts the node to markdown format.
    ///
    /// This is used to generate markdown for test.
    #[allow(dead_code)]
    pub(crate) fn to_markdown(&self) -> String {
        self.blocks
            .iter()
            .map(|child| child.to_markdown())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub(super) fn render_root(
        &self,
        list_state: Option<ListState>,
        node_cx: &NodeContext,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let Some(list_state) = list_state else {
            let blocks_len = self.blocks.len();
            return div()
                .id("document")
                .children(self.blocks.iter().enumerate().map(move |(ix, node)| {
                    let is_last = ix + 1 == blocks_len;
                    let block_bounds = node_cx.block_bounds.clone();
                    div()
                        .relative()
                        .child(node.render_block(
                            NodeRenderOptions {
                                ix,
                                is_last,
                                ..Default::default()
                            },
                            node_cx,
                            window,
                            cx,
                        ))
                        .on_prepaint(move |bounds, _, _| {
                            if let Ok(mut blocks) = block_bounds.lock() {
                                blocks.insert(ix, bounds);
                            }
                        })
                }));
        };

        let options = NodeRenderOptions {
            is_last: true,
            ..Default::default()
        };

        let blocks = &self.blocks;

        if list_state.item_count() != blocks.len() {
            list_state.reset(blocks.len());
        }

        div().id("document").size_full().child(
            gpui::list(list_state, {
                let node_cx = node_cx.clone();
                let blocks = blocks.clone();
                move |ix, window, cx| {
                    let is_last = ix + 1 == blocks.len();
                    let block_bounds = node_cx.block_bounds.clone();
                    div()
                        .relative()
                        .child(blocks[ix].render_block(
                            NodeRenderOptions {
                                ix,
                                is_last,
                                ..options
                            },
                            &node_cx,
                            window,
                            cx,
                        ))
                        .on_prepaint(move |bounds, _, _| {
                            if let Ok(mut blocks) = block_bounds.lock() {
                                blocks.insert(ix, bounds);
                            }
                        })
                        .into_any_element()
                }
            })
            .size_full(),
        )
    }
}
