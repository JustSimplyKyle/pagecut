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

    pub fn end(self) -> u32 {
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
        self.end() == self.size.height && (!self.has_pending_cut())
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
        let page_height = editor.page_height();
        // Leave room to adjust adjacent pages when an earlier cut is moved.
        let reserve = (page_height / ADJUSTMENT_RESERVE_FRACTION).min(MAX_ADJUSTMENT_RESERVE);
        let mut top = 0;
        while top < size.height {
            let limit = top.saturating_add(page_height).min(size.height);
            let end = if limit == size.height {
                limit
            } else {
                limit - reserve
            };
            editor.slices.push(editor.slice(top, end));
            top = end;
        }
        editor.select(0);
        editor
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
    /// The source region beginning at the selected page and extending to the image end.
    pub fn current(&self) -> Rectangle<u32> {
        let top = self
            .slices
            .get(self.selected)
            .map_or_else(|| self.end(), |slice| slice.y_offset);
        self.slice(top, self.size.height).region()
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
        self.slices
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

    /// Save the draft boundary, reflow later pages, and advance the selection if possible.
    pub fn commit_cut(&mut self) -> bool {
        let (min, max) = self.draft_range();
        if self.is_last_page() || !(min..=max).contains(&self.draft) {
            return false;
        }
        let slice = self.slice(self.current().y, self.draft);
        if self.selected < self.slices.len() {
            self.slices[self.selected] = slice;
            self.reflow_following();
        } else {
            self.slices.push(slice);
        }
        // Advance when another page exists; otherwise keep the final page selected.
        let next = (self.selected + 1).min(self.count() - 1);
        self.select(next);
        true
    }

    /// Remove consumed pages, preserve later ends where they fit, and merge the tail.
    fn reflow_following(&mut self) {
        let covered_end = self.end().max(self.draft);
        let following = self.slices.split_off(self.selected + 1);
        let mut top = self.draft;
        for page in following {
            if page.end() <= top {
                continue;
            }
            if covered_end - top <= self.page_height() {
                self.slices.push(self.slice(top, covered_end));
                return;
            }
            let end = self.fitting_end(top, page.end());
            self.slices.push(self.slice(top, end));
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
        self.current()
            .y
            .saturating_add(self.page_height())
            .min(self.size.height)
    }

    fn end(&self) -> u32 {
        self.slices.last().map_or(0, |slice| slice.end())
    }

    pub const fn selected(&self) -> usize {
        self.selected
    }

    pub const fn draft(&self) -> u32 {
        self.draft
    }

    #[cfg(test)]
    pub fn restart(&mut self) {
        *self = Self::new(self.size);
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
        let min = self.current().y.saturating_add(1).min(self.size.height);
        (min, self.limit())
    }

    const fn slice(&self, top: u32, end: u32) -> Slice {
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
        slices.commit_cut();
        assert!(!slices.export_ready());
        slices.commit_cut();
        assert!(slices.export_ready());
        slices.select(0);
        assert!(slices.export_ready());
        slices.move_guide(210);
        assert!(!slices.export_ready());
        slices.commit_cut();
        assert!(slices.export_ready());
    }

    #[test]
    fn dragging_is_only_a_draft_and_confirmation_advances() {
        let mut slices = SliceEditor::new(Size::new(190, 600));
        slices.move_guide(200);
        assert!(slices.slices.is_empty());
        assert!(slices.commit_cut());
        assert_eq!(slices.current().y, 200);
        assert_eq!(slices.current().height, 400);
        slices.move_guide(477);
        assert!(slices.commit_cut());
        assert!(slices.commit_cut());
        assert_eq!(slices.current().y, 477);
        assert_eq!(slices.current().height, 123);
        assert!(slices.can_select(slices.selected()));
        assert!(slices.export_ready());
        assert!(!slices.has_pending_cut());
        assert_eq!(
            slices
                .pages()
                .map(|(_, p)| (p.y_offset, p.rectangle.height))
                .collect::<Vec<_>>(),
            vec![(0, 200), (200, 277), (477, 123)]
        );
        assert!(!slices.commit_cut());
        assert_eq!(slices.count(), 3);
        assert_eq!(slices.selected(), 2);
        assert_eq!(slices.current(), slices.slices[2].region());
        assert!(slices.export_ready());
    }

    #[test]
    fn revising_an_earlier_cut_preserves_later_pages() {
        let mut slices = SliceEditor::new(Size::new(190, 700));
        slices.move_guide(200);
        slices.commit_cut();
        slices.move_guide(400);
        slices.commit_cut();
        slices.move_guide(600);
        slices.commit_cut();
        slices.select(0);
        slices.move_guide(250);
        slices.commit_cut();
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
        editor.commit_cut();
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
        editor.commit_cut();
        editor.commit_cut();
        editor.select(0);
        editor.move_guide(100);
        editor.commit_cut();
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
    fn moving_a_cut_back_removes_the_page_added_by_reflow() {
        let mut editor = SliceEditor::new(Size::new(190, 554));
        editor.commit_cut();
        editor.commit_cut();
        editor.select(0);
        editor.move_guide(100);
        editor.commit_cut();
        assert_eq!(editor.count(), 3);

        editor.select(0);
        editor.move_guide(277);
        editor.commit_cut();
        assert_eq!(editor.count(), 2);
        assert_eq!(
            editor
                .pages()
                .map(|(_, page)| page.end())
                .collect::<Vec<_>>(),
            vec![277, 554]
        );
        assert!(editor.export_ready());
        assert!(editor.can_select(editor.selected()));
        assert_eq!(editor.current(), editor.slices[1].region());
    }

    #[test]
    fn moving_the_cut_to_the_image_end_removes_the_trailing_page() {
        let mut editor = SliceEditor::new(Size::new(190, 530));
        editor.move_guide(260);
        editor.commit_cut();
        editor.move_guide(400);
        editor.commit_cut();
        editor.commit_cut();
        assert_eq!(editor.count(), 3);

        editor.select(1);
        editor.move_guide(530);
        assert_eq!(editor.draft(), 530);
        editor.commit_cut();
        assert_eq!(editor.count(), 2);
        assert_eq!(editor.selected(), 1);
        assert_eq!(editor.current().y, 260);
        assert_eq!(editor.current().height, 270);
        assert!(editor.export_ready());
        assert!(!editor.commit_cut());
        assert!(editor.pages().all(|(_, page)| page.rectangle.height > 0));
    }

    #[test]
    fn moving_a_cut_across_an_internal_page_removes_that_page() {
        let mut editor = SliceEditor::new(Size::new(190, 700));
        for end in [100, 150, 200, 300, 500, 700] {
            editor.move_guide(end);
            assert!(editor.commit_cut());
        }
        assert_eq!(editor.count(), 6);
        editor.select(1);
        editor.move_guide(300);
        assert_eq!(editor.draft(), 300);
        assert!(editor.commit_cut());
        assert_eq!(editor.count(), 4);
        assert_eq!(
            editor
                .pages()
                .map(|(_, page)| page.end())
                .collect::<Vec<_>>(),
            vec![100, 300, 500, 700]
        );
        assert_eq!(editor.selected(), 2);
        assert_eq!(editor.current().y, 300);
        assert!(editor.export_ready());
        let mut top = 0;
        for (_, page) in editor.pages() {
            assert_eq!(page.y_offset, top);
            assert!((1..=editor.page_height()).contains(&page.rectangle.height));
            top = page.end();
        }
        assert_eq!(top, 700);
    }

    #[test]
    fn moving_a_cut_past_saved_pages_keeps_the_remaining_region_selected() {
        let mut editor = SliceEditor::new(Size::new(190, 700));
        for end in [100, 200, 300] {
            editor.move_guide(end);
            assert!(editor.commit_cut());
        }
        editor.select(1);
        editor.move_guide(350);
        assert!(editor.commit_cut());
        assert_eq!(editor.count(), 3);
        assert_eq!(
            editor
                .pages()
                .map(|(_, page)| page.end())
                .collect::<Vec<_>>(),
            vec![100, 350]
        );
        assert_eq!(editor.selected(), 2);
        assert_eq!(editor.current().y, 350);
        assert_eq!(editor.current().height, 350);
        assert_eq!(editor.pending(), Some((2, editor.current())));
        assert!(!editor.export_ready());
    }

    #[test]
    fn first_cut_can_move_when_the_following_page_is_full() {
        let mut slices = SliceEditor::new(Size::new(190, 700));
        slices.commit_cut();
        slices.commit_cut();
        slices.select(0);
        slices.move_guide(1);
        assert_eq!(slices.draft(), 1);
        slices.commit_cut();
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
