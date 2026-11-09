//! Cut controls; geometry and page calculations belong to the editor and preview.
use crate::{Message, blank::CutDetection, cut_layer::CutAction, metrics, preview};
use iced::{Length, alignment::Vertical};
use vse_ui::iced::Widget;
use vse_ui::{
    Apply,
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
    next_page: Option<Message>,
    style: impl Fn(&vse_ui::Theme) -> widget::container::Style + 'a,
    compacted: bool,
) -> impl Widget<Message> + 'a {
    let controls = match selected {
        Some(selected) => widget::column![
            (!compacted).then_some(cut_preview(allocation, selected)),
            cut_position(selected),
            widget::rule::horizontal(1),
            cut_detection(detection),
            widget::rule::horizontal(1),
            remove_cut(selected),
        ]
        .spacing(theme::spacing().space_xs)
        .apply(widget::scrollable)
        .height(Fill)
        .boxed(),
        None => widget::text::body(
            "Click a cut to adjust it, or click between cuts to add one. \
             Right-click a cut to remove it.",
        )
        .style(widget::text::secondary)
        .apply(widget::container)
        .height(Fill)
        .boxed(),
    };
    widget::column![
        controls,
        // The compact sheet has its own button beside the tabs.
        (!compacted).then(|| jump_to_next_page(next_page)),
        widget::rule::horizontal(1),
        widget::text::caption(summary).style(widget::text::secondary),
    ]
    .spacing(theme::spacing().space_s)
    .apply(widget::container)
    .padding(theme::spacing().space_s)
    .height(Fill)
    .style(style)
}

fn cut_preview(
    allocation: &widget::image::Allocation,
    selected: SelectedCut,
) -> impl Widget<Message> + '_ {
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
}

fn cut_position(selected: SelectedCut) -> impl Widget<Message> {
    let minus = widget::icon!("list-remove-symbolic")
        .apply(widget::button)
        .padding(theme::spacing().space_xxs)
        .on_press(Message::NudgeCut(-1));
    let plus = widget::icon!("list-add-symbolic")
        .apply(widget::button)
        .padding(theme::spacing().space_xxs)
        .on_press(Message::NudgeCut(1));
    widget::row![
        widget::tooltip(
            widget::text::body("Cut position"),
            "Up/Down  1 px    Shift  5 px",
            widget::tooltip::Position::FollowCursor
        ),
        widget::space().width(Fill),
        widget::text::body(format!("{} px", selected.position)),
        minus,
        plus
    ]
    .spacing(theme::spacing().space_xxs)
    .align_y(iced::Alignment::Center)
}

fn cut_detection(detection: &CutDetection) -> impl Widget<Message> + '_ {
    use theme::{COSMIC, color};
    let (tone, icon) = match detection {
        CutDetection::Waiting | CutDetection::Checking => (
            COSMIC.accent.base,
            widget::icon!("emblem-synchronizing-symbolic"),
        ),
        CutDetection::Uniform => (COSMIC.success.base, widget::icon!("object-select-symbolic")),
        CutDetection::Mixed => (
            COSMIC.warning.base,
            widget::icon!("dialog-warning-symbolic"),
        ),
        CutDetection::ImageEnd => (COSMIC.accent.base, widget::icon!("go-bottom-symbolic")),
        CutDetection::Failed(_) => (
            COSMIC.destructive.base,
            widget::icon!("dialog-error-symbolic"),
        ),
    };
    let tone = color(tone);
    let status = widget::row![
        icon.width(14)
            .height(14)
            .style(move |_, _| widget::svg::Style { color: Some(tone) }),
        widget::text::caption(detection.label()).color(tone),
    ]
    .spacing(theme::spacing().space_xxs)
    .align_y(Vertical::Center);
    let card = widget::row![
        widget::text::body("Cut detection")
            .font(iced::Font {
                weight: iced::font::Weight::Semibold,
                ..iced::Font::DEFAULT
            })
            .color(tone),
        widget::space().width(Length::Fill),
        status,
    ]
    .spacing(theme::spacing().space_xxs)
    .align_y(Vertical::Center)
    .apply(widget::container)
    .padding(theme::spacing().space_xs)
    .width(Length::Fill)
    // A tinted card with an outline in the status colour, like a selected row.
    .style(move |_| widget::container::Style {
        background: Some(tone.scale_alpha(0.12).into()),
        border: iced::border::rounded(theme::spacing().space_s),
        ..widget::container::Style::default()
    });
    widget::tooltip(
        card,
        "checks whether all pixels on the line is roughly the same color",
        widget::tooltip::Position::FollowCursor,
    )
}

fn jump_to_next_page(action: Option<Message>) -> impl Widget<Message> {
    widget::text("Jump to next page")
        .apply(widget::button)
        .padding(theme::spacing().space_xs)
        .width(Fill)
        .style(theme::button::suggested)
        .on_press_maybe(action)
}

fn remove_cut(selected: SelectedCut) -> impl Widget<Message> {
    widget::text("Remove cut")
        .apply(widget::button)
        .padding(theme::spacing().space_xs)
        .width(Fill)
        .on_press(Message::Cut(CutAction::Remove(selected.index)))
}
