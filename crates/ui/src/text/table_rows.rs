//! Paint visible rows of fixed-width Markdown tables without adding a scroll owner.

use super::TextViewStyle;
use gpui::{
    AnyElement, App, AvailableSpace, Pixels, TextStyle, Window, canvas, point, prelude::*, px, size,
};
use std::{
    rc::Rc,
    sync::{Arc, Mutex},
};

pub(super) type RenderRow = Rc<dyn Fn(usize, &mut Window, &mut App) -> AnyElement>;

pub(super) struct RowLayout {
    width: Pixels,
    text_style: TextStyle,
    rem_size: Pixels,
    style: TextViewStyle,
    offsets: Vec<Pixels>,
}

impl std::fmt::Debug for RowLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RowLayout")
            .field("width", &self.width)
            .field("offsets", &self.offsets)
            .finish()
    }
}

pub(super) fn visible_rows(
    offsets: &[Pixels],
    top: Pixels,
    bottom: Pixels,
) -> std::ops::Range<usize> {
    let count = offsets.len().saturating_sub(1);
    let start = offsets[1..].partition_point(|end| *end <= top).min(count);
    let end = offsets[..count]
        .partition_point(|start| *start < bottom)
        .min(count);
    start..end.max(start)
}

pub(super) fn rows(
    cache: &Arc<Mutex<Option<RowLayout>>>,
    width: Pixels,
    count: usize,
    render: RenderRow,
    style: &TextViewStyle,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let text_style = window.text_style();
    let mut cache = cache.lock().unwrap();
    let valid = cache.as_ref().is_some_and(|cache| {
        cache.width == width
            && cache.text_style == text_style
            && cache.rem_size == window.rem_size()
            && cache.style == *style
    });
    if !valid {
        let mut offsets = vec![px(0.)];
        for index in 0..count {
            let height = render(index, window, cx)
                .layout_as_root(size(width.into(), AvailableSpace::MinContent), window, cx)
                .height;
            offsets.push(offsets[index] + height);
        }
        *cache = Some(RowLayout {
            width,
            text_style,
            rem_size: window.rem_size(),
            style: style.clone(),
            offsets,
        });
    }
    let offsets = cache.as_ref().unwrap().offsets.clone();
    let height = *offsets.last().unwrap();
    canvas(
        move |bounds, window, cx| {
            let mask = window.content_mask().bounds;
            let range = visible_rows(
                &offsets,
                mask.top() - bounds.top(),
                mask.bottom() - bounds.top(),
            );
            range
                .map(|index| {
                    let mut element = render(index, window, cx);
                    element.layout_as_root(
                        size(
                            bounds.size.width.into(),
                            AvailableSpace::Definite(offsets[index + 1] - offsets[index]),
                        ),
                        window,
                        cx,
                    );
                    element.prepaint_at(
                        point(bounds.left(), bounds.top() + offsets[index]),
                        window,
                        cx,
                    );
                    element
                })
                .collect::<Vec<_>>()
        },
        |_, elements, window, cx| {
            for element in elements {
                element.paint(window, cx);
            }
        },
    )
    .w_full()
    .h(height)
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_rows_respects_variable_heights_and_exact_edges() {
        let offsets = [px(0.), px(24.), px(48.), px(96.)];
        assert_eq!(visible_rows(&offsets, px(24.), px(48.)), 1..2);
        assert_eq!(visible_rows(&offsets, px(23.), px(49.)), 0..3);
        assert_eq!(visible_rows(&offsets, px(120.), px(144.)), 3..3);
        assert_eq!(visible_rows(&offsets, px(-30.), px(-1.)), 0..0);
    }
}
