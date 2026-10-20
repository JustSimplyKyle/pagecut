//! Compact-layout bottom sheet: Cut, Pages and Setup behind a segmented tab row.
use crate::{Message, cut_layer::CutModel, layout, sidebar};
use vse_ui::{
    Apply, Element,
    iced::{Fill, alignment::Vertical},
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

pub fn tabs(active: Tab) -> Element<'static, Message> {
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
            .into()
        })
        .apply(widget::row)
        .spacing(1)
        .width(Fill)
        .into()
}

pub fn pages<'a>(
    allocation: &'a widget::image::Allocation,
    cuts: &CutModel,
    selected: Option<usize>,
    model: &'a SingleSelectModel<usize>,
) -> Element<'a, Message> {
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
        .into()
}

pub fn setup(settings: layout::PageSettings, fit_width: bool) -> Element<'static, Message> {
    let paper = segments(
        layout::Paper::ALL,
        settings.paper,
        ToString::to_string,
        Message::Paper,
    );
    let orientation = segments(
        layout::Orientation::ALL,
        settings.orientation,
        ToString::to_string,
        Message::Orientation,
    );
    let margins = segments(
        layout::Margin::ALL,
        settings.margin,
        |margin: &layout::Margin| margin.0.to_string(),
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
    .into()
}

fn field<'a>(label: &'a str, control: Element<'a, Message>) -> Element<'a, Message> {
    widget::column![
        widget::text::caption(label).style(widget::text::secondary),
        control
    ]
    .spacing(theme::spacing().space_xxs)
    .width(Fill)
    .into()
}

/// A row of mutually exclusive choices, styled like the tab row.
fn segments<T: Copy + PartialEq + 'static, const N: usize>(
    options: [T; N],
    selected: T,
    label: impl Fn(&T) -> String,
    on_press: impl Fn(T) -> Message,
) -> Element<'static, Message> {
    options
        .into_iter()
        .enumerate()
        .map(|(index, option)| {
            let active = option == selected;
            widget::text(label(&option))
                .apply(widget::container)
                .center_x(Fill)
                .apply(widget::button)
                .padding([6, 0])
                .width(Fill)
                .on_press(on_press(option))
                .style(move |theme, status| {
                    theme::segmented_button(theme, status, active, index == 0, index == N - 1)
                })
                .into()
        })
        .apply(widget::row)
        .spacing(1)
        .width(Fill)
        .into()
}

/// Wraps the active tab's panel under the tab row on the sheet surface.
pub fn sheet<'a>(active: Tab, content: Element<'a, Message>) -> Element<'a, Message> {
    widget::column![tabs(active), content]
        .spacing(theme::spacing().space_xxs)
        .apply(widget::container)
        .padding(theme::spacing().space_xs)
        .height(Fill)
        .width(Fill)
        .style(theme::container::navigation)
        .into()
}
