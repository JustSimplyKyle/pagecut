use super::{Message, metrics};
use crate::cut_layer::CutModel;
use vse_ui::iced::{Fill, Rectangle, Size};
use vse_ui::{Apply, Element, theme, widget};

pub fn pages<'a>(
    allocation: &'a widget::image::Allocation,
    cuts: &CutModel,
    selected: Option<usize>,
) -> Element<'a, Message> {
    let width = allocation.size().width;
    let thumbnails = cuts.pages().enumerate().map(|(index, page)| {
        let fits = page.end - page.start <= cuts.max_length();
        // Show at most a page of an oversized page, so it doesn't fill the list.
        let end = page.end.min(page.start + cuts.max_length());
        let region = Rectangle {
            x: 0,
            y: page.start as u32,
            width,
            height: (end - page.start) as u32,
        };
        thumbnail(
            allocation,
            Thumbnail::new(index, region, fits, selected == Some(index)),
        )
    });
    let count = cuts.pages().count();
    let header = widget::row![
        widget::text::title3("Pages"),
        widget::space().width(Fill),
        widget::text::caption(format!("{count} pages")).style(widget::text::secondary)
    ]
    .align_y(vse_ui::iced::Alignment::Center);
    let list = widget::column(thumbnails)
        .spacing(theme::spacing().space_xxs)
        .apply(widget::scrollable)
        .height(Fill);
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
    region: Rectangle<u32>,
    size: Size,
    label: String,
    status: &'static str,
    fits: bool,
    active: bool,
}
impl Thumbnail {
    fn new(index: usize, region: Rectangle<u32>, fits: bool, active: bool) -> Self {
        let size = Size::new(
            metrics::THUMB_WIDTH,
            metrics::THUMB_WIDTH * region.height as f32 / region.width.max(1) as f32,
        );
        Self {
            index,
            region,
            size,
            label: format!("Page {}", index + 1),
            fits,
            active,
            status: if !fits {
                "⚠  Too long — add a cut"
            } else if active {
                "○  Editing"
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
    let (active, fits) = (thumbnail.active, thumbnail.fits);
    let labels = widget::column![
        widget::text::body(thumbnail.label),
        widget::text::caption(thumbnail.status).style(move |theme| {
            if !fits {
                widget::text::warning(theme)
            } else if active {
                theme::text::accent(theme)
            } else {
                widget::text::secondary(theme)
            }
        })
    ]
    .spacing(theme::spacing().space_xxs);
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
