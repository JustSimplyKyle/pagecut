use super::{
    Message, metrics,
    slices::{SliceEditor, a4_height},
};
use vse_ui::iced::{Fill, Size};
use vse_ui::{Apply, Element, theme, widget};

pub fn pages<'a>(
    allocation: &'a widget::image::Allocation,
    slices: &SliceEditor,
) -> Element<'a, Message> {
    let mut thumbnails = widget::column![].spacing(theme::spacing().space_xxs);
    for (index, region) in slices.pages() {
        thumbnails = thumbnails.push(thumbnail(
            allocation,
            Thumbnail::new(index, region.region(), false, slices.selected()),
        ));
    }
    if let Some((index, region)) = slices.pending() {
        thumbnails = thumbnails.push(thumbnail(
            allocation,
            Thumbnail::new(index, region, true, slices.selected()),
        ));
    }
    let header = widget::row![
        widget::text::title3("Pages"),
        widget::space().width(Fill),
        widget::text::caption(slices.count_label()).style(widget::text::secondary)
    ]
    .align_y(vse_ui::iced::Alignment::Center);
    let list = thumbnails.apply(widget::scrollable).height(Fill);
    widget::column![header, list]
        .spacing(theme::spacing().space_xs)
        .apply(widget::container)
        .padding(theme::spacing().space_xs)
        .width(metrics::SIDEBAR)
        .height(Fill)
        .style(theme::container::card)
        .into()
}
struct Thumbnail {
    index: usize,
    region: vse_ui::iced::Rectangle<u32>,
    size: Size,
    label: String,
    status: &'static str,
    active: bool,
}
impl Thumbnail {
    fn new(
        index: usize,
        mut region: vse_ui::iced::Rectangle<u32>,
        pending: bool,
        selected: usize,
    ) -> Self {
        if pending {
            region.height = region.height.min(a4_height(region.width));
        }
        let size = Size::new(
            metrics::THUMB_WIDTH,
            metrics::THUMB_WIDTH * region.height as f32 / region.width.max(1) as f32,
        );
        let active = index == selected;
        Self {
            index,
            region,
            size,
            label: format!("Page {}", index + 1),
            active,
            status: if active {
                "○  Editing"
            } else if pending {
                "○  Remaining"
            } else {
                "✓  Ready"
            },
        }
    }
}
fn thumbnail(allocation: &widget::image::Allocation, thumbnail: Thumbnail) -> Element<'_, Message> {
    let image = widget::image(allocation.handle())
        .crop(thumbnail.region)
        .content_fit(vse_ui::iced::ContentFit::Contain)
        .width(thumbnail.size.width)
        .height(thumbnail.size.height)
        .apply(widget::container)
        .padding(theme::spacing().space_xxs)
        .style(theme::container::secondary);
    let labels = widget::column![
        widget::text::body(thumbnail.label),
        widget::text::caption(thumbnail.status).style(move |theme| {
            if thumbnail.active {
                theme::text::accent(theme)
            } else {
                widget::text::secondary(theme)
            }
        })
    ]
    .spacing(theme::spacing().space_xxs);
    let active = thumbnail.active;
    widget::row![image, labels]
        .spacing(theme::spacing().space_xs)
        .align_y(vse_ui::iced::Alignment::Center)
        .apply(widget::button)
        .padding(theme::spacing().space_xxs)
        .width(Fill)
        .on_press(Message::SelectPage(thumbnail.index))
        .style(if active {
            theme::button::navigation_active
        } else {
            theme::button::navigation_inactive
        })
        .into()
}
