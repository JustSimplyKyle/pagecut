use std::convert::identity;

use vse_ui::iced::{Rectangle, Size};

pub const A4_WIDTH_MM: f64 = 210.0;
pub const A4_HEIGHT_MM: f64 = 297.0;
pub const MARGIN_MM: f64 = 10.0;

pub fn a4_height(width: u32) -> u32 {
    (f64::from(width) * (A4_HEIGHT_MM - 2.0 * MARGIN_MM) / (A4_WIDTH_MM - 2.0 * MARGIN_MM))
        .floor()
        .max(1.0) as u32
}

/// A local crop rectangle and its vertical offset in the source image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slice {
    pub rectangle: Rectangle<u32>,
    pub y_offset: u32,
}

impl Slice {
    pub fn region(self) -> Rectangle<u32> {
        Rectangle {
            y: self.rectangle.y + self.y_offset,
            ..self.rectangle
        }
    }

    fn end(self) -> u32 {
        self.region().y + self.rectangle.height
    }
}

pub struct SliceEditor {
    size: Size<u32>,
    slices: Vec<Slice>,
    selected: usize,
    draft: u32,
}

impl SliceEditor {
    pub fn export_ready(&self) -> bool {
        let draft_at_end =
            try { self.slices.get(self.selected)?.end() == self.draft }.is_some_and(identity);
        self.end() == self.size.height && (self.current().is_none() || draft_at_end)
    }

    pub fn export_hint(&self) -> Option<&'static str> {
        if self.end() != self.size.height {
            Some("Confirm all pages to export")
        } else if !self.export_ready() {
            Some("Save this cut to export")
        } else {
            None
        }
    }

    pub fn new(size: Size<u32>) -> Self {
        Self {
            size,
            slices: Vec::new(),
            selected: 0,
            draft: a4_height(size.width).min(size.height),
        }
    }

    pub fn current(&self) -> Option<Rectangle<u32>> {
        let top = self.start();
        (top < self.size.height).then(|| self.slice(top, self.size.height).region())
    }

    pub fn pages(&self) -> impl Iterator<Item = (usize, Slice)> + '_ {
        self.slices.iter().copied().enumerate()
    }

    pub fn pending(&self) -> Option<(usize, Rectangle<u32>)> {
        let top = self.end();
        (top < self.size.height).then(|| {
            (
                self.slices.len(),
                self.slice(top, self.size.height).region(),
            )
        })
    }

    pub fn select(&mut self, index: usize) -> bool {
        if index > self.slices.len()
            || (index == self.slices.len() && self.end() == self.size.height)
        {
            return false;
        }
        self.selected = index;
        self.draft = self
            .slices
            .get(index)
            .map_or_else(|| self.limit(), |slice| slice.end());
        true
    }

    pub fn move_guide(&mut self, end: u32) {
        let (min, max) = self.draft_range();
        self.draft = end.clamp(min, max);
    }

    pub fn nudge(&mut self, delta: i32) {
        self.move_guide(self.draft.saturating_add_signed(delta));
    }

    pub fn confirm(&mut self) -> bool {
        let (min, max) = self.draft_range();
        if self.current().is_none() || !(min..=max).contains(&self.draft) {
            return false;
        }
        let slice = self.slice(self.start(), self.draft);
        if self.selected < self.slices.len() {
            self.slices[self.selected] = slice;
            if let Some(following) = self.slices.get_mut(self.selected + 1) {
                let end = following.end();
                following.y_offset = self.draft;
                following.rectangle.height = end - self.draft;
            }
        } else {
            self.slices.push(slice);
        }
        self.selected += 1;
        self.draft = self
            .slices
            .get(self.selected)
            .map(|slice| slice.end())
            .unwrap_or(self.limit());
        true
    }

    pub fn limit(&self) -> u32 {
        self.start()
            .saturating_add(a4_height(self.size.width))
            .min(self.size.height)
    }

    pub fn start(&self) -> u32 {
        self.slices
            .get(self.selected)
            .map_or_else(|| self.end(), |slice| slice.y_offset)
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn draft(&self) -> u32 {
        self.draft
    }

    pub fn page_label(&self) -> String {
        format!("Page {} · cut at Y={}", self.selected + 1, self.draft)
    }

    pub fn confirm_label(&self) -> &'static str {
        if self.selected < self.slices.len() {
            "Save cut"
        } else if self.draft == self.size.height {
            "Finish last page"
        } else {
            "Confirm cut"
        }
    }

    pub fn completion_label(&self) -> String {
        format!("Split into {} pages", self.slices.len())
    }

    pub fn restart(&mut self) {
        *self = Self::new(self.size);
    }

    fn end(&self) -> u32 {
        self.slices.last().map_or(0, |slice| slice.end())
    }

    fn draft_range(&self) -> (u32, u32) {
        let mut min = self.start().saturating_add(1).min(self.size.height);
        let mut max = self.limit();
        if let Some(following) = self.slices.get(self.selected + 1) {
            min = min.max(following.end().saturating_sub(a4_height(self.size.width)));
            max = max.min(following.end().saturating_sub(1));
        }
        (min, max)
    }

    fn slice(&self, top: u32, end: u32) -> Slice {
        Slice {
            rectangle: Rectangle {
                x: 0,
                y: 0,
                width: self.size.width,
                height: end - top,
            },
            y_offset: top,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_requires_complete_pages_and_a_saved_guide() {
        let mut slices = SliceEditor::new(Size::new(190, 400));
        assert!(!slices.export_ready());
        slices.move_guide(200);
        slices.confirm();
        assert!(!slices.export_ready());
        slices.confirm();
        assert!(slices.export_ready());
        slices.select(0);
        assert!(slices.export_ready());
        slices.move_guide(210);
        assert!(!slices.export_ready());
        slices.confirm();
        assert!(slices.export_ready());
    }

    #[test]
    fn dragging_is_only_a_draft_and_confirmation_advances() {
        let mut slices = SliceEditor::new(Size::new(190, 600));
        slices.move_guide(200);
        assert!(slices.slices.is_empty());
        assert!(slices.confirm());
        assert_eq!(slices.current().unwrap().y, 200);
        assert_eq!(slices.current().unwrap().height, 400);
        slices.move_guide(477);
        assert!(slices.confirm());
        assert!(slices.confirm());
        assert!(slices.current().is_none());
        assert_eq!(
            slices
                .pages()
                .map(|(_, p)| (p.y_offset, p.rectangle.height))
                .collect::<Vec<_>>(),
            vec![(0, 200), (200, 277), (477, 123)]
        );
        assert!(!slices.confirm());
    }

    #[test]
    fn revising_an_earlier_cut_preserves_later_pages() {
        let mut slices = SliceEditor::new(Size::new(190, 700));
        slices.move_guide(200);
        slices.confirm();
        slices.move_guide(400);
        slices.confirm();
        slices.move_guide(600);
        slices.confirm();
        slices.select(0);
        slices.move_guide(250);
        slices.confirm();
        assert_eq!(
            slices.pages().map(|(_, p)| p.y_offset).collect::<Vec<_>>(),
            vec![0, 250, 400]
        );
        assert_eq!(
            slices
                .pages()
                .map(|(_, p)| p.rectangle.height)
                .collect::<Vec<_>>(),
            vec![250, 150, 200]
        );
    }

    #[test]
    fn guide_is_clamped_to_both_neighboring_a4_limits() {
        let mut slices = SliceEditor::new(Size::new(190, 700));
        slices.confirm();
        slices.confirm();
        slices.select(0);
        slices.move_guide(1);
        assert_eq!(slices.draft(), 277);
        slices.move_guide(600);
        assert_eq!(slices.draft(), 277);
        assert!(!slices.select(99));
        slices.restart();
        assert!(slices.slices.is_empty());
    }
}
