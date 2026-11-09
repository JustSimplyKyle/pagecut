//! Compact-layout bottom sheet: Cut, Pages and Setup behind a segmented tab row.
use crate::{Message, cut_layer::CutModel, layout, sidebar};
use vse_ui::iced::Widget;
use vse_ui::{
    Apply,
    iced::{self, Fill, alignment::Vertical},
    theme,
    widget::{self, segmented_button::SingleSelectModel},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Cut,
    Pages,
    Setup,
}

impl Tab {
    const ALL: [Self; 3] = [Self::Cut, Self::Pages, Self::Setup];

    fn label(self) -> &'static str {
        match self {
            Self::Cut => "Cut",
            Self::Pages => "Pages",
            Self::Setup => "Setup",
        }
    }

    fn icon(self) -> widget::Svg<'static, vse_ui::Theme> {
        match self {
            Self::Cut => widget::icon!("edit-cut-symbolic"),
            Self::Pages => widget::icon!("edit-copy-symbolic"),
            Self::Setup => widget::icon!("document-page-setup-symbolic"),
        }
    }
}

/// The page dots under the Pages tab, rebuilt whenever the pages change.
pub fn page_model(cuts: &CutModel, selected: Option<usize>) -> SingleSelectModel<usize> {
    let label = |index| {
        format!(
            "Page {} · {}",
            index + 1,
            sidebar::page_status(cuts, selected, index)
        )
    };
    let mut builder = widget::segmented_button::SingleSelectModelBuilder::new()
        .insert(label(0), 0)
        .with_first_as_active();
    for index in 1..cuts.pages().count() {
        builder = builder.insert(label(index), index);
    }
    let mut model = builder.build();
    let selected = model
        .entries()
        .iter()
        .find_map(|(id, entry)| (Some(*entry.data()) == selected).then_some(*id));
    if let Some(id) = selected {
        model.activate(id);
    }
    model
}

pub fn tabs(active: Tab) -> impl Widget<Message> {
    let last = Tab::ALL.len() - 1;
    Tab::ALL
        .into_iter()
        .enumerate()
        .map(|(index, tab)| {
            let selected = tab == active;
            widget::row![
                tab.icon().style(move |_: &vse_ui::Theme, _| {
                    widget::svg::Style {
                        color: Some(if selected {
                            theme::color(theme::COSMIC.accent.on)
                        } else {
                            theme::icon_color()
                        }),
                    }
                }),
                widget::text(tab.label())
            ]
            .spacing(theme::spacing().space_xxs)
            .align_y(Vertical::Center)
            .apply(widget::container)
            .center_x(Fill)
            .apply(widget::button)
            .padding([6, 14])
            .width(Fill)
            .on_press(Message::SheetTab(tab))
            .style(move |theme, status| {
                theme::segmented_button(theme, status, selected, index == 0, index == last)
            })
        })
        .apply(widget::row)
        .spacing(1)
        .width(Fill)
}

pub fn pages<'a>(
    allocation: &'a widget::image::Allocation,
    cuts: &CutModel,
    selected: Option<usize>,
    model: &'a SingleSelectModel<usize>,
) -> impl Widget<Message> + 'a {
    let pager = widget::pager::horizontal(model)
        .on_activate(move |id| Message::SelectPage(*model.entries()[&id].data()));
    let footer = widget::row![
        pager,
        widget::space().width(Fill),
        widget::text::caption(format!("{} pages", cuts.pages().count()))
            .style(widget::text::secondary)
    ]
    .align_y(Vertical::Center);
    widget::column![sidebar::strip(allocation, cuts, selected), footer]
        .spacing(theme::spacing().space_s)
        .padding(theme::spacing().space_s)
}

pub fn setup(settings: layout::PageSettings, fit_width: bool) -> impl Widget<Message> {
    let paper = segments(
        layout::Paper::ALL,
        settings.paper,
        |paper, _| widget::text(paper.to_string()),
        Message::Paper,
    );
    let orientation = segments(
        layout::Orientation::ALL,
        settings.orientation,
        orientation_icon,
        Message::Orientation,
    );
    let margins = segments(
        layout::Margin::ALL,
        settings.margin,
        |margin, _| widget::text(margin.0.to_string()),
        Message::Margin,
    );
    let fit = widget::row![
        widget::column![
            widget::text::body("Fit preview to width"),
            widget::text::caption("Scale the image to fill the editor")
                .style(widget::text::secondary)
        ]
        .width(Fill),
        widget::checkbox(fit_width).on_toggle(Message::FitWidthToggled)
    ]
    .align_y(Vertical::Center);
    widget::column![
        field("Paper size", paper),
        widget::row![
            field("Orientation", orientation),
            field("Margins (mm)", margins)
        ]
        .spacing(theme::spacing().space_s),
        fit
    ]
    .spacing(theme::spacing().space_s)
    .padding(theme::spacing().space_s)
}

fn field<'a>(label: &'a str, control: impl Widget<Message> + 'a) -> impl Widget<Message> + 'a {
    widget::column![
        widget::text::caption(label).style(widget::text::secondary),
        control
    ]
    .spacing(theme::spacing().space_xxs)
    .width(Fill)
}

fn orientation_icon(
    orientation: &layout::Orientation,
    active: bool,
) -> impl Widget<Message> + use<> {
    const LONG: f32 = 16.0;
    const SHORT: f32 = LONG * 3.0 / 4.0;
    let [width, height] = match orientation {
        layout::Orientation::Portrait => [SHORT, LONG],
        layout::Orientation::Landscape => [LONG, SHORT],
    };
    let tone = if active {
        theme::color(theme::COSMIC.accent.on)
    } else {
        theme::icon_color()
    };
    let shape = widget::space()
        .width(width)
        .height(height)
        .apply(widget::container)
        .style(move |_| widget::container::Style {
            border: iced::border::rounded(2).color(tone).width(1.5),
            ..widget::container::Style::default()
        });
    // A fixed-height slot keeps both buttons as tall as the text segments.
    widget::container(shape).center_y(LONG + 4.0)
}

/// A row of mutually exclusive choices, styled like the tab row.
fn segments<T: Copy + PartialEq + 'static, W: Widget<Message>, const N: usize>(
    options: [T; N],
    selected: T,
    label: impl Fn(&T, bool) -> W,
    on_press: impl Fn(T) -> Message,
) -> impl Widget<Message> {
    options
        .into_iter()
        .enumerate()
        .map(|(index, option)| {
            let active = option == selected;
            label(&option, active)
                .apply(widget::container)
                .center_x(Fill)
                .apply(widget::button)
                .padding([6, 0])
                .width(Fill)
                .on_press(on_press(option))
                .style(move |theme, status| {
                    theme::segmented_button(theme, status, active, index == 0, index == N - 1)
                })
        })
        .apply(widget::row)
        .spacing(1)
        .width(Fill)
}

/// A round "Jump to next page" button beside the tabs, so moving on does not
/// require dragging the sheet open.
fn jump_to_next_page(action: Option<Message>) -> impl Widget<Message> {
    let button = crate::on_accent(widget::icon!("go-down-symbolic"))
        .apply(widget::button)
        .padding(theme::spacing().space_xs)
        .style(theme::button::suggested)
        .on_press_maybe(action);
    widget::tooltip(button, "Jump to next page", widget::tooltip::Position::Top)
}

/// Wraps the active tab's panel under the tab row on the sheet surface.
pub fn sheet<'a>(
    active: Tab,
    next_page: Option<Message>,
    content: impl Widget<Message> + 'a,
) -> impl Widget<Message> + 'a {
    let header = widget::row![tabs(active), jump_to_next_page(next_page)]
        .spacing(theme::spacing().space_xs)
        .align_y(Vertical::Center);
    widget::column![header, content]
        .spacing(theme::spacing().space_xxs)
        .apply(widget::container)
        .padding(theme::spacing().space_xs)
        .height(Fill)
        .width(Fill)
        .style(theme::container::navigation)
}
