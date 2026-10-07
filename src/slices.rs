use crate::layout::PageSettings;
use std::convert::identity;

use vse_ui::iced::{Rectangle, Size};

const MAX_ADJUSTMENT_RESERVE: u32 = 80;
const ADJUSTMENT_RESERVE_FRACTION: u32 = 20;

pub fn a4_height(width: u32) -> u32 {
    PageSettings::default().height(width)
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

#[derive(Clone)]
pub struct SliceEditor {
    size: Size<u32>,
    slices: Vec<Slice>,
    selected: usize,
    draft: u32,
    pub settings: PageSettings,
}

impl SliceEditor {
    pub fn export_ready(&self) -> bool {
        let draft_at_end =
            try { self.slices.get(self.selected)?.end() == self.draft }.is_some_and(identity);
        self.end() == self.size.height && (self.current().is_none() || draft_at_end)
    }

    pub fn new(size: Size<u32>) -> Self {
        Self {
            size,
            slices: Vec::new(),
            selected: 0,
            draft: a4_height(size.width).min(size.height),
            settings: PageSettings::default(),
        }
    }

    pub fn prepared(size: Size<u32>, settings: PageSettings) -> Self {
        let mut editor = Self::new(size);
        editor.settings = settings;
        while editor.current().is_some() {
            editor.draft = editor.suggested_cut();
            editor.confirm();
        }
        editor.select(0);
        editor
    }

    fn suggested_cut(&self) -> u32 {
        if self.limit() == self.size.height {
            return self.size.height;
        }
        // Leave room to adjust adjacent pages when an earlier cut is moved.
        let reserve =
            (self.page_height() / ADJUSTMENT_RESERVE_FRACTION).min(MAX_ADJUSTMENT_RESERVE);
        self.limit().saturating_sub(reserve)
    }

    pub fn cut_label(&self) -> String {
        format!("Page {} ends here", self.selected + 1)
    }

    pub fn count_label(&self) -> String {
        format!("{} pages", self.count())
    }
    pub fn count(&self) -> usize {
        self.slices.len() + usize::from(self.pending().is_some())
    }
    pub fn selected_label(&self) -> String {
        format!("Page {}", self.selected + 1)
    }
    pub fn position_label(&self) -> String {
        format!("{} px", self.draft)
    }
    pub fn progress_label(&self) -> String {
        format!("Page {} of {}", self.selected + 1, self.count())
    }
    pub fn export_label(&self) -> String {
        format!("All {} pages will be included in the PDF.", self.count())
    }
    pub fn reset_cut(&mut self) {
        self.select(self.selected);
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

    pub fn can_select(&self, index: usize) -> bool {
        index < self.count()
    }

    pub fn has_pending_cut(&self) -> bool {
        self.current().is_some()
            && self
                .slices
                .get(self.selected)
                .is_none_or(|page| page.end() != self.draft)
    }

    pub fn select(&mut self, index: usize) -> bool {
        if !self.can_select(index) {
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
        let (min, max) = self.draft_range();
        self.draft = self.draft.saturating_add_signed(delta).clamp(min, max);
    }

    pub fn confirm(&mut self) -> bool {
        let (min, max) = self.draft_range();
        if self.current().is_none() || !(min..=max).contains(&self.draft) {
            return false;
        }
        let slice = self.slice(self.start(), self.draft);
        if self.selected < self.slices.len() {
            self.slices[self.selected] = slice;
            self.reflow_following();
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

    /// Preserve later ends where they fit, moving overflowing boundaries earlier.
    fn reflow_following(&mut self) {
        let covered_end = self.end();
        let mut top = self.draft;
        for index in self.selected + 1..self.slices.len() {
            let end = self.fitting_end(top, self.slices[index].end());
            self.slices[index] = self.slice(top, end);
            top = end;
        }
        while top < covered_end {
            let end = self.fitting_end(top, covered_end);
            self.slices.push(self.slice(top, end));
            top = end;
        }
    }

    fn fitting_end(&self, top: u32, previous_end: u32) -> u32 {
        let limit = top.saturating_add(self.page_height());
        if previous_end <= limit {
            return previous_end;
        }
        limit
    }

    pub fn page_height(&self) -> u32 {
        self.settings.height(self.size.width)
    }

    pub fn limit(&self) -> u32 {
        self.start()
            .saturating_add(self.page_height())
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

    #[cfg(test)]
    pub fn restart(&mut self) {
        *self = Self::new(self.size);
    }

    fn end(&self) -> u32 {
        self.slices.last().map_or(0, |slice| slice.end())
    }

    pub fn is_last_page(&self) -> bool {
        self.slices
            .get(self.selected)
            .is_some_and(|page| page.end() == self.size.height)
    }

    fn draft_range(&self) -> (u32, u32) {
        if self.is_last_page() {
            return (self.size.height, self.size.height);
        }
        let min = self.start().saturating_add(1).min(self.size.height);
        let mut max = self.limit();
        if let Some(following) = self.slices.get(self.selected + 1) {
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
    fn prepared_pages_cover_the_image_and_leave_room_to_adjust() {
        let mut editor = SliceEditor::prepared(Size::new(190, 700), PageSettings::default());
        assert!(editor.export_ready());
        let mut end = 0;
        for (_, page) in editor.pages() {
            assert_eq!(page.y_offset, end);
            assert!(page.rectangle.height <= editor.page_height());
            end = page.end();
        }
        assert_eq!(end, 700);
        let cut = editor.draft();
        editor.nudge(-1);
        assert_eq!(editor.draft(), cut - 1);
        assert!(!editor.export_ready());
        editor.reset_cut();
        assert_eq!(editor.draft(), cut);
        assert!(editor.export_ready());
        editor.select(editor.count() - 1);
        editor.nudge(-50);
        assert_eq!(editor.draft(), 700);
    }

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
    fn moving_the_first_cut_reflows_all_pages_and_keeps_export_ready() {
        let mut editor = SliceEditor::prepared(Size::new(190, 700), PageSettings::default());
        let count = editor.count();
        editor.move_guide(150);
        assert_eq!(editor.draft(), 150);
        assert!(!editor.export_ready());
        editor.confirm();
        assert_eq!(editor.slices[0].end(), 150);
        assert_eq!(editor.count(), count);
        assert!(editor.export_ready());
        let mut end = 0;
        for (_, page) in editor.pages() {
            assert_eq!(page.y_offset, end);
            assert!((1..=editor.page_height()).contains(&page.rectangle.height));
            end = page.end();
        }
        assert_eq!(end, 700);
    }

    #[test]
    fn reflow_adds_a_page_if_existing_pages_cannot_hold_the_image() {
        let mut editor = SliceEditor::new(Size::new(190, 554));
        editor.confirm();
        editor.confirm();
        editor.select(0);
        editor.move_guide(100);
        editor.confirm();
        assert_eq!(editor.count(), 3);
        assert_eq!(
            editor
                .slices
                .iter()
                .map(|page| page.end())
                .collect::<Vec<_>>(),
            vec![100, 377, 554]
        );
        assert!(editor.export_ready());
    }

    #[test]
    fn first_cut_can_move_when_the_following_page_is_full() {
        let mut slices = SliceEditor::new(Size::new(190, 700));
        slices.confirm();
        slices.confirm();
        slices.select(0);
        slices.move_guide(1);
        assert_eq!(slices.draft(), 1);
        slices.confirm();
        assert_eq!(slices.slices[0].end(), 1);
        assert_eq!(slices.slices[1].end(), 278);
        assert_eq!(slices.end(), 554);
        slices.select(0);
        slices.move_guide(600);
        assert_eq!(slices.draft(), 277);
        assert!(!slices.select(99));
        slices.restart();
        assert!(slices.slices.is_empty());
    }
}
