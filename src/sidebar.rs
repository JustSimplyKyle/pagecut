use super::{Message, metrics};
use crate::cut_layer::CutModel;
use vse_ui::iced::Widget;
use vse_ui::iced::{Fill, Rectangle, Size};
use vse_ui::{Apply, theme, widget};

pub fn pages<'a>(
    allocation: &'a widget::image::Allocation,
    cuts: &CutModel,
    selected: Option<usize>,
) -> impl Widget<Message> + 'a {
    let thumbnails = thumbnails(allocation, cuts, selected).map(|page| thumbnail(allocation, page));
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
}
/// Every page with the region its thumbnail shows.
fn thumbnails(
    allocation: &widget::image::Allocation,
    cuts: &CutModel,
    selected: Option<usize>,
) -> impl Iterator<Item = Thumbnail> {
    let width = allocation.size().width;
    let max_length = cuts.max_length();
    cuts.pages().enumerate().map(move |(index, page)| {
        let fits = page.end - page.start <= max_length;
        // Show at most a page of an oversized page, so it doesn't fill the list.
        let end = page.end.min(page.start + max_length);
        let region = Rectangle {
            x: 0,
            y: page.start as u32,
            width,
            height: (end - page.start) as u32,
        };
        Thumbnail::new(index, region, fits, selected == Some(index))
    })
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
            status: status(fits, active),
        }
    }
    fn glyph(&self) -> &'static str {
        if !self.fits {
            "⚠"
        } else if self.active {
            "○"
        } else {
            "✓"
        }
    }
    fn style(&self) -> impl Fn(&vse_ui::Theme) -> widget::text::Style + use<> {
        let (active, fits) = (self.active, self.fits);
        move |theme| {
            if !fits {
                widget::text::warning(theme)
            } else if active {
                theme::text::accent(theme)
            } else {
                widget::text::secondary(theme)
            }
        }
    }
}
fn status(fits: bool, active: bool) -> &'static str {
    if !fits {
        "Too long — add a cut"
    } else if active {
        "Editing"
    } else {
        "Ready"
    }
}
/// The status word shown beside a page, without its glyph.
pub fn page_status(cuts: &CutModel, selected: Option<usize>, index: usize) -> &'static str {
    let fits = cuts
        .pages()
        .nth(index)
        .is_none_or(|page| page.end - page.start <= cuts.max_length());
    status(fits, selected == Some(index))
}
/// Every page as a card in one horizontally scrolling row, for compact layouts.
pub fn strip<'a>(
    allocation: &'a widget::image::Allocation,
    cuts: &CutModel,
    selected: Option<usize>,
) -> impl Widget<Message> + 'a {
    thumbnails(allocation, cuts, selected)
        .map(|page| card(allocation, page))
        .apply(widget::row)
        .spacing(theme::spacing().space_xxs)
        .apply(widget::scrollable)
        .horizontal()
        .width(Fill)
}
fn card(allocation: &widget::image::Allocation, thumbnail: Thumbnail) -> impl Widget<Message> + '_ {
    let image = widget::image(allocation.handle())
        .crop(thumbnail.region)
        .content_fit(vse_ui::iced::ContentFit::Contain)
        .width(Fill)
        .height(metrics::CARD_IMAGE_HEIGHT)
        .apply(widget::container)
        .padding(theme::spacing().space_xxs)
        .width(Fill)
        .style(theme::container::secondary);
    let active = thumbnail.active;
    let style = thumbnail.style();
    widget::column![
        image,
        widget::text::body(thumbnail.label),
        widget::text::caption(thumbnail.status).style(style)
    ]
    .spacing(theme::spacing().space_xxs)
    .apply(widget::button)
    .padding(theme::spacing().space_xxs)
    .width(metrics::CARD_WIDTH)
    .on_press(Message::SelectPage(thumbnail.index))
    .style(if active {
        theme::button::navigation_active
    } else {
        theme::button::navigation_inactive
    })
}
fn thumbnail(
    allocation: &widget::image::Allocation,
    thumbnail: Thumbnail,
) -> impl Widget<Message> + '_ {
    let image = widget::image(allocation.handle())
        .crop(thumbnail.region)
        .content_fit(vse_ui::iced::ContentFit::Contain)
        .width(thumbnail.size.width)
        .height(thumbnail.size.height)
        .apply(widget::container)
        .padding(theme::spacing().space_xxs)
        .style(theme::container::secondary);
    let status = format!("{}  {}", thumbnail.glyph(), thumbnail.status);
    let (active, style) = (thumbnail.active, thumbnail.style());
    let labels = widget::column![
        widget::text::body(thumbnail.label),
        widget::text::caption(status).style(style)
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
}
