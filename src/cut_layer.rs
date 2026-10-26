//! Page cuts over a long document, and a layer for editing them with the mouse.
use std::cell::Cell;
use std::ops::Deref;
use std::range::Range;
use std::rc::Rc;

use vse_ui::iced::{self, Color, Fill};
use vse_ui::{Apply, Element, Theme, widget};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutModel {
    document_height: u64,
    max_length: u64,
    cuts: OrderedCuts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OrderedCuts(Vec<u64>);

impl Deref for OrderedCuts {
    type Target = [u64];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl OrderedCuts {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn remove(&mut self, index: usize) -> u64 {
        self.0.remove(index)
    }

    pub fn push(&mut self, position: u64) -> bool {
        match self.0.binary_search(&position) {
            Ok(_) => false,
            Err(index) => {
                self.0.insert(index, position);
                true
            }
        }
    }

    /// Moves a cut, clamped between its neighbours. Returns where it landed.
    ///
    /// Landing on a neighbour leaves a duplicate, which the caller must remove.
    pub fn move_cut(&mut self, index: usize, position: u64) -> u64 {
        let lowest = index.checked_sub(1).map_or(0, |previous| self.0[previous]);
        let highest = self.0.get(index + 1).map_or(u64::MAX, |&next| next);
        self.0[index] = position.clamp(lowest, highest);
        self.0[index]
    }
}

/// A change the user asked for through a [`CutLayer`].
#[derive(Debug, Clone)]
pub enum CutAction {
    Introduce(u64),
    Move(usize, u64),
    Remove(usize),
}

impl CutModel {
    pub fn perform(&mut self, action: CutAction) {
        match action {
            CutAction::Introduce(position) => self.introduce_cut(position),
            CutAction::Move(index, position) => self.move_cut(index, position),
            CutAction::Remove(index) => self.remove_cut(index),
        }
    }

    /// Starts with no cuts, so the whole document is one page.
    #[must_use]
    pub fn new(document_height: u64, max_length: u64) -> Self {
        debug_assert!(max_length > 0, "pages must have a height");

        Self {
            document_height,
            max_length,
            cuts: OrderedCuts::new(),
        }
    }

    /// Starts with a cut every `page_length`, which should be at most
    /// `max_length` so the pages fit.
    #[must_use]
    pub fn paginated(document_height: u64, max_length: u64, page_length: u64) -> Self {
        debug_assert!(page_length > 0, "pages must have a height");

        let mut model = Self::new(document_height, max_length);
        for cut in (page_length..document_height).step_by(page_length as usize) {
            model.cuts.push(cut);
        }
        model
    }

    pub const fn max_length(&self) -> u64 {
        self.max_length
    }

    /// Whether every page is at most `max_length` long.
    pub fn fits(&self) -> bool {
        self.pages().all(|page| page.end - page.start <= self.max_length)
    }

    /// The index of the cut at `position`, if there is one.
    pub fn index_of(&self, position: u64) -> Option<usize> {
        self.cuts.binary_search(&position).ok()
    }

    pub fn introduce_cut(&mut self, absolute_position: u64) {
        if absolute_position == 0 || absolute_position >= self.document_height {
            return;
        }

        let index = self.cuts.partition_point(|&cut| cut < absolute_position);
        let previous = index
            .checked_sub(1)
            .map_or(0, |previous| self.cuts[previous]);
        if absolute_position - previous > self.max_length {
            return;
        }

        self.cuts.push(absolute_position);
    }

    /// Moving a cut onto its neighbour or a document edge removes it.
    pub fn move_cut(&mut self, index: usize, absolute_position: u64) {
        if index >= self.cuts.len() {
            return;
        }

        self.cuts
            .move_cut(index, absolute_position.min(self.document_height));
        self.reflow();
    }

    pub fn remove_cut(&mut self, index: usize) {
        if index >= self.cuts.len() {
            return;
        }

        self.cuts.remove(index);
        self.reflow();
    }

    pub fn pages(&self) -> impl Iterator<Item = Range<u64>> + '_ {
        std::iter::once(0)
            .chain(self.cuts.iter().copied())
            .chain([self.document_height])
            .map_windows(|&[start, end]| Range { start, end })
    }

    pub fn cuts(&self) -> &[u64] {
        &self.cuts
    }

    /// Pulls up cuts that make a page longer than `max_length`, then removes
    /// cuts that leave a page with no length.
    fn reflow(&mut self) {
        let cuts = Cell::from_mut(self.cuts.0.as_mut_slice()).as_slice_of_cells();

        if let Some(first) = cuts.first() {
            first.set(first.get().min(self.max_length));
        }
        for [top, bottom] in cuts.array_windows::<2>() {
            if bottom.get() - top.get() > self.max_length {
                bottom.set(top.get() + self.max_length);
            }
        }

        self.cuts.0.dedup();
        self.cuts
            .0
            .retain(|&cut| cut != 0 && cut != self.document_height);
    }
}

/// How close, in screen pixels, the cursor must be to grab a cut.
const GRAB_DISTANCE: f32 = 6.0;

/// The state of a single cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Active,
    Hovered,
    Dragged,
}

impl Status {
    /// Line thickness in screen pixels; part of the layout, so it can't depend
    /// on the theme like the colors in [`Style`] do.
    fn thickness(self) -> f32 {
        match self {
            Self::Active => 1.0,
            Self::Hovered | Self::Dragged => 2.0,
        }
    }
}

/// The appearance of a [`CutLayer`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// The color of a cut.
    pub cut: Color,
    /// The color of the dotted line marking the furthest allowed cut.
    pub limit: Color,
}

/// The default style of a [`CutLayer`].
pub fn default(_theme: &Theme, status: Status) -> Style {
    Style {
        cut: match status {
            Status::Active => Color::from_rgb8(220, 60, 60),
            Status::Hovered | Status::Dragged => Color::from_rgb8(250, 90, 90),
        },
        limit: Color::from_rgb8(140, 140, 140),
    }
}

/// A one pixel high (or `thickness` high) bar filling the available width.
fn solid_line<'a, Message: 'a>(
    thickness: f32,
    color: impl Fn(&Theme) -> Color + 'a,
) -> widget::Container<'a, Message, Theme> {
    widget::container(widget::Space::new())
        .width(Fill)
        .height(thickness)
        .style(move |theme| widget::container::Style {
            background: Some(color(theme).into()),
            ..widget::container::Style::default()
        })
}

/// A dotted line, one pixel high, filling the available width.
///
/// Dots and gaps share the width equally; a row of fixed-size dots wider than
/// the space it has doesn't get drawn at all.
fn dotted_line<'a, Message: 'a>(
    color: impl Fn(&Theme) -> Color + Clone + 'a,
) -> Element<'a, Message> {
    const DOTS: usize = 120;
    let dot = || solid_line(1.0, color.clone()).into();
    let gap = || widget::Space::new().width(Fill).into();
    widget::row((0..DOTS).flat_map(|_| [dot(), gap()]))
        .width(Fill)
        .into()
}

/// Draws the cuts of a [`CutModel`] on top of `content` and lets the user
/// drag (left), introduce (left on empty space) and remove (right) cuts.
///
/// `content` may be shown at any size; the layer measures how tall it is on
/// screen and scales the cuts to match.
pub fn cut_layer<'a, Message: Clone + 'static>(
    model: &'a CutModel,
    content: impl Fn() -> Element<'a, Message> + 'a,
) -> CutLayer<'a, Message> {
    CutLayer {
        model,
        content: Box::new(content),
        on_action: None,
        on_rescale: None,
        style: Rc::new(default),
    }
}

pub struct CutLayer<'a, Message> {
    model: &'a CutModel,
    // `view` can be called many times, so the content has to be rebuildable.
    content: Box<dyn Fn() -> Element<'a, Message> + 'a>,
    on_action: Option<Box<dyn Fn(CutAction) -> Message + 'a>>,
    on_rescale: Option<Box<dyn Fn(f32) -> Message + 'a>>,
    // Shared with the style closure of every line drawn.
    style: Rc<dyn Fn(&Theme, Status) -> Style + 'a>,
}

impl<'a, Message> CutLayer<'a, Message> {
    #[must_use]
    pub fn on_action(mut self, callback: impl Fn(CutAction) -> Message + 'a) -> Self {
        self.on_action = Some(Box::new(callback));
        self
    }

    /// Reports the screen pixels per document pixel whenever it changes.
    #[must_use]
    pub fn on_rescale(mut self, callback: impl Fn(f32) -> Message + 'a) -> Self {
        self.on_rescale = Some(Box::new(callback));
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self {
        self.style = Rc::new(style);
        self
    }

    fn action(&self, action: CutAction) -> Option<Message> {
        self.on_action.as_ref().map(|on_action| on_action(action))
    }

    fn cut_near(&self, state: &CutLayerState) -> Option<usize> {
        let grab_distance = (GRAB_DISTANCE / state.scale) as u64;
        self.model
            .cuts
            .iter()
            .position(|&cut| cut.abs_diff(state.cursor) <= grab_distance)
    }

    fn status(&self, state: &CutLayerState, index: usize) -> Status {
        if state.dragging.map(|drag| drag.index) == Some(index) {
            Status::Dragged
        } else if state.dragging.is_none() && self.cut_near(state) == Some(index) {
            Status::Hovered
        } else {
            Status::Active
        }
    }
}

#[derive(Default)]
pub struct CutLayerState {
    /// Screen pixels per document pixel; `0.0` until the content is measured.
    scale: f32,
    /// Cursor position in document pixels.
    cursor: u64,
    dragging: Option<Drag>,
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    index: usize,
    /// How many cuts there were when the drag started. Dropping the cut onto a
    /// neighbour removes it, which shifts the indices, so the drag must end.
    cuts: usize,
}

#[derive(Clone)]
pub enum CutLayerEvent<Message> {
    Content(Message),
    Rescaled(f32),
    CursorMoved(u64),
    Pressed,
    RightPressed,
    Released,
}

/// A horizontal line drawn by the layer, in document pixels.
enum Mark {
    Cut(usize),
    Limit,
}

impl<'a, Message: Clone + 'static> widget::Component<'a, Message, Theme> for CutLayer<'a, Message> {
    type State = CutLayerState;
    type Event = CutLayerEvent<Message>;

    /// Measures the scale and translates the cursor into document pixels, both
    /// of which need the bounds that only `listen` gets.
    fn listen(
        &self,
        state: &CutLayerState,
        event: &iced::Event,
        bounds: iced::Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> widget::Action<Self::Event> {
        let scale = bounds.height / self.model.document_height as f32;
        if scale != state.scale {
            return widget::Action::publish(CutLayerEvent::Rescaled(scale));
        }

        // The `mouse_area` handles presses, including touches; this tracks
        // where they land.
        let position = match event {
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { .. }) => cursor.position(),
            iced::Event::Touch(
                iced::touch::Event::FingerPressed { position, .. }
                | iced::touch::Event::FingerMoved { position, .. },
            ) => Some(*position),
            _ => None,
        };
        let Some(position) = position else {
            return widget::Action::none();
        };

        let cursor = ((position.y - bounds.y) / scale).max(0.0) as u64;
        widget::Action::publish(CutLayerEvent::CursorMoved(cursor))
    }

    fn update(
        &self,
        state: &mut CutLayerState,
        event: Self::Event,
        _renderer: &iced::Renderer,
    ) -> Option<Message> {
        match event {
            CutLayerEvent::Content(message) => Some(message),
            CutLayerEvent::Rescaled(scale) => {
                state.scale = scale;
                self.on_rescale.as_ref().map(|on_rescale| on_rescale(scale))
            }
            CutLayerEvent::CursorMoved(cursor) => {
                state.cursor = cursor;
                let drag = state.dragging?;
                if self.model.cuts.len() != drag.cuts {
                    state.dragging = None;
                    return None;
                }
                self.action(CutAction::Move(drag.index, cursor))
            }
            CutLayerEvent::Pressed => match self.cut_near(state) {
                Some(index) => {
                    state.dragging = Some(Drag {
                        index,
                        cuts: self.model.cuts.len(),
                    });
                    None
                }
                None => self.action(CutAction::Introduce(state.cursor)),
            },
            CutLayerEvent::RightPressed => {
                let index = self.cut_near(state)?;
                self.action(CutAction::Remove(index))
            }
            CutLayerEvent::Released => {
                state.dragging = None;
                None
            }
        }
    }

    fn view(&self, state: &CutLayerState) -> Element<'a, Self::Event> {
        let max_length = self.model.max_length;

        // Every cut, plus a limit inside each page that's over `max_length`
        // showing where the furthest allowed cut is, from top to bottom.
        let mut marks = Vec::new();
        for (index, page) in self.model.pages().enumerate() {
            if page.end - page.start > max_length {
                marks.push((page.start + max_length, Mark::Limit));
            }
            if index < self.model.cuts.len() {
                marks.push((page.end, Mark::Cut(index)));
            }
        }

        // Stack the lines with spaces in between, centering each on its mark.
        let mut lines = Vec::new();
        let mut top = 0.0;
        for (position, mark) in marks {
            let (line, thickness) = match mark {
                Mark::Cut(index) => {
                    let status = self.status(state, index);
                    let style = self.style.clone();
                    let line = solid_line(status.thickness(), move |theme| style(theme, status).cut);
                    (line.into(), status.thickness())
                }
                Mark::Limit => {
                    let style = self.style.clone();
                    let line = dotted_line(move |theme| style(theme, Status::Active).limit);
                    (line, 1.0)
                }
            };
            let line_top = position as f32 * state.scale - thickness / 2.0;
            lines.push(widget::Space::new().height((line_top - top).max(0.0)).into());
            lines.push(line);
            top = line_top + thickness;
        }
        let lines = widget::column(lines).width(Fill).height(Fill);

        let interaction = if state.dragging.is_some() || self.cut_near(state).is_some() {
            iced::mouse::Interaction::ResizingVertically
        } else {
            iced::mouse::Interaction::None
        };

        widget::stack![(self.content)().map(CutLayerEvent::Content), lines]
            .apply(widget::mouse_area)
            .on_press(CutLayerEvent::Pressed)
            .on_right_press(CutLayerEvent::RightPressed)
            .on_release(CutLayerEvent::Released)
            .interaction(interaction)
            .into()
    }
}

impl<'a, Message: Clone + 'static> From<CutLayer<'a, Message>> for Element<'a, Message> {
    fn from(layer: CutLayer<'a, Message>) -> Self {
        widget::component(layer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paginated_pages_cover_the_document_and_fit() {
        let model = CutModel::paginated(1000, 300, 280);
        assert_eq!(model.cuts(), [280, 560, 840]);
        assert!(model.fits());
        assert_eq!(model.pages().last().unwrap().end, 1000);
    }

    #[test]
    fn introducing_a_cut_past_the_limit_is_ignored() {
        let mut model = CutModel::new(1000, 300);
        assert!(!model.fits());
        model.perform(CutAction::Introduce(301));
        assert!(model.cuts().is_empty());
        model.perform(CutAction::Introduce(300));
        assert_eq!(model.cuts(), [300]);
    }

    #[test]
    fn moving_a_cut_up_pulls_the_following_cuts_and_down_is_blocked() {
        let mut model = CutModel::paginated(1000, 300, 280);
        model.perform(CutAction::Move(0, 100));
        assert_eq!(model.cuts(), [100, 400, 700]);
        // 100..500 would be longer than a page, so the cut stops at 400.
        model.perform(CutAction::Move(1, 500));
        assert_eq!(model.cuts(), [100, 400, 700]);
    }

    #[test]
    fn moving_the_first_cut_to_the_top_removes_its_empty_page() {
        let mut model = CutModel::paginated(1000, 300, 280);
        model.perform(CutAction::Move(0, 0));
        // Like removing it: the cuts after it are pulled up first.
        assert_eq!(model.cuts(), [300, 600]);
        assert!(model.pages().all(|page| page.end > page.start));
    }

    #[test]
    fn moving_a_cut_onto_its_neighbour_or_the_bottom_removes_it() {
        let mut model = CutModel::paginated(1000, 300, 280);
        model.perform(CutAction::Move(1, 100));
        assert_eq!(model.cuts(), [280, 580]);
        assert!(model.pages().all(|page| page.end > page.start));

        // The page above may be up to 500 long, so the last cut can reach the bottom.
        let mut model = CutModel::paginated(1000, 500, 280);
        model.perform(CutAction::Move(2, 1000));
        assert_eq!(model.cuts(), [280, 560]);
    }

    #[test]
    fn actions_on_cuts_that_no_longer_exist_are_ignored() {
        let mut model = CutModel::paginated(1000, 300, 280);
        model.perform(CutAction::Move(3, 900));
        model.perform(CutAction::Remove(3));
        assert_eq!(model.cuts(), [280, 560, 840]);
    }

    #[test]
    fn removing_a_cut_pulls_up_the_cuts_after_it() {
        let mut model = CutModel::paginated(1000, 300, 200);
        model.perform(CutAction::Remove(0));
        assert_eq!(model.cuts(), [300, 600, 800]);
    }
}
