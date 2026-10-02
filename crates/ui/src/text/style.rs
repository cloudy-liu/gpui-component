use std::sync::Arc;

use gpui::{Hsla, Pixels, Rems, StyleRefinement, px, rems};

use crate::highlighter::HighlightTheme;

/// TextViewStyle used to customize the style for [`TextView`].
#[derive(Clone)]
pub struct TextViewStyle {
    /// Gap of each paragraphs, default is 1 rem.
    pub paragraph_gap: Rems,
    /// Base font size for headings, default is 14px.
    pub heading_base_font_size: Pixels,
    /// Function to calculate heading font size based on heading level (1-6).
    ///
    /// The first parameter is the heading level (1-6), the second parameter is the base font size.
    /// The second parameter is the base font size.
    pub heading_font_size: Option<Arc<dyn Fn(u8, Pixels) -> Pixels + Send + Sync + 'static>>,
    /// Highlight theme for code blocks. Default: [`HighlightTheme::default_light()`]
    pub highlight_theme: Arc<HighlightTheme>,
    /// The style refinement for code blocks.
    pub code_block: StyleRefinement,
    /// Style refinement applied to the table container (the bordered wrapper).
    ///
    /// Set `overflow_x: scroll` here to keep table cells on a single line and
    /// scroll the table horizontally instead of wrapping cell content, e.g.
    /// `TextViewStyle::default().table({ let mut s = StyleRefinement::default(); s.overflow.x = Some(Overflow::Scroll); s })`.
    pub table: StyleRefinement,
    /// Style refinement applied to each table cell.
    pub table_cell: StyleRefinement,
    /// Per-level heading refinements, applied after the default heading style.
    pub headings: [StyleRefinement; 6],
    pub blockquote: StyleRefinement,
    pub nested_blockquote: StyleRefinement,
    pub alert: StyleRefinement,
    pub table_header: StyleRefinement,
    pub list_item: StyleRefinement,
    pub link_color: Option<Hsla>,
    pub link_hover_color: Option<Hsla>,
    pub inline_code_color: Option<Hsla>,
    pub inline_code_background: Option<Hsla>,
    pub inline_code_font: Option<gpui::SharedString>,
    pub inline_code_fallbacks: Option<gpui::FontFallbacks>,
    /// Border colour of an inline code chip. When set, the chip is painted as
    /// a rounded box (background and border) instead of a plain text-run
    /// background.
    pub inline_code_border: Option<Hsla>,
    /// Colour of the bullet / number in front of a list item.
    pub list_marker_color: Option<Hsla>,
    pub selection_color: Option<Hsla>,
    pub border_color: Option<Hsla>,
    pub task_color: Option<Hsla>,
    pub task_foreground: Option<Hsla>,
    pub quote_link_color: Option<Hsla>,
    pub quote_code_color: Option<Hsla>,
    pub quote_code_background: Option<Hsla>,
    pub table_code_background: Option<Hsla>,
    pub table_hover_background: Option<Hsla>,
    pub is_dark: bool,
}

impl PartialEq for TextViewStyle {
    fn eq(&self, other: &Self) -> bool {
        self.paragraph_gap == other.paragraph_gap
            && self.heading_base_font_size == other.heading_base_font_size
            && self.highlight_theme == other.highlight_theme
            && match (&self.heading_font_size, &other.heading_font_size) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && self.code_block == other.code_block
            && self.table == other.table
            && self.table_cell == other.table_cell
            && self.headings == other.headings
            && self.blockquote == other.blockquote
            && self.nested_blockquote == other.nested_blockquote
            && self.alert == other.alert
            && self.table_header == other.table_header
            && self.list_item == other.list_item
            && self.link_color == other.link_color
            && self.link_hover_color == other.link_hover_color
            && self.inline_code_color == other.inline_code_color
            && self.inline_code_background == other.inline_code_background
            && self.inline_code_font == other.inline_code_font
            && self.inline_code_fallbacks == other.inline_code_fallbacks
            && self.inline_code_border == other.inline_code_border
            && self.list_marker_color == other.list_marker_color
            && self.selection_color == other.selection_color
            && self.border_color == other.border_color
            && self.task_color == other.task_color
            && self.task_foreground == other.task_foreground
            && self.quote_link_color == other.quote_link_color
            && self.quote_code_color == other.quote_code_color
            && self.quote_code_background == other.quote_code_background
            && self.table_code_background == other.table_code_background
            && self.table_hover_background == other.table_hover_background
            && self.is_dark == other.is_dark
    }
}

impl Default for TextViewStyle {
    fn default() -> Self {
        Self {
            paragraph_gap: rems(1.),
            heading_base_font_size: px(14.),
            heading_font_size: None,
            highlight_theme: HighlightTheme::default_light().clone(),
            code_block: StyleRefinement::default(),
            table: StyleRefinement::default(),
            table_cell: StyleRefinement::default(),
            headings: std::array::from_fn(|_| StyleRefinement::default()),
            blockquote: StyleRefinement::default(),
            nested_blockquote: StyleRefinement::default(),
            alert: StyleRefinement::default(),
            table_header: StyleRefinement::default(),
            list_item: StyleRefinement::default(),
            link_color: None,
            link_hover_color: None,
            inline_code_color: None,
            inline_code_background: None,
            inline_code_font: None,
            inline_code_fallbacks: None,
            inline_code_border: None,
            list_marker_color: None,
            selection_color: None,
            border_color: None,
            task_color: None,
            task_foreground: None,
            quote_link_color: None,
            quote_code_color: None,
            quote_code_background: None,
            table_code_background: None,
            table_hover_background: None,
            is_dark: false,
        }
    }
}

impl TextViewStyle {
    /// Set paragraph gap, default is 1 rem.
    pub fn paragraph_gap(mut self, gap: Rems) -> Self {
        self.paragraph_gap = gap;
        self
    }

    pub fn heading_font_size<F>(mut self, f: F) -> Self
    where
        F: Fn(u8, Pixels) -> Pixels + Send + Sync + 'static,
    {
        self.heading_font_size = Some(Arc::new(f));
        self
    }

    /// Set style for code blocks.
    pub fn code_block(mut self, style: StyleRefinement) -> Self {
        self.code_block = style;
        self
    }

    /// Set extra style for the table container.
    ///
    /// Set `overflow_x: scroll` on the refinement to make wide tables scroll
    /// horizontally (cells stop wrapping) instead of shrinking to fit.
    pub fn table(mut self, style: StyleRefinement) -> Self {
        self.table = style;
        self
    }

    /// Set extra style for each table cell.
    pub fn table_cell(mut self, style: StyleRefinement) -> Self {
        self.table_cell = style;
        self
    }
}
