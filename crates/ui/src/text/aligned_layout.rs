use gpui::{Pixels, Point, TextAlign, TextLayout, point, px};
use std::{ops::Deref, rc::Rc};

/// GPUI paints aligned glyphs but reports unaligned TextLayout coordinates.
/// Keep selection, links and code chips in the same coordinates as the glyphs.
#[derive(Clone)]
pub(super) struct AlignedTextLayout {
    layout: TextLayout,
    offsets: Rc<Vec<(Pixels, Pixels)>>,
}

impl AlignedTextLayout {
    pub(super) fn new(layout: TextLayout, text: &str, align: TextAlign) -> Self {
        let mut offsets = Vec::new();
        if align != TextAlign::Left {
            let mut start = 0;
            for text_line in text.split('\n') {
                if let (Some(line), Some(origin)) = (
                    layout.line_layout_for_index(start),
                    layout.position_for_index(start),
                ) {
                    let mut left = px(0.);
                    for (row, right) in line
                        .wrap_boundaries
                        .iter()
                        .map(|boundary| {
                            line.unwrapped_layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix]
                                .position
                                .x
                        })
                        .chain([line.unwrapped_layout.width])
                        .enumerate()
                    {
                        let extra = layout.bounds().size.width - (right - left);
                        let offset = match align {
                            TextAlign::Center => extra / 2.,
                            TextAlign::Right => extra,
                            TextAlign::Left => px(0.),
                        };
                        offsets.push((origin.y + layout.line_height() * row, offset));
                        left = right;
                    }
                }
                start += text_line.len() + 1;
            }
        }
        Self {
            layout,
            offsets: Rc::new(offsets),
        }
    }

    fn offset_at(&self, y: Pixels) -> Pixels {
        let index = self.offsets.partition_point(|(top, _)| *top <= y);
        index
            .checked_sub(1)
            .map_or(px(0.), |index| self.offsets[index].1)
    }

    pub(super) fn position_for_index(&self, index: usize) -> Option<Point<Pixels>> {
        let mut position = self.layout.position_for_index(index)?;
        position.x += self.offset_at(position.y);
        Some(position)
    }

    pub(super) fn index_for_position(&self, position: Point<Pixels>) -> Result<usize, usize> {
        self.layout
            .index_for_position(position - point(self.offset_at(position.y), px(0.)))
    }
}

impl Deref for AlignedTextLayout {
    type Target = TextLayout;

    fn deref(&self) -> &Self::Target {
        &self.layout
    }
}
