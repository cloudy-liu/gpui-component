use std::sync::Arc;

use gpui::{Hsla, Pixels, Rems, StyleRefinement, px, rems};

use crate::highlighter::HighlightTheme;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkUnderline {
    #[default]
    Always,
    Hover,
    Never,
}

#[derive(Clone, PartialEq)]
pub struct AlertStyle {
    pub container: StyleRefinement,
    pub title: StyleRefinement,
    /// Application-supplied icon asset; the component does not select a theme.
    pub icon: gpui::SharedString,
    pub icon_size: Pixels,
}

#[derive(Clone, PartialEq)]
pub struct InlineCodeStyle {
    pub radius: Pixels,
    pub padding_x: Pixels,
    pub padding_y: Pixels,
    pub font_size: Pixels,
}

#[derive(Clone, PartialEq)]
pub struct KeyboardStyle {
    pub background: Hsla,
    pub border: Hsla,
    pub shadow: Hsla,
    pub radius: Pixels,
    pub padding: Pixels,
    pub font_size: Pixels,
    pub line_height: Pixels,
}

/// TextViewStyle used to customize the style for [`TextView`].
#[derive(Clone)]
pub struct TextViewStyle {
    pub bold_weight: gpui::FontWeight,
    pub link_underline: LinkUnderline,
    /// 1, i, a at nested depths; false keeps the legacy 1, A, a sequence.
    pub roman_ordered_lists: bool,
    pub list_indent: Option<Pixels>,
    pub list_paragraph_gap: Option<Pixels>,
    pub alerts: [Option<AlertStyle>; 5],
    pub table_fill: bool,
    pub table_radius: Option<Pixels>,
    /// Space after a table; unset preserves the original one-rem gap.
    pub table_gap: Option<Pixels>,
    pub table_stripe: Option<Hsla>,
    pub table_row_border: Option<Hsla>,
    pub horizontal_rule: StyleRefinement,
    pub horizontal_rule_container: StyleRefinement,
    pub inline_code: Option<InlineCodeStyle>,
    pub keyboard: Option<KeyboardStyle>,
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
        self.bold_weight == other.bold_weight
            && self.link_underline == other.link_underline
            && self.roman_ordered_lists == other.roman_ordered_lists
            && self.list_indent == other.list_indent
            && self.list_paragraph_gap == other.list_paragraph_gap
            && self.alerts == other.alerts
            && self.table_fill == other.table_fill
            && self.table_radius == other.table_radius
            && self.table_gap == other.table_gap
            && self.table_stripe == other.table_stripe
            && self.table_row_border == other.table_row_border
            && self.horizontal_rule == other.horizontal_rule
            && self.horizontal_rule_container == other.horizontal_rule_container
            && self.inline_code == other.inline_code
            && self.keyboard == other.keyboard
            && self.paragraph_gap == other.paragraph_gap
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
            bold_weight: gpui::FontWeight::BOLD,
            link_underline: LinkUnderline::Always,
            roman_ordered_lists: false,
            list_indent: None,
            list_paragraph_gap: None,
            alerts: std::array::from_fn(|_| None),
            table_fill: true,
            table_radius: None,
            table_gap: None,
            table_stripe: None,
            table_row_border: None,
            horizontal_rule: StyleRefinement::default(),
            horizontal_rule_container: StyleRefinement::default(),
            inline_code: None,
            keyboard: None,
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
