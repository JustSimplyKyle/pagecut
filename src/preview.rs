use vse_ui::iced::{self, Color, Fill, Point, Rectangle, Size};
use vse_ui::{Apply, Element, theme, widget};

use super::{Message, metrics, slices::SliceEditor};

#[cfg(test)]
use super::slices::a4_height;

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
    slices: &'a SliceEditor,
    zoom: f32,
    fit_width: bool,
) -> Element<'a, Message> {
    let region = slices.current();
    widget::responsive(move |available| {
        let geometry =
            PreviewGeometry::configured(region, available, slices.page_height(), fit_width)
                .zoomed(zoom);
        let segments = PreviewSegments::configured(region, &geometry, slices.page_height());
        let prefix = segments.prefix.map(|(region, size)| {
            let image = segment(allocation, region, size);
            let overlay =
                widget::canvas(NonActiveRules::new(slices, region.height, geometry.scale))
                    .width(size.width)
                    .height(size.height);
            widget::stack![image, overlay]
        });
        let page = segment(allocation, segments.page, segments.page_size);
        let overlay = widget::canvas(GuideOverlay {
            source: segments.page,
            scale: geometry.scale,
            guide_y: geometry.local_y(slices.draft()),
            guide_source: slices.draft(),
            limit_y: segments.page_size.height,
        })
        .width(segments.page_size.width)
        .height(segments.page_size.height);
        let page = widget::stack![page, overlay];
        let remainder = segments
            .remainder
            .map(|(region, size)| segment(allocation, region, size));

        widget::column![prefix, page, remainder]
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

pub fn zoom<'a>(
    allocation: &'a widget::image::Allocation,
    slices: &SliceEditor,
) -> Element<'a, Message> {
    let cut = slices.draft();
    widget::responsive(move |available| {
        let zoom = ZoomGeometry::new(allocation.size(), cut, available);
        let image = segment(allocation, zoom.region, zoom.size);
        let rule = widget::canvas(ZoomRule {
            y: zoom.rule_y,
            source: zoom.region,
            scale: zoom.scale,
            cut,
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

struct PreviewGeometry {
    width: f32,
    page_height: f32,
    scale: f32,
    start: u32,
}

impl PreviewGeometry {
    #[cfg(test)]
    fn fit(source: Rectangle<u32>, available: Size) -> Self {
        Self::configured(source, available, a4_height(source.width), false)
    }
    fn configured(
        source: Rectangle<u32>,
        available: Size,
        page_height: u32,
        fit_width: bool,
    ) -> Self {
        let source_width = source.width.max(1) as f32;
        let source_page_height = page_height as f32;
        let width = (available.width - SCROLLBAR_SPACE_PX).max(1.0);
        let height = (available.height - BEYOND_LIMIT_PX).max(1.0);
        let scale = if fit_width {
            width / source_width
        } else {
            (width / source_width)
                .min(height / source_page_height)
                .min(1.0)
        };
        Self {
            width: source_width * scale,
            page_height: source_page_height * scale,
            scale,
            start: source.y,
        }
    }

    fn zoomed(mut self, zoom: f32) -> Self {
        self.width *= zoom;
        self.page_height *= zoom;
        self.scale *= zoom;
        self
    }

    fn rule_bounds(&self, source_y: u32, page_height: f32) -> Rectangle {
        let height = (RULE_HIT_PX * 2.0).min(page_height);
        let y = (self.local_y(source_y) - RULE_HIT_PX).clamp(0.0, page_height - height);
        Rectangle {
            x: 0.0,
            y,
            width: self.width,
            height,
        }
    }

    fn local_y(&self, source_y: u32) -> f32 {
        source_y.saturating_sub(self.start) as f32 * self.scale
    }
}

struct PreviewSegments {
    prefix: Option<(Rectangle<u32>, Size)>,
    page: Rectangle<u32>,
    page_size: Size,
    remainder: Option<(Rectangle<u32>, Size)>,
}

impl PreviewSegments {
    #[cfg(test)]
    fn new(region: Rectangle<u32>, geometry: &PreviewGeometry) -> Self {
        Self::configured(region, geometry, a4_height(region.width))
    }
    fn configured(region: Rectangle<u32>, geometry: &PreviewGeometry, page_height: u32) -> Self {
        let page_height = page_height.min(region.height);
        let page = Rectangle {
            height: page_height,
            ..region
        };
        let page_size = Size::new(
            geometry.width,
            geometry
                .page_height
                .min(region.height as f32 * geometry.scale),
        );
        let prefix = (region.y > 0).then(|| {
            (
                Rectangle {
                    y: 0,
                    height: region.y,
                    ..region
                },
                Size::new(geometry.width, region.y as f32 * geometry.scale),
            )
        });
        let remainder = (page_height < region.height).then(|| {
            let remaining = Rectangle {
                y: region.y + page_height,
                height: region.height - page_height,
                ..region
            };
            let size = Size::new(geometry.width, remaining.height as f32 * geometry.scale);
            (remaining, size)
        });
        Self {
            prefix,
            page,
            page_size,
            remainder,
        }
    }
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
    reported_scale: Option<f32>,
}

struct GuideOverlay {
    source: Rectangle<u32>,
    scale: f32,
    guide_y: f32,
    guide_source: u32,
    limit_y: f32,
}

impl GuideOverlay {
    fn near_rule(&self, position: Point) -> bool {
        (position.y - self.guide_y).abs() <= RULE_HIT_PX
    }

    fn drag_position(&self, position: Point) -> u32 {
        let y = (position.y / self.scale)
            .round()
            .clamp(1.0, self.source.height as f32) as u32;
        self.source.y + y
    }
}

impl widget::canvas::Program<Message> for GuideOverlay {
    type State = DragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &widget::canvas::Event,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Option<widget::canvas::Action<Message>> {
        use iced::mouse::{Button, Event};
        use widget::canvas::Action;

        match event {
            widget::canvas::Event::Window(iced::window::Event::RedrawRequested(_))
                if state.reported_scale != Some(self.scale) =>
            {
                state.reported_scale = Some(self.scale);
                Some(Action::publish(Message::PreviewScale(self.scale)))
            }
            widget::canvas::Event::Mouse(Event::ButtonPressed(Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                if self.near_rule(position) {
                    state.dragging = true;
                    return Some(
                        Action::publish(Message::MoveGuide(self.guide_source)).and_capture(),
                    );
                }
                Some(
                    Action::publish(Message::MoveGuide(self.drag_position(position))).and_capture(),
                )
            }
            widget::canvas::Event::Mouse(Event::CursorMoved { .. }) if state.dragging => {
                let position = cursor.position_from(bounds.position())?;
                let end = self.drag_position(position);
                Some(Action::publish(Message::MoveGuide(end)).and_capture())
            }
            widget::canvas::Event::Mouse(Event::ButtonReleased(Button::Left)) if state.dragging => {
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
        use widget::canvas::{LineDash, Path, Stroke, Text};
        let mut frame = widget::canvas::Frame::new(renderer, bounds.size());
        let limit_y = (self.limit_y - 1.0).max(0.0);
        frame.stroke(
            &Path::line(Point::new(0.0, limit_y), Point::new(bounds.width, limit_y)),
            Stroke {
                line_dash: LineDash {
                    segments: &metrics::LIMIT_DASH,
                    offset: 0,
                },
                ..Stroke::default().with_color(app_theme.palette().secondary.base.color)
            },
        );
        frame.fill_text(Text {
            content: "Page limit".into(),
            position: Point::new(
                theme::spacing().space_xxs,
                limit_y - theme::spacing().space_xxs,
            ),
            align_y: iced::alignment::Vertical::Bottom,
            color: app_theme.palette().secondary.base.color,
            ..Default::default()
        });
        draw_rule(&mut frame, self.guide_y, app_theme.seed().primary, true);
        vec![frame.into_geometry()]
    }
}

struct NonActiveRules {
    rules: Vec<(usize, f32)>,
}

impl NonActiveRules {
    fn new(slices: &SliceEditor, source_end: u32, scale: f32) -> Self {
        let rules = slices
            .pages()
            .filter(|(_, page)| page.end() <= source_end)
            .map(|(index, page)| {
                (
                    index,
                    (page.region().y + page.rectangle.height) as f32 * scale,
                )
            })
            .collect();
        Self { rules }
    }
}

impl widget::canvas::Program<Message> for NonActiveRules {
    type State = ();
    fn update(
        &self,
        _state: &mut Self::State,
        event: &widget::canvas::Event,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Option<widget::canvas::Action<Message>> {
        if !matches!(
            event,
            widget::canvas::Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Left
            ))
        ) {
            return None;
        }
        let position = cursor.position_in(bounds)?;
        let (index, _) = self
            .rules
            .iter()
            .find(|(_, y)| (position.y - y).abs() <= RULE_HIT_PX)?;
        Some(widget::canvas::Action::publish(Message::SelectPage(*index)).and_capture())
    }
    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> iced::mouse::Interaction {
        if cursor.position_in(bounds).is_some_and(|p| {
            self.rules
                .iter()
                .any(|(_, y)| (p.y - y).abs() <= RULE_HIT_PX)
        }) {
            iced::mouse::Interaction::Move
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
        for (_, y) in &self.rules {
            draw_rule(&mut frame, *y, app_theme.seed().primary, false);
        }
        vec![frame.into_geometry()]
    }
}

struct ZoomRule {
    y: f32,
    source: Rectangle<u32>,
    scale: f32,
    cut: u32,
}
impl ZoomRule {
    fn overlay(&self) -> GuideOverlay {
        GuideOverlay {
            source: self.source,
            scale: self.scale,
            guide_y: self.y,
            guide_source: self.cut,
            limit_y: 0.0,
        }
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
        if matches!(event, widget::canvas::Event::Window(_)) {
            return None;
        }
        self.overlay().update(state, event, bounds, cursor)
    }
    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> iced::mouse::Interaction {
        self.overlay().mouse_interaction(state, bounds, cursor)
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
            true,
        );
        vec![frame.into_geometry()]
    }
}

fn draw_rule(frame: &mut widget::canvas::Frame, y: f32, color: Color, handles: bool) {
    use widget::canvas::{Path, Stroke};
    frame.stroke(
        &Path::line(Point::new(0.0, y), Point::new(frame.width(), y)),
        Stroke::default()
            .with_color(color)
            .with_width(metrics::RULE_WIDTH),
    );
    if handles {
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

    fn region() -> Rectangle<u32> {
        Rectangle {
            x: 0,
            y: 1000,
            width: 190,
            height: 600,
        }
    }
    fn overlay() -> GuideOverlay {
        GuideOverlay {
            source: region(),
            scale: 0.5,
            guide_y: 100.0,
            guide_source: 1200,
            limit_y: 138.5,
        }
    }

    #[test]
    fn initial_preview_leaves_one_hundred_pixels_beyond_a4() {
        let viewport = Size::new(1000.0, 760.0);
        let geometry = PreviewGeometry::fit(
            Rectangle {
                width: 2000,
                height: 12000,
                ..region()
            },
            viewport,
        );
        assert!((geometry.page_height + BEYOND_LIMIT_PX - viewport.height).abs() < 0.001);
    }

    #[test]
    fn tooltip_hover_area_stays_on_the_cut_line_at_page_edges() {
        let geometry = PreviewGeometry::fit(region(), Size::new(1000.0, 760.0));
        let height = geometry.page_height;
        for cut in [1000, 1100, 1277] {
            let target = geometry.rule_bounds(cut, height);
            assert_eq!(target.width, geometry.width);
            assert!(target.y >= 0.0 && target.y + target.height <= height);
            let visible_rule_y = geometry.local_y(cut).clamp(1.0, height - 1.0);
            assert!(target.contains(Point::new(geometry.width / 2.0, visible_rule_y)));
            assert_eq!(target.height, RULE_HIT_PX * 2.0);
        }
        let target = geometry.rule_bounds(1100, height);
        assert!(!target.contains(Point::new(geometry.width / 2.0, 0.0)));
    }

    #[test]
    fn contiguous_segments_and_history_preserve_all_source_rows() {
        let geometry = PreviewGeometry::fit(region(), Size::new(1000.0, 760.0));
        let segments = PreviewSegments::new(region(), &geometry);
        assert_eq!(segments.prefix.unwrap().0.height, 1000);
        assert_eq!(segments.page.height, 277);
        assert_eq!(segments.remainder.unwrap().0.y, 1277);
        assert_eq!(segments.remainder.unwrap().0.height, 323);
    }

    #[test]
    fn image_clicks_move_the_rule_without_starting_a_drag() {
        let overlay = overlay();
        let bounds = Rectangle {
            width: 95.0,
            height: 138.5,
            ..Rectangle::default()
        };
        let event = widget::canvas::Event::Mouse(iced::mouse::Event::ButtonPressed(
            iced::mouse::Button::Left,
        ));
        let mut state = DragState::default();
        let action = overlay
            .update(
                &mut state,
                &event,
                bounds,
                iced::mouse::Cursor::Available(Point::new(30.0, 40.0)),
            )
            .unwrap();
        assert!(matches!(
            action.into_inner().0,
            Some(Message::MoveGuide(1080))
        ));
        assert!(!state.dragging);
    }

    #[test]
    fn pressing_the_rule_adjusts_it_without_quick_cutting() {
        let overlay = overlay();
        let bounds = Rectangle {
            width: 95.0,
            height: 138.5,
            ..Rectangle::default()
        };
        let event = widget::canvas::Event::Mouse(iced::mouse::Event::ButtonPressed(
            iced::mouse::Button::Left,
        ));
        let mut state = DragState::default();
        let action = overlay
            .update(
                &mut state,
                &event,
                bounds,
                iced::mouse::Cursor::Available(Point::new(30.0, 100.0)),
            )
            .unwrap();
        assert!(matches!(
            action.into_inner().0,
            Some(Message::MoveGuide(1200))
        ));
        assert!(state.dragging);
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
    fn drag_uses_source_coordinates_even_when_scrolled() {
        let overlay = overlay();
        let bounds = Rectangle {
            x: 32.0,
            y: -50.0,
            width: 95.0,
            height: 138.5,
        };
        let mut state = DragState {
            dragging: true,
            reported_scale: None,
        };
        let position = Point::new(62.0, 25.0);
        let event = widget::canvas::Event::Mouse(iced::mouse::Event::CursorMoved { position });
        let action = overlay
            .update(
                &mut state,
                &event,
                bounds,
                iced::mouse::Cursor::Available(position),
            )
            .unwrap();
        assert!(matches!(
            action.into_inner().0,
            Some(Message::MoveGuide(1150))
        ));
        let release = widget::canvas::Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Left,
        ));
        assert!(
            overlay
                .update(
                    &mut state,
                    &release,
                    bounds,
                    iced::mouse::Cursor::Available(position)
                )
                .unwrap()
                .into_inner()
                .0
                .is_none()
        );
        assert!(!state.dragging);
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
