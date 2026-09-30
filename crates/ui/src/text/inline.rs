use gpui::Corners;
use std::{
    ops::Range,
    rc::Rc,
    sync::{Arc, Mutex},
};

use gpui::{
    App, BorderStyle, Bounds, CursorStyle, Edges, Element, ElementId, GlobalElementId, Half,
    HighlightStyle, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, SharedString, StyledText,
    TextLayout, Window, point, px, quad,
};

use crate::{
    ActiveTheme, WindowExt as _, global_state::GlobalState, input::Selection,
    text::TextViewMultiClickKind, text::node::LinkMark, text::selection::word_range_at,
};

/// A inline element used to render a inline text and support selectable.
///
/// All text in TextView (including the CodeBlock) used this for text rendering.
pub(super) struct Inline {
    id: ElementId,
    text: SharedString,
    links: Rc<Vec<(Range<usize>, LinkMark)>>,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    styled_text: StyledText,
    hover_color: Option<gpui::Hsla>,
    code_ranges: Vec<Range<usize>>,
    code_font: Option<SharedString>,
    code_fallbacks: Option<gpui::FontFallbacks>,

    state: Arc<Mutex<InlineState>>,
}

/// The inline text state, used RefCell to keep the selection state.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct InlineState {
    hovered_index: Option<usize>,
    /// The text that actually rendering, matched with selection.
    pub(super) text: SharedString,
    pub(super) selection: Option<Selection>,
}

impl InlineState {
    /// Save actually rendered text for selected text to use.
    pub(crate) fn set_text(&mut self, text: SharedString) {
        self.text = text;
    }
}

impl Inline {
    pub(super) fn new(
        id: impl Into<ElementId>,
        state: Arc<Mutex<InlineState>>,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    ) -> Self {
        let text = state
            .lock()
            .map(|state| state.text.clone())
            .unwrap_or_default();

        Self {
            id: id.into(),
            links: Rc::new(links),
            highlights,
            text: text.clone(),
            styled_text: StyledText::new(text),
            hover_color: None,
            code_ranges: Vec::new(),
            code_font: None,
            code_fallbacks: None,
            state,
        }
    }

    pub(super) fn reading_style(
        mut self,
        style: &super::TextViewStyle,
        code_ranges: Vec<Range<usize>>,
    ) -> Self {
        self.hover_color = style.link_hover_color;
        self.code_font = style.inline_code_font.clone();
        self.code_fallbacks = style.inline_code_fallbacks.clone();
        self.code_ranges = code_ranges;
        self
    }

    /// Get link at given mouse position.
    fn link_for_position(
        layout: &TextLayout,
        links: &Vec<(Range<usize>, LinkMark)>,
        position: Point<Pixels>,
    ) -> Option<LinkMark> {
        let offset = layout.index_for_position(position).ok()?;
        for (range, link) in links.iter() {
            if range.contains(&offset) {
                return Some(link.clone());
            }
        }

        None
    }

    /// Paint selected bounds for debug.
    #[allow(unused)]
    fn paint_selected_bounds(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        window.paint_quad(gpui::PaintQuad {
            bounds,
            background: cx.theme().blue.alpha(0.01).into(),
            corner_radii: Corners::default(),
            border_color: gpui::transparent_black(),
            border_style: BorderStyle::default(),
            border_widths: gpui::Edges::all(px(0.)),
        });
    }

    fn layout_selections(
        &self,
        text_layout: &TextLayout,
        bounds: &Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> (bool, bool, Option<Selection>) {
        let Some(text_view_state) = GlobalState::global(cx).text_view_state() else {
            return (false, false, None);
        };

        let text_view_state = text_view_state.read(cx);
        let is_selectable = text_view_state.is_selectable();
        if !is_selectable {
            return (false, false, None);
        }

        if text_view_state.is_all_selected() {
            return (is_selectable, true, Some((0..self.text.len()).into()));
        }

        if let Some(selection) = text_view_state.multi_click_selection() {
            return (
                is_selectable,
                true,
                selection_for_multi_click(
                    &self.text,
                    text_layout,
                    *bounds,
                    selection.pos,
                    selection.kind,
                )
                .map(Selection::from),
            );
        }

        let Some((selection_start, selection_end)) = text_view_state.selection_points(window, cx)
        else {
            return (is_selectable, false, None);
        };
        let line_height = window.line_height();
        let mask_bounds = window.content_mask().bounds;

        // Use for debug selection bounds
        // self.paint_selected_bounds(Bounds::from_corners(selection_start, selection_end), window, cx);

        let mut selection: Option<Selection> = None;
        let mut offset = 0;
        let mut chars = self.text.chars().peekable();
        while let Some(c) = chars.next() {
            let Some(pos) = text_layout.position_for_index(offset) else {
                offset += c.len_utf8();
                continue;
            };

            let next_offset = offset + c.len_utf8();
            let mut char_width = line_height.half();
            if let Some(next_pos) = text_layout.position_for_index(next_offset) {
                if next_pos.y == pos.y {
                    char_width = next_pos.x - pos.x;
                }
            }

            let char_center = point(pos.x + char_width.half(), pos.y + line_height.half());
            if mask_bounds.contains(&char_center)
                && point_in_text_selection(
                    pos,
                    char_width,
                    selection_start,
                    selection_end,
                    line_height,
                )
            {
                if selection.is_none() {
                    selection = Some((offset..offset).into());
                }

                if let Some(selection) = selection.as_mut() {
                    selection.end = next_offset;
                }
            }

            offset = next_offset;
        }

        (true, true, selection)
    }

    fn text_line_bounds(
        &self,
        text_layout: &TextLayout,
        line_height: Pixels,
        mask_bounds: Bounds<Pixels>,
    ) -> Vec<Bounds<Pixels>> {
        let mut line_bounds = Vec::new();
        let mut current_line_y = None;
        let mut current_bounds: Option<Bounds<Pixels>> = None;
        let mut offset = 0;

        for c in self.text.chars() {
            let next_offset = offset + c.len_utf8();
            let Some(pos) = text_layout.position_for_index(offset) else {
                offset = next_offset;
                continue;
            };

            let mut char_width = line_height.half();
            if let Some(next_pos) = text_layout.position_for_index(next_offset) {
                if next_pos.y == pos.y {
                    char_width = next_pos.x - pos.x;
                }
            }

            let bounds = Bounds::from_corners(pos, point(pos.x + char_width, pos.y + line_height))
                .intersect(&mask_bounds);
            if bounds.size.width > px(0.) && bounds.size.height > px(0.) {
                if current_line_y == Some(pos.y) {
                    if let Some(current) = current_bounds.as_mut() {
                        *current = current.union(&bounds);
                    }
                } else {
                    if let Some(current) = current_bounds.take() {
                        line_bounds.push(current);
                    }
                    current_line_y = Some(pos.y);
                    current_bounds = Some(bounds);
                }
            }

            offset = next_offset;
        }

        if let Some(current) = current_bounds {
            line_bounds.push(current);
        }

        line_bounds
    }

    /// Paint the selection background.
    fn paint_selection(
        selection: &Selection,
        text_layout: &TextLayout,
        bounds: &Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let selection_color = GlobalState::global(cx)
            .text_view_state()
            .and_then(|state| state.read(cx).text_view_style.selection_color)
            .unwrap_or(cx.theme().selection);
        let mut start = selection.start;
        let mut end = selection.end;
        if end < start {
            std::mem::swap(&mut start, &mut end);
        }
        let Some(start_position) = text_layout.position_for_index(start) else {
            return;
        };
        let Some(end_position) = text_layout.position_for_index(end) else {
            return;
        };

        let line_height = text_layout.line_height();
        if start_position.y == end_position.y {
            window.paint_quad(quad(
                Bounds::from_corners(
                    start_position,
                    point(end_position.x, end_position.y + line_height),
                ),
                px(0.),
                selection_color,
                Edges::default(),
                gpui::transparent_black(),
                BorderStyle::default(),
            ));
        } else {
            window.paint_quad(quad(
                Bounds::from_corners(
                    start_position,
                    point(bounds.right(), start_position.y + line_height),
                ),
                px(0.),
                selection_color,
                Edges::default(),
                gpui::transparent_black(),
                BorderStyle::default(),
            ));

            if end_position.y > start_position.y + line_height {
                window.paint_quad(quad(
                    Bounds::from_corners(
                        point(bounds.left(), start_position.y + line_height),
                        point(bounds.right(), end_position.y),
                    ),
                    px(0.),
                    selection_color,
                    Edges::default(),
                    gpui::transparent_black(),
                    BorderStyle::default(),
                ));
            }

            window.paint_quad(quad(
                Bounds::from_corners(
                    point(bounds.left(), end_position.y),
                    point(end_position.x, end_position.y + line_height),
                ),
                px(0.),
                selection_color,
                Edges::default(),
                gpui::transparent_black(),
                BorderStyle::default(),
            ));
        }
    }
}

impl IntoElement for Inline {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Inline {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_element_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let text_style = window.text_style();

        let hovered = self.state.lock().ok().and_then(|state| state.hovered_index);
        let mut highlights = self.highlights.clone();
        if let (Some(color), Some(index)) = (self.hover_color, hovered) {
            if let Some((range, _)) = self.links.iter().find(|(range, _)| range.contains(&index)) {
                highlights = gpui::combine_highlights(
                    highlights,
                    vec![(
                        range.clone(),
                        HighlightStyle {
                            color: Some(color),
                            ..Default::default()
                        },
                    )],
                )
                .collect();
            }
        }
        let runs = styled_runs(
            &self.text,
            &text_style,
            &highlights,
            &self.code_ranges,
            self.code_font.as_ref(),
            self.code_fallbacks.as_ref(),
        );

        self.styled_text = StyledText::new(self.text.clone()).with_runs(runs);
        let (layout_id, _) =
            self.styled_text
                .request_layout(global_element_id, inspector_id, window, cx);

        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled_text
            .prepaint(id, inspector_id, bounds, &mut (), window, cx);

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        hitbox
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let current_view = window.current_view();
        let hitbox = prepaint;
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        let text_layout = self.styled_text.layout().clone();
        self.styled_text
            .paint(global_id, None, bounds, &mut (), &mut (), window, cx);

        // layout selections
        let (is_selectable, is_selection, selection) =
            self.layout_selections(&text_layout, &bounds, window, cx);

        state.selection = selection;

        if is_selection || is_selectable {
            window.set_cursor_style(CursorStyle::IBeam, &hitbox);
        }

        // link cursor pointer
        let mouse_position = window.mouse_position();
        if let Some(_) = Self::link_for_position(&text_layout, &self.links, mouse_position) {
            window.set_cursor_style(CursorStyle::PointingHand, &hitbox);
        }

        if let Some(selection) = &state.selection {
            Self::paint_selection(selection, &text_layout, &bounds, window, cx);
        }

        if is_selectable {
            if let Some(text_view_state) = GlobalState::global(cx).text_view_state().cloned() {
                let text_bounds = self.text_line_bounds(
                    &text_layout,
                    text_layout.line_height(),
                    window.content_mask().bounds,
                );
                crate::Root::register_selectable_text_inline(
                    &text_view_state,
                    text_bounds,
                    window,
                    cx,
                );
            }

            window.on_mouse_event({
                let hitbox = hitbox.clone();
                let text_layout = text_layout.clone();
                let inline_state = self.state.clone();
                let text = self.text.clone();
                let text_view_state = GlobalState::global(cx).text_view_state().cloned();

                move |event: &MouseDownEvent, phase, window, cx| {
                    if !phase.bubble()
                        || !hitbox.is_hovered(window)
                        || event.button != MouseButton::Left
                    {
                        return;
                    }

                    let kind = match event.click_count {
                        2 => TextViewMultiClickKind::Word,
                        3 => TextViewMultiClickKind::Paragraph,
                        _ => return,
                    };

                    let Some(range) = selection_for_multi_click(
                        &text,
                        &text_layout,
                        hitbox.bounds,
                        event.position,
                        kind,
                    ) else {
                        return;
                    };

                    let selected_text = text[range.clone()].to_string();

                    if let Ok(mut inline_state) = inline_state.lock() {
                        inline_state.selection = Some(range.into());
                    }
                    if let Some(text_view_state) = &text_view_state {
                        text_view_state.update(cx, |state, _| {
                            state.set_multi_click_selection(event.position, kind, selected_text);
                        });
                    }
                    cx.notify(current_view);
                }
            });
        }

        // mouse move, update hovered link
        window.on_mouse_event({
            let hitbox = hitbox.clone();
            let text_layout = text_layout.clone();
            let inline_state = self.state.clone();
            let links = self.links.clone();
            move |event: &MouseMoveEvent, phase, window, cx| {
                if !phase.bubble() {
                    return;
                }
                let updated = hitbox
                    .is_hovered(window)
                    .then(|| text_layout.index_for_position(event.position).ok())
                    .flatten()
                    .and_then(|index| {
                        links
                            .iter()
                            .find(|(range, _)| range.contains(&index))
                            .map(|(range, _)| range.start)
                    });
                if let Ok(mut state) = inline_state.lock() {
                    if state.hovered_index != updated {
                        state.hovered_index = updated;
                        cx.notify(current_view);
                    }
                }
            }
        });

        if !is_selection {
            // click to open link
            window.on_mouse_event({
                let links = self.links.clone();
                let text_layout = text_layout.clone();
                let hitbox = hitbox.clone();
                let text_view_state = GlobalState::global(cx).text_view_state().cloned();

                move |event: &MouseUpEvent, phase, window, cx| {
                    if event.button != MouseButton::Left
                        || !phase.bubble()
                        || !hitbox.is_hovered(window)
                    {
                        return;
                    }
                    if text_view_state
                        .as_ref()
                        .is_some_and(|state| state.read(cx).has_selection(window, cx))
                    {
                        return;
                    }

                    if let Some(link) =
                        Self::link_for_position(&text_layout, &links, event.position)
                    {
                        window.end_text_selection(cx);
                        cx.stop_propagation();
                        if let Some(state) = &text_view_state {
                            let interactions = state.read(cx).interactions.clone();
                            interactions.open_link(&link.url, window, cx);
                        } else {
                            cx.open_url(&link.url);
                        }
                    }
                }
            });
        }
    }
}

/// Keep measurement and painting on identical font runs, including inline code.
pub(super) fn styled_runs(
    text: &str,
    base: &gpui::TextStyle,
    highlights: &[(Range<usize>, HighlightStyle)],
    code_ranges: &[Range<usize>],
    code_font: Option<&SharedString>,
    code_fallbacks: Option<&gpui::FontFallbacks>,
) -> Vec<gpui::TextRun> {
    // Syntax highlighting and combine_highlights return sorted, disjoint spans.
    // Walk them once: scanning every token for every run is quadratic on code.
    debug_assert!(
        highlights
            .windows(2)
            .all(|pair| pair[0].0.end <= pair[1].0.start)
    );
    let mut code = code_ranges.to_vec();
    code.sort_unstable_by_key(|range| range.start);
    let mut merged_code: Vec<Range<usize>> = Vec::with_capacity(code.len());
    for range in code.into_iter().filter(|range| !range.is_empty()) {
        if let Some(previous) = merged_code
            .last_mut()
            .filter(|previous| range.start <= previous.end)
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged_code.push(range);
        }
    }
    let mut runs = Vec::with_capacity(highlights.len() * 2 + merged_code.len() * 2 + 1);
    let (mut start, mut highlight_ix, mut code_ix) = (0, 0, 0);
    while start < text.len() {
        while highlights
            .get(highlight_ix)
            .is_some_and(|(range, _)| range.end <= start)
        {
            highlight_ix += 1;
        }
        while merged_code
            .get(code_ix)
            .is_some_and(|range| range.end <= start)
        {
            code_ix += 1;
        }
        let mut end = text.len();
        let mut style = base.clone();
        if let Some((range, highlight)) = highlights.get(highlight_ix) {
            if range.contains(&start) {
                style = style.highlight(*highlight);
                end = end.min(range.end);
            } else {
                end = end.min(range.start);
            }
        }
        if let Some(range) = merged_code.get(code_ix) {
            if range.contains(&start) {
                if let Some(font) = code_font {
                    style.font_family = font.clone();
                }
                if let Some(fallbacks) = code_fallbacks {
                    style.font_fallbacks = Some(fallbacks.clone());
                }
                end = end.min(range.end);
            } else {
                end = end.min(range.start);
            }
        }
        debug_assert!(text.is_char_boundary(end));
        runs.push(style.to_run(end - start));
        start = end;
    }
    runs
}

fn selection_for_multi_click(
    text: &str,
    text_layout: &TextLayout,
    bounds: Bounds<Pixels>,
    pos: Point<Pixels>,
    kind: TextViewMultiClickKind,
) -> Option<std::ops::Range<usize>> {
    if !bounds.contains(&pos) {
        return None;
    }

    let offset = text_layout.index_for_position(pos).ok()?;

    match kind {
        TextViewMultiClickKind::Word => word_range_at(text, offset),
        // Known limitation: a paragraph maps to a single Inline run here. When a
        // paragraph embeds an inline image it is split into multiple Inline runs,
        // so triple-click only selects the run on the clicked side of the image.
        TextViewMultiClickKind::Paragraph => (!text.is_empty()).then_some(0..text.len()),
    }
}

/// Check if a `pos` is within a `bounds`, considering multi-line selections.
fn point_in_text_selection(
    pos: Point<Pixels>,
    char_width: Pixels,
    selection_start: Point<Pixels>,
    selection_end: Point<Pixels>,
    line_height: Pixels,
) -> bool {
    let point_in_line = |point: Point<Pixels>| point.y >= pos.y && point.y < pos.y + line_height;
    let top = selection_start.y.min(selection_end.y);
    let bottom = selection_start.y.max(selection_end.y);
    let x = pos.x + char_width.half();

    // Out of the vertical bounds
    if pos.y + line_height <= top || pos.y > bottom {
        return false;
    }

    // Treat the selection as single-line when both drag points fall within the
    // same rendered line, even if their y coordinates differ inside that line.
    if point_in_line(selection_start) && point_in_line(selection_end) {
        let left = selection_start.x.min(selection_end.x);
        let right = selection_start.x.max(selection_end.x);
        return x >= left && x <= right;
    }

    let (top_point, bottom_point) = if selection_start.y < selection_end.y {
        (selection_start, selection_end)
    } else {
        (selection_end, selection_start)
    };
    let is_top_line = point_in_line(top_point);
    let is_bottom_line = point_in_line(bottom_point);

    if is_top_line {
        return x >= top_point.x;
    } else if is_bottom_line {
        return x <= bottom_point.x;
    } else {
        return true;
    }
}

#[cfg(test)]
mod tests {
    use super::point_in_text_selection;
    use gpui::{point, px};

    #[test]
    fn test_point_in_text_selection() {
        let line_height = px(20.);
        let char_width = px(10.);
        let start = point(px(50.), px(50.));
        let end = point(px(150.), px(150.));

        // First line but haft line height, true
        // | p --------|
        // | selection |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(50.), px(40.)),
            char_width,
            start,
            end,
            line_height
        ));

        // First line in selection, true
        // | p --------|
        // | selection |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(50.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        // First line, but left out of selection, false
        // p |-----------|
        //   | selection |
        //   |-----------|
        assert!(!point_in_text_selection(
            point(px(40.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        // First line but right out of selection, true
        // |-----------| p
        // | selection |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(160.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));

        // Middle line in selection, true
        // |-----------|
        // |     p     |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(100.), px(70.)),
            char_width,
            start,
            end,
            line_height
        ));
        // Middle line, but left out of selection, true
        //   |-----------|
        // p | selection |
        //   |-----------|
        assert!(point_in_text_selection(
            point(px(40.), px(70.)),
            char_width,
            start,
            end,
            line_height
        ));
        // Middle line, but right out of selection, true
        // |-----------|
        // | selection | p
        // |-----------|
        assert!(point_in_text_selection(
            point(px(160.), px(70.)),
            char_width,
            start,
            end,
            line_height
        ));

        // Last line in selection, true
        // |-----------|
        // | selection |
        // |------- p -|
        assert!(point_in_text_selection(
            point(px(100.), px(140.)),
            char_width,
            start,
            end,
            line_height
        ));
        // Last line, but left out of selection, true
        //
        //   |-----------|
        //   | selection |
        // p |-----------|
        assert!(point_in_text_selection(
            point(px(40.), px(140.)),
            char_width,
            start,
            end,
            line_height
        ));
        // Last line, but right out of selection, false
        // |-----------|
        // | selection |
        // |-----------| p
        assert!(!point_in_text_selection(
            point(px(160.), px(140.)),
            char_width,
            start,
            end,
            line_height
        ));

        // Out of vertical bounds (top), false
        //       p
        // |-----------|
        // | selection |
        // |-----------|
        assert!(!point_in_text_selection(
            point(px(100.), px(20.)),
            char_width,
            start,
            end,
            line_height
        ));
        // Out of vertical bounds (bottom), false
        // |-----------|
        // | selection |
        // |-----------|
        //       p
        assert!(!point_in_text_selection(
            point(px(100.), px(160.)),
            char_width,
            start,
            end,
            line_height
        ));
    }

    #[test]
    fn test_point_in_text_selection_reversed_drag_direction() {
        let line_height = px(20.);
        let char_width = px(10.);

        // Mouse down on lower line then drag upward to x=150.
        // Top line should follow current mouse x, bottom line should keep anchor x.
        let start = point(px(80.), px(150.));
        let end = point(px(150.), px(50.));

        // On top line, selection starts from top cursor x (150), so x=140 should be excluded.
        assert!(!point_in_text_selection(
            point(px(140.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(point_in_text_selection(
            point(px(150.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));

        // On bottom line, selection ends at anchor x (80), so x=90 should be excluded.
        assert!(point_in_text_selection(
            point(px(75.), px(140.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(!point_in_text_selection(
            point(px(80.), px(140.)),
            char_width,
            start,
            end,
            line_height
        ));
    }

    #[test]
    fn test_point_in_text_selection_same_visual_line_with_different_y() {
        let line_height = px(20.);
        let char_width = px(10.);
        let start = point(px(100.), px(55.));
        let end = point(px(60.), px(58.));

        assert!(!point_in_text_selection(
            point(px(40.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(point_in_text_selection(
            point(px(70.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(!point_in_text_selection(
            point(px(110.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
    }

    #[test]
    fn test_point_in_text_selection_same_visual_line_with_reversed_y() {
        let line_height = px(20.);
        let char_width = px(10.);
        let start = point(px(60.), px(58.));
        let end = point(px(100.), px(55.));

        assert!(!point_in_text_selection(
            point(px(40.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(point_in_text_selection(
            point(px(70.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(!point_in_text_selection(
            point(px(110.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
    }
}
