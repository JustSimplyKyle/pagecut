//! Cut controls; geometry and page calculations belong to the editor and preview.
use crate::{Message, blank::CutDetection, cut_layer::CutAction, metrics, preview};
use vse_ui::{
    Apply, Element,
    iced::{self, Fill},
    theme, widget,
};

/// The selected cut, by index and position in source pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedCut {
    pub index: usize,
    pub position: u32,
}

pub fn cut<'a>(
    allocation: &'a widget::image::Allocation,
    selected: Option<SelectedCut>,
    detection: &'a CutDetection,
    summary: String,
) -> Element<'a, Message> {
    let controls: Element<'a, Message> = match selected {
        Some(selected) => widget::column![
            cut_preview(allocation, selected),
            cut_position(selected),
            widget::text::caption("Up/Down  1 px    Shift  5 px").style(widget::text::secondary),
            widget::rule::horizontal(1),
            cut_detection(detection),
            widget::rule::horizontal(1),
            remove_cut(selected),
        ]
        .spacing(theme::spacing().space_xs)
        .apply(widget::scrollable)
        .height(Fill)
        .into(),
        None => widget::text::body(
            "Click a cut to adjust it, or click between cuts to add one. \
             Right-click a cut to remove it.",
        )
        .style(widget::text::secondary)
        .apply(widget::container)
        .height(Fill)
        .into(),
    };
    widget::column![
        widget::text::title3("Adjust cut"),
        controls,
        widget::rule::horizontal(1),
        widget::text::caption(summary).style(widget::text::secondary),
    ]
    .spacing(theme::spacing().space_s)
    .apply(widget::container)
    .padding(theme::spacing().space_s)
    .width(metrics::INSPECTOR)
    .height(Fill)
    .style(theme::container::card)
    .into()
}

fn cut_preview(
    allocation: &widget::image::Allocation,
    selected: SelectedCut,
) -> Element<'_, Message> {
    widget::column![
        widget::text::body("Cut preview").style(widget::text::secondary),
        preview::zoom(allocation, selected.index, selected.position)
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

fn cut_position(selected: SelectedCut) -> Element<'static, Message> {
    let minus = widget::icon!("list-remove-symbolic")
        .apply(widget::button)
        .padding(theme::spacing().space_xxs)
        .on_press(Message::NudgeCut(-1));
    let plus = widget::icon!("list-add-symbolic")
        .apply(widget::button)
        .padding(theme::spacing().space_xxs)
        .on_press(Message::NudgeCut(1));
    widget::row![
        widget::text::body("Cut position"),
        widget::space().width(Fill),
        widget::text::body(format!("{} px", selected.position)),
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

fn remove_cut(selected: SelectedCut) -> Element<'static, Message> {
    widget::text("Remove cut")
        .apply(widget::button)
        .padding(theme::spacing().space_xs)
        .width(Fill)
        .on_press(Message::Cut(CutAction::Remove(selected.index)))
        .into()
}
