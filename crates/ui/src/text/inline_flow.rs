use std::{
    ops::Range,
    sync::{Arc, Mutex},
};

use gpui::{
    AbsoluteLength, AnyElement, App, AvailableSpace, Bounds, DefiniteLength, Element, ElementId,
    GlobalElementId, HighlightStyle, InspectorElementId, InteractiveElement as _, IntoElement,
    LayoutId, LineFragment as WrapLineFragment, ObjectFit, Pixels, ShapedLine, SharedString,
    SharedUri, Size, Styled, StyledImage as _, TextRun, TextStyle, WhiteSpace, Window, img, point,
    prelude::FluentBuilder as _, px, relative, size,
};

use super::{
    inline::{ChipKind, Inline, InlineState},
    node::LinkMark,
};

const IMAGE_LEN: usize = 1;

pub(super) struct InlineFlow {
    id: ElementId,
    items: Vec<InlineFlowItem>,
    interactions: super::interaction::TextViewInteractions,
    reading_style: super::TextViewStyle,
}

pub(super) enum InlineFlowItem {
    Text {
        state: Arc<Mutex<InlineState>>,
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        code_ranges: Vec<Range<usize>>,
        keyboard_ranges: Vec<Range<usize>>,
        chip_kind: Option<ChipKind>,
    },
    Image {
        url: SharedUri,
        link: Option<LinkMark>,
        title: String,
        width: Option<DefiniteLength>,
        height: Option<DefiniteLength>,
    },
}

#[derive(Default)]
pub(crate) struct InlineFlowLayoutState {
    layout: Arc<Mutex<Option<InlineFlowLayout>>>,
}

#[derive(Default)]
struct InlineFlowLayout {
    fragments: Vec<PositionedFragment>,
    size: Size<Pixels>,
}

#[derive(Clone)]
enum PositionedFragment {
    Text {
        item_ix: usize,
        origin: gpui::Point<Pixels>,
        size: Size<Pixels>,
        source_range: Range<usize>,
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    },
    Image {
        item_ix: usize,
        origin: gpui::Point<Pixels>,
        size: Size<Pixels>,
    },
}

enum MeasureItem {
    Text {
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        code_ranges: Vec<Range<usize>>,
        chip_kind: Option<ChipKind>,
    },
    Image {
        url: SharedUri,
        title: String,
        width: Option<DefiniteLength>,
        height: Option<DefiniteLength>,
    },
}

struct LineFragmentLayout {
    item_ix: usize,
    kind: LineFragmentKind,
    size: Size<Pixels>,
    source_range: Range<usize>,
}

enum LineFragmentKind {
    Text {
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    },
    Image,
}

impl InlineFlow {
    pub(super) fn new(id: impl Into<ElementId>, items: Vec<InlineFlowItem>) -> Self {
        Self {
            id: id.into(),
            items,
            interactions: Default::default(),
            reading_style: Default::default(),
        }
    }

    pub(super) fn interactions(
        mut self,
        interactions: super::interaction::TextViewInteractions,
    ) -> Self {
        self.interactions = interactions;
        self
    }

    pub(super) fn reading_style(mut self, style: super::TextViewStyle) -> Self {
        if style.inline_code.is_some() || style.keyboard.is_some() {
            let mut split = Vec::new();
            for item in self.items {
                let InlineFlowItem::Text {
                    state,
                    text,
                    links,
                    highlights,
                    code_ranges,
                    keyboard_ranges,
                    ..
                } = item
                else {
                    split.push(item);
                    continue;
                };
                if code_ranges.is_empty() {
                    split.push(InlineFlowItem::Text {
                        state,
                        text,
                        links,
                        highlights,
                        code_ranges,
                        keyboard_ranges,
                        chip_kind: None,
                    });
                    continue;
                }
                let mut boundaries = vec![0, text.len()];
                for range in &code_ranges {
                    boundaries.extend([range.start, range.end]);
                }
                boundaries.sort_unstable();
                boundaries.dedup();
                let mut parent = state.lock().expect("inline state");
                parent.text = text.clone();
                let previous = parent.fragments.clone();
                parent.fragments.clear();
                for pair in boundaries.windows(2) {
                    let (start, end) = (pair[0], pair[1]);
                    let fragment = previous.get(&(start, end)).cloned().unwrap_or_default();
                    if let Ok(mut child) = fragment.lock() {
                        child.text = SharedString::from(text[start..end].to_string());
                        if let Some(selection) = parent.selection {
                            let lo = selection.start.max(start);
                            let hi = selection.end.min(end);
                            child.selection = (lo < hi).then(|| (lo - start..hi - start).into());
                        }
                    }
                    parent.fragments.insert((start, end), fragment.clone());
                    let code = code_ranges.iter().any(|r| r.contains(&start));
                    let keyboard = keyboard_ranges.iter().any(|r| r.contains(&start));
                    split.push(InlineFlowItem::Text {
                        state: fragment,
                        text: SharedString::from(text[start..end].to_string()),
                        links: slice_ranges(&links, start, end, |r, l| (r, l.clone())),
                        highlights: slice_ranges(&highlights, start, end, |r, h| (r, *h)),
                        code_ranges: if code {
                            vec![0..end - start]
                        } else {
                            Vec::new()
                        },
                        keyboard_ranges: Vec::new(),
                        chip_kind: code.then_some(if keyboard {
                            ChipKind::Keyboard
                        } else {
                            ChipKind::Code
                        }),
                    });
                }
                parent.selection = None;
            }
            self.items = split;
        }
        self.reading_style = style;
        self
    }
}

impl IntoElement for InlineFlow {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for InlineFlow {
    type RequestLayoutState = InlineFlowLayoutState;
    type PrepaintState = Vec<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let measure_items = self.items.iter().map(MeasureItem::from).collect::<Vec<_>>();
        let line_height = window.line_height();
        let rem_size = window.rem_size();
        let image_sizes = measure_items
            .iter()
            .enumerate()
            .map(|(ix, item)| match item {
                MeasureItem::Image {
                    url,
                    title,
                    width,
                    height,
                } => Some(measure_image_size(
                    ix,
                    url,
                    title,
                    *width,
                    *height,
                    line_height,
                    rem_size,
                    &self.interactions,
                    window,
                    cx,
                )),
                MeasureItem::Text { .. } => None,
            })
            .collect::<Vec<_>>();
        let layout_state = InlineFlowLayoutState::default();
        let layout_ref = layout_state.layout.clone();
        let reading_style = self.reading_style.clone();
        // Layout callbacks run after the parent's text-style scope has ended.
        // Capture the inherited heading/body font here, while it is in scope.
        let text_style = window.text_style();

        let layout_id = window.request_measured_layout(Default::default(), {
            move |known_dimensions, available_space, window, _cx| {
                let wrap_width = if text_style.white_space == WhiteSpace::Normal {
                    known_dimensions.width.or(match available_space.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    })
                } else {
                    None
                };
                let layout = layout_flow(
                    &measure_items,
                    &image_sizes,
                    &text_style,
                    wrap_width,
                    &reading_style,
                    window,
                );
                let size = layout.size;
                if let Ok(mut state) = layout_ref.lock() {
                    *state = Some(layout);
                }
                size
            }
        });

        (layout_id, layout_state)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let fragments = request_layout
            .layout
            .lock()
            .ok()
            .and_then(|layout| layout.as_ref().map(|layout| layout.fragments.clone()))
            .unwrap_or_default();
        let mut elements = Vec::with_capacity(fragments.len());

        // Fold last frame's wrapped pieces back into their source ranges before
        // splitting at the new line breaks. Keep their selection through reflow.
        for item in &self.items {
            if let InlineFlowItem::Text { state, .. } = item {
                if let Ok(mut state) = state.lock() {
                    state.collapse_fragments();
                }
            }
        }

        for fragment in fragments {
            match fragment {
                PositionedFragment::Text {
                    item_ix,
                    origin,
                    size: fragment_size,
                    source_range,
                    text,
                    links,
                    highlights,
                    ..
                } => {
                    let state = match &self.items[item_ix] {
                        InlineFlowItem::Text {
                            state,
                            text: source,
                            ..
                        } if source_range == (0..source.len()) => state.clone(),
                        InlineFlowItem::Text { state, .. } => {
                            let mut parent = state.lock().expect("inline state");
                            let child = Arc::new(Mutex::new(InlineState::default()));
                            if let Some(selection) = parent.selection {
                                let start = selection.start.max(source_range.start);
                                let end = selection.end.min(source_range.end);
                                if start < end {
                                    child.lock().expect("inline state").selection = Some(
                                        (start - source_range.start..end - source_range.start)
                                            .into(),
                                    );
                                }
                            }
                            parent
                                .fragments
                                .insert((source_range.start, source_range.end), child.clone());
                            child
                        }
                        _ => unreachable!("text fragment must refer to text"),
                    };
                    if let Ok(mut state) = state.lock() {
                        state.set_text(text);
                    }

                    let code = match &self.items[item_ix] {
                        InlineFlowItem::Text { code_ranges, .. } => {
                            clip_code_ranges(code_ranges, source_range.start, source_range.end)
                        }
                        _ => Vec::new(),
                    };
                    let chip_kind = match &self.items[item_ix] {
                        InlineFlowItem::Text { chip_kind, .. } => *chip_kind,
                        _ => None,
                    };
                    let ChipMetrics {
                        font_size,
                        padding_x: pad_x,
                        padding_y: pad_y,
                        line_height: text_height,
                    } = chip_metrics(
                        chip_kind,
                        &self.reading_style,
                        window.text_style().font_size.to_pixels(window.rem_size()),
                        window.line_height(),
                    );
                    let inline = Inline::new(elements.len(), state, links, highlights)
                        .reading_style(&self.reading_style, code);
                    let mut element = match chip_kind {
                        Some(keyboard) => inline.chip(keyboard),
                        None => inline,
                    }
                    .into_any_element();
                    let mut fragment_style = window.text_style();
                    fragment_style.font_size = font_size.into();
                    if chip_kind == Some(ChipKind::Keyboard) {
                        fragment_style.line_height = text_height.into();
                    }
                    window.with_text_style(
                        Some(gpui::TextStyleRefinement {
                            font_size: Some(fragment_style.font_size),
                            line_height: Some(fragment_style.line_height),
                            // InlineFlow already chose the line breaks. The
                            // child layout rounds fractional widths at the
                            // device scale; wrapping again can move its last
                            // word below the next positioned code fragment.
                            white_space: Some(WhiteSpace::Nowrap),
                            ..Default::default()
                        }),
                        |window| {
                            element.prepaint_as_root(
                                bounds.origin + origin + point(pad_x, pad_y),
                                size(
                                    AvailableSpace::Definite(fragment_size.width - pad_x * 2.),
                                    AvailableSpace::Definite(fragment_size.height - pad_y * 2.),
                                ),
                                window,
                                cx,
                            );
                        },
                    );
                    elements.push(element);
                }
                PositionedFragment::Image {
                    item_ix,
                    origin,
                    size: fragment_size,
                } => {
                    let InlineFlowItem::Image {
                        url, link, title, ..
                    } = &self.items[item_ix]
                    else {
                        continue;
                    };
                    let mut element = self.interactions.image(
                        elements.len(),
                        url,
                        link,
                        title.as_str(),
                        Some(fragment_size.width.into()),
                        Some(fragment_size.height.into()),
                        window,
                        cx,
                    );
                    element.prepaint_as_root(
                        bounds.origin + origin,
                        size(
                            AvailableSpace::Definite(fragment_size.width),
                            AvailableSpace::Definite(fragment_size.height),
                        ),
                        window,
                        cx,
                    );
                    elements.push(element);
                }
            }
        }

        elements
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for element in prepaint {
            element.paint(window, cx);
        }
    }
}

impl From<&InlineFlowItem> for MeasureItem {
    fn from(item: &InlineFlowItem) -> Self {
        match item {
            InlineFlowItem::Text {
                state: _,
                text,
                links,
                highlights,
                code_ranges,
                ..
            } => MeasureItem::Text {
                text: text.clone(),
                links: links.clone(),
                highlights: highlights.clone(),
                code_ranges: code_ranges.clone(),
                chip_kind: match item {
                    InlineFlowItem::Text { chip_kind, .. } => *chip_kind,
                    _ => None,
                },
            },
            InlineFlowItem::Image {
                url,
                title,
                width,
                height,
                ..
            } => MeasureItem::Image {
                url: url.clone(),
                title: title.clone(),
                width: *width,
                height: *height,
            },
        }
    }
}

impl MeasureItem {
    fn len(&self) -> usize {
        match self {
            MeasureItem::Text { text, .. } => text.len(),
            MeasureItem::Image { .. } => IMAGE_LEN,
        }
    }
}

fn layout_flow(
    items: &[MeasureItem],
    image_sizes: &[Option<Size<Pixels>>],
    text_style: &TextStyle,
    wrap_width: Option<Pixels>,
    reading_style: &super::TextViewStyle,
    window: &mut Window,
) -> InlineFlowLayout {
    let line_height = text_style.line_height_in_pixels(window.rem_size());
    let rem_size = window.rem_size();
    let total_len = items.iter().map(MeasureItem::len).sum::<usize>();
    if total_len == 0 {
        return InlineFlowLayout::default();
    }

    let line_ranges = line_ranges(
        items,
        image_sizes,
        text_style,
        wrap_width,
        reading_style,
        window,
    );
    let font_size = text_style.font_size.to_pixels(rem_size);
    let mut fragments = Vec::new();
    let mut max_width = Pixels::ZERO;
    let mut y = Pixels::ZERO;

    for line_range in line_ranges {
        let mut line_fragments = Vec::new();
        let mut line_width = Pixels::ZERO;
        let mut actual_line_height = line_height;
        let mut item_start = 0;

        for (item_ix, item) in items.iter().enumerate() {
            let item_end = item_start + item.len();
            if item_end <= line_range.start {
                item_start = item_end;
                continue;
            }
            if item_start >= line_range.end {
                break;
            }

            match item {
                MeasureItem::Text {
                    text,
                    links,
                    highlights,
                    code_ranges,
                    chip_kind,
                } => {
                    let local_start = line_range.start.max(item_start) - item_start;
                    let local_end = line_range.end.min(item_end) - item_start;
                    if local_start < local_end {
                        let subtext = SharedString::from(text[local_start..local_end].to_string());
                        let highlights =
                            slice_ranges(highlights, local_start, local_end, |range, style| {
                                (range, *style)
                            });
                        let links = slice_ranges(links, local_start, local_end, |range, link| {
                            (range, link.clone())
                        });
                        let code = clip_code_ranges(code_ranges, local_start, local_end);
                        let runs = super::inline::styled_runs(
                            &subtext,
                            text_style,
                            &highlights,
                            &code,
                            reading_style.inline_code_font.as_ref(),
                            reading_style.inline_code_fallbacks.as_ref(),
                        );
                        let ChipMetrics {
                            font_size: chip_size,
                            padding_x: pad_x,
                            padding_y: pad_y,
                            line_height: chip_height,
                        } = chip_metrics(*chip_kind, reading_style, font_size, line_height);
                        let shaped_line = shape_line(subtext.clone(), chip_size, &runs, window);
                        let width = shaped_line.width() + pad_x * 2.;
                        actual_line_height = actual_line_height.max(chip_height + pad_y * 2.);
                        line_width += width;
                        line_fragments.push(LineFragmentLayout {
                            item_ix,
                            kind: LineFragmentKind::Text {
                                text: subtext,
                                links,
                                highlights,
                            },
                            size: size(width, chip_height + pad_y * 2.),
                            source_range: local_start..local_end,
                        });
                    }
                }
                MeasureItem::Image { .. } => {
                    if line_range.start <= item_start && item_end <= line_range.end {
                        let size = image_sizes[item_ix]
                            .expect("image size should be measured before layout");
                        line_width += size.width;
                        actual_line_height = actual_line_height.max(size.height);
                        line_fragments.push(LineFragmentLayout {
                            item_ix,
                            kind: LineFragmentKind::Image,
                            size,
                            source_range: 0..IMAGE_LEN,
                        });
                    }
                }
            }

            item_start = item_end;
        }

        let remaining = wrap_width
            .map(|width| (width - line_width).max(Pixels::ZERO))
            .unwrap_or_default();
        let mut x = match text_style.text_align {
            gpui::TextAlign::Center => remaining / 2.,
            gpui::TextAlign::Right => remaining,
            _ => Pixels::ZERO,
        };
        for fragment in line_fragments {
            let origin = point(x, y + (actual_line_height - fragment.size.height) / 2.);
            let positioned = match fragment.kind {
                LineFragmentKind::Text {
                    text,
                    links,
                    highlights,
                } => PositionedFragment::Text {
                    item_ix: fragment.item_ix,
                    origin,
                    size: fragment.size,
                    source_range: fragment.source_range,
                    text,
                    links,
                    highlights,
                },
                LineFragmentKind::Image => PositionedFragment::Image {
                    item_ix: fragment.item_ix,
                    origin,
                    size: fragment.size,
                },
            };
            x += fragment.size.width;
            fragments.push(positioned);
        }

        max_width = max_width.max(line_width);
        y += actual_line_height;
    }

    InlineFlowLayout {
        fragments,
        size: size(
            if text_style.text_align != gpui::TextAlign::Left {
                wrap_width.unwrap_or(max_width)
            } else {
                max_width
            },
            y,
        ),
    }
}

fn line_ranges(
    items: &[MeasureItem],
    image_sizes: &[Option<Size<Pixels>>],
    text_style: &TextStyle,
    wrap_width: Option<Pixels>,
    reading_style: &super::TextViewStyle,
    window: &mut Window,
) -> Vec<Range<usize>> {
    if items.iter().any(|item| {
        matches!(
            item,
            MeasureItem::Text {
                chip_kind: Some(ChipKind::Code),
                ..
            }
        )
    }) {
        return code_line_ranges(
            items,
            image_sizes,
            text_style,
            wrap_width,
            reading_style,
            window,
        );
    }
    let rem_size = window.rem_size();
    let font_size = text_style.font_size.to_pixels(rem_size);
    let mut wrapper = window
        .text_system()
        .line_wrapper(text_style.font(), font_size);
    let mut ranges = Vec::new();
    let mut append_line =
        |fragments: &[WrapLineFragment<'_>], line_start: usize, line_len: usize| {
            let line_end = line_start + line_len;
            let Some(width) = wrap_width.filter(|_| line_len > 0) else {
                ranges.push(line_start..line_end);
                return;
            };
            let mut start = line_start;
            for boundary in wrapper.wrap_line(fragments, width) {
                let end = line_start + boundary.ix.min(line_len);
                if start < end {
                    ranges.push(start..end);
                }
                start = end;
            }
            if start < line_end {
                ranges.push(start..line_end);
            }
        };

    // A hard break starts a fresh wrapping line even when an image shares the
    // paragraph. Keep offsets in source bytes; never send newlines to shaping.
    let mut fragments = Vec::new();
    let (mut start, mut len) = (0, 0);
    for (ix, item) in items.iter().enumerate() {
        match item {
            MeasureItem::Text {
                text,
                chip_kind: Some(keyboard),
                code_ranges,
                highlights,
                ..
            } => {
                let ChipMetrics {
                    font_size: chip_size,
                    padding_x: padding,
                    ..
                } = chip_metrics(
                    Some(*keyboard),
                    reading_style,
                    font_size,
                    text_style.line_height_in_pixels(window.rem_size()),
                );
                let runs = super::inline::styled_runs(
                    text,
                    text_style,
                    highlights,
                    code_ranges,
                    reading_style.inline_code_font.as_ref(),
                    reading_style.inline_code_fallbacks.as_ref(),
                );
                let width =
                    shape_line(text.clone(), chip_size, &runs, window).width() + padding * 2.;
                fragments.push(WrapLineFragment::element(width, text.len()));
                len += text.len();
            }
            MeasureItem::Text { text, .. } => {
                for part in text.split_inclusive('\n') {
                    let content = part.strip_suffix('\n').unwrap_or(part);
                    if !content.is_empty() {
                        fragments.push(WrapLineFragment::text(content));
                    }
                    len += content.len();
                    if part.ends_with('\n') {
                        append_line(&fragments, start, len);
                        start += len + 1;
                        len = 0;
                        fragments.clear();
                    }
                }
            }
            MeasureItem::Image { .. } => {
                fragments.push(WrapLineFragment::element(
                    image_sizes[ix]
                        .expect("image size should be measured before wrapping")
                        .width,
                    IMAGE_LEN,
                ));
                len += IMAGE_LEN;
            }
        }
    }
    append_line(&fragments, start, len);
    ranges
}

/// Rebuild the first remaining line after every break so a continuing code
/// span reserves padding exactly once on each painted line. Keyboard keys stay
/// atomic; code words and oversized words retain their source-byte boundaries.
fn code_line_ranges(
    items: &[MeasureItem],
    image_sizes: &[Option<Size<Pixels>>],
    text_style: &TextStyle,
    wrap_width: Option<Pixels>,
    reading_style: &super::TextViewStyle,
    window: &mut Window,
) -> Vec<Range<usize>> {
    let font_size = text_style.font_size.to_pixels(window.rem_size());
    let mut wrapper = window
        .text_system()
        .line_wrapper(text_style.font(), font_size);
    let shaped_chips = items
        .iter()
        .map(|item| match item {
            MeasureItem::Text {
                text,
                highlights,
                code_ranges,
                chip_kind: Some(kind),
                ..
            } => {
                let metrics = chip_metrics(
                    Some(*kind),
                    reading_style,
                    font_size,
                    text_style.line_height_in_pixels(window.rem_size()),
                );
                let runs = super::inline::styled_runs(
                    text,
                    text_style,
                    highlights,
                    code_ranges,
                    reading_style.inline_code_font.as_ref(),
                    reading_style.inline_code_fallbacks.as_ref(),
                );
                Some(shape_line(text.clone(), metrics.font_size, &runs, window))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let source = items
        .iter()
        .map(|item| match item {
            MeasureItem::Text { text, .. } => text.as_ref(),
            MeasureItem::Image { .. } => "\0",
        })
        .collect::<String>();
    let mut ranges = Vec::new();
    let mut hard_start = 0;
    for hard_line in source.split_inclusive('\n') {
        let hard_end = hard_start + hard_line.trim_end_matches('\n').len();
        let mut start = hard_start;
        if start == hard_end {
            ranges.push(start..hard_end);
        }
        while start < hard_end {
            let Some(width) = wrap_width else {
                ranges.push(start..hard_end);
                break;
            };
            let mut fragments = Vec::new();
            let mut item_start = 0;
            for (index, item) in items.iter().enumerate() {
                let item_end = item_start + item.len();
                let from = start.max(item_start);
                let to = hard_end.min(item_end);
                if from < to {
                    match item {
                        MeasureItem::Image { .. } => fragments.push(WrapLineFragment::element(
                            image_sizes[index].unwrap().width,
                            IMAGE_LEN,
                        )),
                        MeasureItem::Text {
                            text, chip_kind, ..
                        } => {
                            let lo = from - item_start;
                            let hi = to - item_start;
                            let part = &text[lo..hi];
                            if let Some(kind) = chip_kind {
                                let metrics = chip_metrics(
                                    Some(*kind),
                                    reading_style,
                                    font_size,
                                    text_style.line_height_in_pixels(window.rem_size()),
                                );
                                let shaped =
                                    shaped_chips[index].as_ref().expect("chip was measured");
                                if *kind == ChipKind::Keyboard {
                                    fragments.push(WrapLineFragment::element(
                                        shaped.x_for_index(hi) - shaped.x_for_index(lo)
                                            + metrics.padding_x * 2.,
                                        part.len(),
                                    ));
                                } else {
                                    let mut offset = lo;
                                    let mut padding = metrics.padding_x * 2.;
                                    let mut measured = px(0.);
                                    let mut tokens = 0;
                                    'words: for word in part.split_inclusive(char::is_whitespace) {
                                        let end = offset + word.len();
                                        let word_width =
                                            shaped.x_for_index(end) - shaped.x_for_index(offset);
                                        if word_width + metrics.padding_x * 2. > width {
                                            for (ix, character) in word.char_indices() {
                                                let ix = offset + ix;
                                                let glyph_width = shaped
                                                    .x_for_index(ix + character.len_utf8())
                                                    - shaped.x_for_index(ix);
                                                fragments.push(WrapLineFragment::element(
                                                    glyph_width + padding,
                                                    character.len_utf8(),
                                                ));
                                                measured += glyph_width + padding;
                                                tokens += 1;
                                                padding = px(0.);
                                                if measured > width && tokens > 1 {
                                                    break 'words;
                                                }
                                            }
                                        } else {
                                            fragments.push(WrapLineFragment::element(
                                                word_width + padding,
                                                word.len(),
                                            ));
                                            measured += word_width + padding;
                                            tokens += 1;
                                            padding = px(0.);
                                            if measured > width && tokens > 1 {
                                                break;
                                            }
                                        }
                                        offset = end;
                                    }
                                }
                            } else {
                                fragments.push(WrapLineFragment::text(part));
                            }
                        }
                    }
                }
                item_start = item_end;
                // Only the first boundary is consumed below. Avoid scanning
                // the entire remaining suffix for every line of long code.
                if wrapper.wrap_line(&fragments, width).next().is_some() {
                    break;
                }
            }
            let end = wrapper
                .wrap_line(&fragments, width)
                .next()
                .map(|boundary| start + boundary.ix)
                .unwrap_or(hard_end);
            if end <= start {
                ranges.push(start..hard_end);
                break;
            }
            ranges.push(start..end);
            start = end;
        }
        hard_start += hard_line.len();
    }
    ranges
}

#[allow(clippy::too_many_arguments)]
fn measure_image_size(
    ix: usize,
    url: &SharedUri,
    title: &str,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    line_height: Pixels,
    rem_size: Pixels,
    interactions: &super::interaction::TextViewInteractions,
    window: &mut Window,
    cx: &mut App,
) -> Size<Pixels> {
    let intrinsic_size = if width.is_some() && height.is_some() {
        None
    } else {
        intrinsic_image_size(ix, url, width, height, interactions, window, cx)
    };
    if intrinsic_size.is_none() && width.is_none() && height.is_none() {
        // Loading/error alt text needs its own width; a square image placeholder
        // would wrap badge labels one character per line.
        let alt = if title.is_empty() { "Image" } else { title };
        let text = SharedString::from(format!("[{alt}] …"));
        let style = window.text_style();
        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        return size(
            window
                .text_system()
                .shape_line(text, style.font_size.to_pixels(rem_size), &[run], None)
                .width,
            line_height,
        );
    }
    image_size(width, height, intrinsic_size, line_height, rem_size)
}

fn intrinsic_image_size(
    ix: usize,
    url: &SharedUri,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    interactions: &super::interaction::TextViewInteractions,
    window: &mut Window,
    cx: &mut App,
) -> Option<Size<Pixels>> {
    let super::TextViewImageSource::Ready(source) = interactions.image_source(url, window, cx)
    else {
        return None;
    };
    let mut element = img(source)
        .id(ix)
        .object_fit(ObjectFit::Contain)
        .max_w(relative(1.))
        .when_some(width, |this, width| this.w(width))
        .when_some(height, |this, height| this.h(height))
        .into_any_element();
    let measured_size = element.layout_as_root(AvailableSpace::min_size(), window, cx);

    if measured_size.width <= Pixels::ZERO || measured_size.height <= Pixels::ZERO {
        None
    } else {
        Some(measured_size)
    }
}

fn image_size(
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    intrinsic_size: Option<Size<Pixels>>,
    line_height: Pixels,
    rem_size: Pixels,
) -> Size<Pixels> {
    let base_size = AbsoluteLength::Pixels(line_height);
    match (width, height) {
        (Some(width), Some(height)) => size(
            width.to_pixels(base_size, rem_size),
            height.to_pixels(base_size, rem_size),
        ),
        (Some(width), None) => {
            let width = width.to_pixels(base_size, rem_size);
            let height = intrinsic_size
                .and_then(|intrinsic_size| {
                    (intrinsic_size.width > Pixels::ZERO && intrinsic_size.height > Pixels::ZERO)
                        .then(|| width * (intrinsic_size.height / intrinsic_size.width))
                })
                .unwrap_or(line_height);
            size(width, height)
        }
        (None, Some(height)) => {
            let height = height.to_pixels(base_size, rem_size);
            let width = intrinsic_size
                .and_then(|intrinsic_size| {
                    (intrinsic_size.width > Pixels::ZERO && intrinsic_size.height > Pixels::ZERO)
                        .then(|| height * (intrinsic_size.width / intrinsic_size.height))
                })
                .unwrap_or(height);
            size(width, height)
        }
        (None, None) => inline_image_size_for_line(intrinsic_size, line_height),
    }
}

fn inline_image_size_for_line(
    intrinsic_size: Option<Size<Pixels>>,
    line_height: Pixels,
) -> Size<Pixels> {
    let height = line_height * 0.75;
    let aspect_ratio = intrinsic_size
        .and_then(|intrinsic_size| {
            (intrinsic_size.width > Pixels::ZERO && intrinsic_size.height > Pixels::ZERO)
                .then(|| intrinsic_size.width / intrinsic_size.height)
        })
        .unwrap_or(1.);

    size((height * aspect_ratio).max(px(1.)), height.max(px(1.)))
}

fn clip_code_ranges(ranges: &[Range<usize>], start: usize, end: usize) -> Vec<Range<usize>> {
    ranges
        .iter()
        .filter_map(|range| {
            let lo = range.start.max(start);
            let hi = range.end.min(end);
            (lo < hi).then_some(lo.saturating_sub(start)..hi.saturating_sub(start))
        })
        .collect()
}

fn shape_line(
    text: SharedString,
    font_size: Pixels,
    runs: &[TextRun],
    window: &mut Window,
) -> ShapedLine {
    // Hard breaks were split before wrapping. Normalize source CR whitespace
    // without changing byte offsets used by highlights and selection.
    let text = if text.contains(['\n', '\r']) {
        SharedString::from(text.replace(['\n', '\r'], " "))
    } else {
        text
    };
    window.text_system().shape_line(text, font_size, runs, None)
}

fn slice_ranges<T, U>(
    ranges: &[(Range<usize>, T)],
    start: usize,
    end: usize,
    map: impl Fn(Range<usize>, &T) -> U,
) -> Vec<U> {
    ranges
        .iter()
        .filter_map(|(range, value)| {
            let clipped_start = range.start.max(start);
            let clipped_end = range.end.min(end);
            (clipped_start < clipped_end)
                .then(|| map((clipped_start - start)..(clipped_end - start), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_image_without_explicit_size_scales_intrinsic_ratio_to_line_height() {
        let line_height = px(20.);
        let intrinsic_size = size(px(160.), px(40.));

        let measured = inline_image_size_for_line(Some(intrinsic_size), line_height);

        assert_eq!(measured, size(px(60.), px(15.)));
    }

    #[test]
    fn inline_image_without_intrinsic_size_uses_compact_square_fallback() {
        let measured = inline_image_size_for_line(None, px(20.));

        assert_eq!(measured, size(px(15.), px(15.)));
    }

    #[gpui::test]
    fn image_paragraph_hard_breaks_start_new_lines(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let text = |value: &str| MeasureItem::Text {
                text: value.to_owned().into(),
                links: Vec::new(),
                highlights: Vec::new(),
                code_ranges: Vec::new(),
                chip_kind: None,
            };
            let items = vec![
                text("before\n"),
                MeasureItem::Image {
                    url: "icon.svg".into(),
                    title: "icon".into(),
                    width: None,
                    height: None,
                },
                text("after\n\nlast"),
            ];
            let image_sizes = vec![None, Some(size(px(18.), px(18.))), None];
            let style = window.text_style();
            for width in [None, Some(px(600.))] {
                let layout = layout_flow(
                    &items,
                    &image_sizes,
                    &style,
                    width,
                    &Default::default(),
                    window,
                );
                let texts: Vec<_> = layout
                    .fragments
                    .iter()
                    .filter_map(|fragment| match fragment {
                        PositionedFragment::Text { text, origin, .. } => {
                            Some((text.as_ref(), origin.y))
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    texts.iter().map(|(text, _)| *text).collect::<Vec<_>>(),
                    ["before", "after", "last"]
                );
                assert!(texts[1].1 > texts[0].1);
                assert!(texts[2].1 >= texts[1].1 + window.line_height() * 2.);
            }
        });
    }
}

struct ChipMetrics {
    font_size: Pixels,
    padding_x: Pixels,
    padding_y: Pixels,
    line_height: Pixels,
}

fn chip_metrics(
    kind: Option<ChipKind>,
    style: &super::TextViewStyle,
    font_size: Pixels,
    line_height: Pixels,
) -> ChipMetrics {
    if kind == Some(ChipKind::Keyboard) {
        if let Some(kbd) = &style.keyboard {
            return ChipMetrics {
                font_size: kbd.font_size,
                padding_x: kbd.padding,
                padding_y: kbd.padding,
                line_height: kbd.line_height,
            };
        }
    }
    if kind.is_some() {
        if let Some(code) = &style.inline_code {
            return ChipMetrics {
                font_size: code.font_size,
                padding_x: code.padding_x,
                padding_y: code.padding_y,
                line_height: line_height * (f32::from(code.font_size) / f32::from(font_size)),
            };
        }
    }
    ChipMetrics {
        font_size,
        padding_x: px(0.),
        padding_y: px(0.),
        line_height,
    }
}
