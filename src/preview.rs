use iced::mouse::{Button as MButton, Event as MEvent};
use vse_ui::iced::{self, Color, Fill, Point, Rectangle, Size};
use vse_ui::{Apply, Element, theme, widget};

use super::{Message, metrics};
use crate::cut_layer::{self, CutAction, CutModel, cut_layer};

const BEYOND_LIMIT_PX: f32 = 100.0;
const SCROLLBAR_SPACE_PX: f32 = 16.0;
const RULE_HIT_PX: f32 = 10.0;
const SCROLL_ID: &str = "image-preview";

pub fn scroll_to(start: u32, scale: f32) -> iced::Task<Message> {
    widget::operation::scroll_to(
        SCROLL_ID,
        widget::operation::AbsoluteOffset {
            x: 0.0,
            y: start as f32 * scale,
        },
        widget::operation::Animation::Instant,
    )
}

pub fn preview<'a>(
    allocation: &'a widget::image::Allocation,
    cuts: &'a CutModel,
    zoom: f32,
    fit_width: bool,
) -> Element<'a, Message> {
    widget::responsive(move |available| {
        let size = allocation.size();
        let scale = preview_scale(size, available, cuts.max_length(), fit_width) * zoom;
        let image_size = Size::new(size.width as f32 * scale, size.height as f32 * scale);
        cut_layer(cuts, move || {
            widget::image(allocation.handle())
                .width(image_size.width)
                .height(image_size.height)
                .into()
        })
        .on_action(Message::Cut)
        .on_rescale(Message::PreviewScale)
        .style(cut_style)
        .apply(Element::from)
        .apply(widget::container)
        .center_x(Fill)
        .apply(widget::scrollable)
        .id(SCROLL_ID)
        .width(Fill)
        .height(Fill)
        .apply(widget::container)
        .style(theme::container::secondary)
    })
    .into()
}

/// Fits the width, and unless `fit_width` also a whole page with room to spare
/// below it, never enlarging the image.
fn preview_scale(source: Size<u32>, available: Size, page_height: u64, fit_width: bool) -> f32 {
    let width = (available.width - SCROLLBAR_SPACE_PX).max(1.0) / source.width.max(1) as f32;
    if fit_width {
        return width;
    }
    let height = (available.height - BEYOND_LIMIT_PX).max(1.0) / page_height.max(1) as f32;
    width.min(height).min(1.0)
}

fn cut_style(app_theme: &vse_ui::Theme, _status: cut_layer::Status) -> cut_layer::Style {
    cut_layer::Style {
        cut: app_theme.seed().primary,
        limit: app_theme.palette().secondary.base.color,
    }
}

/// A close-up of the cut at `index`, whose line can be dragged to move it.
pub fn zoom<'a>(
    allocation: &'a widget::image::Allocation,
    index: usize,
    cut: u32,
) -> Element<'a, Message> {
    widget::responsive(move |available| {
        let zoom = ZoomGeometry::new(allocation.size(), cut, available);
        let image = segment(allocation, zoom.region, zoom.size);
        let rule = widget::canvas(ZoomRule {
            index,
            y: zoom.rule_y,
            source: zoom.region,
            scale: zoom.scale,
        })
        .width(zoom.size.width)
        .height(zoom.size.height);
        widget::stack![image, rule]
            .apply(widget::container)
            .center(Fill)
    })
    .into()
}

fn segment(
    allocation: &widget::image::Allocation,
    region: Rectangle<u32>,
    size: Size,
) -> Element<'_, Message> {
    widget::image(allocation.handle())
        .crop(region)
        .content_fit(iced::ContentFit::Contain)
        .width(size.width)
        .height(size.height)
        .into()
}

struct ZoomGeometry {
    region: Rectangle<u32>,
    size: Size,
    rule_y: f32,
    scale: f32,
}

impl ZoomGeometry {
    fn new(source: Size<u32>, cut: u32, available: Size) -> Self {
        let width = source.width;
        let scale = (available.width.max(1.0) / source.width.max(1) as f32)
            .min(2.0)
            .min(available.height.max(1.0));
        let height = ((available.height / scale).floor() as u32)
            .max(1)
            .min(source.height);
        let y = cut.saturating_sub(height / 2).min(source.height - height);
        Self {
            region: Rectangle {
                x: 0,
                y,
                width,
                height,
            },
            size: Size::new(width as f32 * scale, height as f32 * scale),
            rule_y: cut.saturating_sub(y) as f32 * scale,
            scale,
        }
    }
}

#[derive(Default)]
struct DragState {
    dragging: bool,
}

struct ZoomRule {
    index: usize,
    y: f32,
    source: Rectangle<u32>,
    scale: f32,
}

impl ZoomRule {
    fn near_rule(&self, position: Point) -> bool {
        (position.y - self.y).abs() <= RULE_HIT_PX
    }

    fn drag_position(&self, position: Point) -> u32 {
        let y = (position.y / self.scale)
            .round()
            .clamp(1.0, self.source.height as f32) as u32;
        self.source.y + y
    }

    fn move_to(&self, position: u32) -> Message {
        Message::Cut(CutAction::Move(self.index, u64::from(position)))
    }
}

impl widget::canvas::Program<Message> for ZoomRule {
    type State = DragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &widget::canvas::Event,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Option<widget::canvas::Action<Message>> {
        use iced::touch::Event as TEvent;
        use widget::canvas::Action;
        use widget::canvas::Event;

        match event {
            Event::Mouse(MEvent::ButtonPressed(MButton::Left))
            | Event::Touch(TEvent::FingerPressed { .. }) => {
                let position = cursor.position_in(bounds)?;
                if self.near_rule(position) {
                    state.dragging = true;
                    return Some(Action::request_redraw().and_capture());
                }
                Some(Action::publish(self.move_to(self.drag_position(position))).and_capture())
            }
            Event::Mouse(MEvent::CursorMoved { .. }) | Event::Touch(TEvent::FingerMoved { .. })
                if state.dragging =>
            {
                let position = cursor.position_from(bounds.position())?;
                Some(Action::publish(self.move_to(self.drag_position(position))).and_capture())
            }
            Event::Mouse(MEvent::ButtonReleased(MButton::Left))
            | Event::Touch(TEvent::FingerLifted { .. })
                if state.dragging =>
            {
                state.dragging = false;
                Some(Action::request_redraw().and_capture())
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> iced::mouse::Interaction {
        if state.dragging
            || cursor
                .position_in(bounds)
                .is_some_and(|position| self.near_rule(position))
        {
            iced::mouse::Interaction::ResizingVertically
        } else if cursor.position_in(bounds).is_some() {
            iced::mouse::Interaction::Crosshair
        } else {
            iced::mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        app_theme: &vse_ui::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<widget::canvas::Geometry> {
        let mut frame = widget::canvas::Frame::new(renderer, bounds.size());
        draw_rule(
            &mut frame,
            self.y.min(bounds.height - 1.0),
            app_theme.seed().primary,
        );
        vec![frame.into_geometry()]
    }
}

fn draw_rule(frame: &mut widget::canvas::Frame, y: f32, color: Color) {
    use widget::canvas::{Path, Stroke};
    frame.stroke(
        &Path::line(Point::new(0.0, y), Point::new(frame.width(), y)),
        Stroke::default()
            .with_color(color)
            .with_width(metrics::RULE_WIDTH),
    );
    {
        let center = frame.width() / 2.0;
        frame.fill_rectangle(
            Point::new(
                center - metrics::GRIP_WIDTH / 2.0,
                y - metrics::GRIP_HEIGHT / 2.0,
            ),
            Size::new(metrics::GRIP_WIDTH, metrics::GRIP_HEIGHT),
            color,
        );
        for dx in [-metrics::GRIP_DOT_SPACING, 0.0, metrics::GRIP_DOT_SPACING] {
            frame.fill(
                &Path::circle(Point::new(center + dx, y), metrics::GRIP_DOT_RADIUS),
                theme::color(theme::COSMIC.accent.on),
            );
        }
        frame.fill(
            &Path::circle(Point::new(metrics::HANDLE_INSET, y), metrics::HANDLE_RADIUS),
            color,
        );
        frame.fill(
            &Path::circle(
                Point::new(frame.width() - metrics::HANDLE_INSET, y),
                metrics::HANDLE_RADIUS,
            ),
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use widget::canvas::Program;

    fn rule() -> ZoomRule {
        ZoomRule {
            index: 3,
            y: 100.0,
            source: Rectangle {
                x: 0,
                y: 1000,
                width: 190,
                height: 600,
            },
            scale: 0.5,
        }
    }

    #[test]
    fn initial_preview_leaves_one_hundred_pixels_beyond_a_page() {
        let available = Size::new(1000.0 + SCROLLBAR_SPACE_PX, 760.0);
        let scale = preview_scale(Size::new(2000, 12000), available, 2828, false);
        assert!((2828.0 * scale + BEYOND_LIMIT_PX - available.height).abs() < 0.001);
        let wide = preview_scale(Size::new(2000, 12000), available, 2828, true);
        assert!((wide - 0.5).abs() < 0.001);
    }

    #[test]
    fn image_clicks_move_the_cut_without_starting_a_drag() {
        let bounds = Rectangle {
            width: 95.0,
            height: 300.0,
            ..Rectangle::default()
        };
        let event = widget::canvas::Event::Mouse(MEvent::ButtonPressed(MButton::Left));
        let mut state = DragState::default();
        let action = rule()
            .update(
                &mut state,
                &event,
                bounds,
                iced::mouse::Cursor::Available(Point::new(30.0, 40.0)),
            )
            .unwrap();
        assert!(matches!(
            action.into_inner().0,
            Some(Message::Cut(CutAction::Move(3, 1080)))
        ));
        assert!(!state.dragging);
    }

    #[test]
    fn drag_uses_source_coordinates_even_when_scrolled() {
        let bounds = Rectangle {
            x: 32.0,
            y: -50.0,
            width: 95.0,
            height: 300.0,
        };
        let mut state = DragState { dragging: true };
        let position = Point::new(62.0, 25.0);
        let event = widget::canvas::Event::Mouse(MEvent::CursorMoved { position });
        let action = rule()
            .update(
                &mut state,
                &event,
                bounds,
                iced::mouse::Cursor::Available(position),
            )
            .unwrap();
        assert!(matches!(
            action.into_inner().0,
            Some(Message::Cut(CutAction::Move(3, 1150)))
        ));
        let release = widget::canvas::Event::Mouse(MEvent::ButtonReleased(MButton::Left));
        let _ = rule().update(
            &mut state,
            &release,
            bounds,
            iced::mouse::Cursor::Available(position),
        );
        assert!(!state.dragging);
    }

    #[test]
    fn detail_contains_the_entire_image_width() {
        let zoom = ZoomGeometry::new(Size::new(1000, 2000), 1000, Size::new(600.0, 150.0));
        assert_eq!(zoom.region.x, 0);
        assert_eq!(zoom.region.width, 1000);
        assert!(zoom.size.width <= 600.0);
        assert!(zoom.size.height <= 150.0);
    }

    #[test]
    fn zoom_stays_within_the_source_at_both_ends() {
        for cut in [0, 1000] {
            let zoom = ZoomGeometry::new(Size::new(190, 1000), cut, Size::new(600.0, 150.0));
            assert!(zoom.region.x + zoom.region.width <= 190);
            assert!(zoom.region.y + zoom.region.height <= 1000);
            assert!(zoom.rule_y <= zoom.size.height);
        }
    }
}
