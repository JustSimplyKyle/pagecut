//! Cut controls; geometry and page calculations belong to the editor and preview.
use crate::{Message, blank::CutDetection, metrics, preview, slices::SliceEditor};
use vse_ui::{
    Apply, Element,
    iced::{self, Fill},
    theme, widget,
};

pub fn cut<'a>(
    allocation: &'a widget::image::Allocation,
    slices: &'a SliceEditor,
    detection: &'a CutDetection,
) -> Element<'a, Message> {
    let controls = widget::column![
        cut_preview(allocation, slices),
        cut_position(slices),
        widget::text::caption("↑/↓  1 px    Shift  5 px").style(widget::text::secondary),
        widget::rule::horizontal(1),
        cut_detection(detection),
        widget::rule::horizontal(1),
        cut_actions(slices),
    ]
    .spacing(theme::spacing().space_xs)
    .apply(widget::scrollable)
    .height(Fill);
    widget::column![
        widget::text::title3("Adjust cut"),
        controls,
        widget::rule::horizontal(1),
        widget::text::caption(slices.export_label()).style(widget::text::secondary),
    ]
    .spacing(theme::spacing().space_s)
    .apply(widget::container)
    .padding(theme::spacing().space_s)
    .width(metrics::INSPECTOR)
    .height(Fill)
    .style(theme::container::card)
    .into()
}

fn cut_preview<'a>(
    allocation: &'a widget::image::Allocation,
    slices: &SliceEditor,
) -> Element<'a, Message> {
    widget::column![
        widget::text::body("Cut preview").style(widget::text::secondary),
        preview::zoom(allocation, slices)
            .apply(widget::container)
            .height(metrics::DETAIL_HEIGHT)
            .width(Fill),
    ]
    .spacing(theme::spacing().space_xxs)
    .apply(widget::container)
    .padding(theme::spacing().space_xs)
    .style(theme::container::secondary)
    .into()
}

fn cut_position(slices: &SliceEditor) -> Element<'_, Message> {
    let minus = widget::text("−")
        .apply(widget::button)
        .padding(theme::spacing().space_xxs)
        .on_press(Message::NudgeGuide(-1));
    let plus = widget::text("＋")
        .apply(widget::button)
        .padding(theme::spacing().space_xxs)
        .on_press(Message::NudgeGuide(1));
    widget::row![
        widget::text::body("Cut position"),
        widget::space().width(Fill),
        widget::text::body(slices.position_label()),
        minus,
        plus
    ]
    .spacing(theme::spacing().space_xxs)
    .align_y(iced::Alignment::Center)
    .into()
}

fn cut_detection(detection: &CutDetection) -> Element<'_, Message> {
    let label = widget::text::caption(detection.label()).style(match detection {
        CutDetection::Uniform => widget::text::success,
        CutDetection::Mixed | CutDetection::Failed(_) => widget::text::warning,
        _ => widget::text::secondary,
    });
    widget::column![
        widget::tooltip(
            widget::text::body("Cut detection"),
            "checks whether all pixels on the line is roughly the same color",
            widget::tooltip::Position::Top
        ),
        label
    ]
    .spacing(theme::spacing().space_xxs)
    .into()
}

fn cut_actions(slices: &SliceEditor) -> Element<'static, Message> {
    let enabled = !slices.is_last_page();
    let apply = widget::text("Apply cut")
        .apply(widget::button)
        .padding(theme::spacing().space_xs)
        .width(Fill)
        .style(theme::button::suggested)
        .on_press_maybe(enabled.then_some(Message::ConfirmCut));
    let reset = widget::text("Reset cut")
        .apply(widget::button)
        .padding(theme::spacing().space_xs)
        .width(Fill)
        .on_press_maybe(enabled.then_some(Message::ResetCut));
    widget::column![apply, reset]
        .spacing(theme::spacing().space_xxs)
        .into()
}
