use vse_ui::iced::{Fill, Size};
use vse_ui::{Apply, Element, theme, theme::WithRadius, widget};

use super::{
    Message,
    slices::{SliceEditor, a4_height},
};

pub fn pages<'a>(
    allocation: &'a widget::image::Allocation,
    slices: &SliceEditor,
) -> Element<'a, Message> {
    let mut thumbnails = widget::column![].spacing(10);
    for (index, region) in slices.pages() {
        thumbnails = thumbnails.push(thumbnail(
            allocation,
            Thumbnail::new(index, region.region(), false),
            slices.selected(),
        ));
    }
    if let Some((index, region)) = slices.pending() {
        thumbnails = thumbnails.push(thumbnail(
            allocation,
            Thumbnail::new(index, region, true),
            slices.selected(),
        ));
    }
    let list = thumbnails.apply(widget::scrollable).height(Fill);
    widget::column![widget::text("Pages").size(20), list]
        .spacing(12)
        .apply(widget::container)
        .padding(12)
        .width(190)
        .height(Fill)
        .style(theme::container::card)
        .into()
}

struct Thumbnail {
    index: usize,
    region: vse_ui::iced::Rectangle<u32>,
    size: Size,
    label: String,
}

impl Thumbnail {
    fn new(index: usize, mut region: vse_ui::iced::Rectangle<u32>, pending: bool) -> Self {
        const WIDTH: f32 = 140.0;
        if pending {
            region.height = region.height.min(a4_height(region.width));
        }
        let size = Size::new(
            WIDTH,
            WIDTH * region.height as f32 / region.width.max(1) as f32,
        );
        let label = if pending {
            format!("Page {} · remaining", index + 1)
        } else {
            format!("Page {}", index + 1)
        };
        Self {
            index,
            region,
            size,
            label,
        }
    }
}

fn thumbnail(
    allocation: &widget::image::Allocation,
    thumbnail: Thumbnail,
    selected: usize,
) -> Element<'_, Message> {
    let image = widget::image(allocation.handle())
        .crop(thumbnail.region)
        .content_fit(vse_ui::iced::ContentFit::Contain)
        .width(thumbnail.size.width)
        .height(thumbnail.size.height)
        .apply(widget::container)
        .center_x(Fill);
    let active = thumbnail.index == selected;
    widget::column![widget::text(thumbnail.label).size(13).width(Fill), image]
        .spacing(6)
        .width(Fill)
        .apply(widget::button)
        .padding(8)
        .width(Fill)
        .on_press(Message::SelectPage(thumbnail.index))
        .style(move |theme, status| {
            let mut style = if active {
                theme::button::navigation_active(theme, status)
            } else {
                theme::button::standard(theme, status)
            }
            .with_radius(6.0);
            if active {
                style.border.width = 1.0;
                style.border.color = style.text_color;
            }
            style
        })
        .into()
}
