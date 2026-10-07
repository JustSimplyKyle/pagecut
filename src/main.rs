#![feature(try_trait_v2, try_blocks)]

use std::convert::Infallible;
use std::ops::FromResidual;
use std::path::PathBuf;
use std::sync::Arc;

use vse_ui::iced::{self, Fill, Task};
use vse_ui::{Apply, Element, theme, widget};

mod pdf;
mod preview;
mod sidebar;
mod slices;

use slices::SliceEditor;

fn main() -> iced::Result {
    iced::application(Longshot::new, Longshot::update, Longshot::view)
        .title("Longshot")
        .theme(|_: &Longshot| theme::iced_theme())
        .subscription(Longshot::subscription)
        .window_size((1200.0, 900.0))
        .run()
}

struct Longshot {
    image: Option<widget::image::Allocation>,
    source: Option<widget::image::Handle>,
    error: Option<Arc<eyre::Report>>,
    slicing: Option<SliceEditor>,
    preview_scale: f32,
    status: Option<String>,
}

#[derive(Debug, Clone)]
enum Message {
    PickImage,
    ImagePicked(Option<PathBuf>),
    ExportPdf,
    PdfExported(Result<Option<PathBuf>, Arc<eyre::Report>>),
    DismissError,
    AllocateImage(widget::image::Handle),
    ImageAllocated(widget::image::Allocation),
    ErrorReported(Arc<eyre::Report>),
    MoveGuide(u32),
    NudgeGuide(i32),
    ConfirmCut,
    SelectPage(usize),
    PreviewScale(f32),
    Restart,
}

impl Message {
    fn keyboard(event: iced::keyboard::Event) -> Option<Self> {
        use iced::keyboard::{Key, key::Named};

        let iced::keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
            return None;
        };
        if modifiers.control() || modifiers.alt() || modifiers.logo() {
            return None;
        }
        let step = if modifiers.shift() { 5 } else { 1 };
        match key {
            Key::Named(Named::ArrowUp) => Some(Self::NudgeGuide(-step)),
            Key::Named(Named::ArrowDown) => Some(Self::NudgeGuide(step)),
            _ => None,
        }
    }
}

struct Event(Task<Message>);

impl From<Task<Message>> for Event {
    fn from(task: Task<Message>) -> Self {
        Self(task)
    }
}

impl From<Message> for Event {
    fn from(message: Message) -> Self {
        Self(Task::done(message))
    }
}

impl From<Event> for Task<Message> {
    fn from(event: Event) -> Self {
        event.0
    }
}

impl<E: Into<Arc<eyre::Report>>> FromResidual<Result<Infallible, E>> for Event {
    fn from_residual(residual: Result<Infallible, E>) -> Self {
        match residual {
            Err(error) => Message::ErrorReported(error.into()).into(),
            Ok(never) => match never {},
        }
    }
}

impl FromResidual<Option<Infallible>> for Event {
    fn from_residual(residual: Option<Infallible>) -> Self {
        residual.map_or_else(|| Task::none().into(), |never| match never {})
    }
}

impl Longshot {
    fn subscription(&self) -> iced::Subscription<Message> {
        if self.error.is_none()
            && self
                .slicing
                .as_ref()
                .is_some_and(|slices| slices.current().is_some())
        {
            iced::keyboard::listen().filter_map(Message::keyboard)
        } else {
            iced::Subscription::none()
        }
    }

    fn new() -> (Self, Task<Message>) {
        let task = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .map(widget::image::Handle::from_path)
            .map_or_else(Task::none, |handle| {
                Task::done(Message::AllocateImage(handle))
            });

        (
            Self {
                image: None,
                source: None,
                error: None,
                slicing: None,
                preview_scale: 1.0,
                status: None,
            },
            task,
        )
    }
    fn update(&mut self, message: Message) -> Event {
        match message {
            Message::PickImage => {
                self.status = Some("Choose an image…".into());
                Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Open image")
                            .add_filter("Images", &["png", "jpg", "jpeg"])
                            .pick_file()
                            .await
                            .map(|file| file.path().to_path_buf())
                    },
                    Message::ImagePicked,
                )
            }
            Message::ImagePicked(path) => {
                self.status = None;
                return self.update(Message::AllocateImage(widget::image::Handle::from_path(
                    path?,
                )));
            }
            Message::ExportPdf => {
                let slices = self
                    .slicing
                    .as_ref()
                    .filter(|slices| slices.export_ready())?;
                let source = self.source.clone()?;
                let pages = slices.pages().map(|(_, region)| region).collect();
                self.status = Some("Exporting PDF…".into());
                Task::perform(pdf::choose_and_export(source, pages), |result| {
                    Message::PdfExported(result.map_err(Arc::new))
                })
            }
            Message::PdfExported(result) => {
                self.status = None;
                self.status = Some(format!("Saved {}", result??.display()));
                Task::none()
            }
            Message::DismissError => {
                self.error = None;
                Task::none()
            }
            Message::AllocateImage(handle) => {
                self.status = Some("Loading image…".into());
                self.source = Some(handle.clone());
                self.image = None;
                self.error = None;
                self.slicing = None;
                widget::image::allocate(handle).map(|result| match result {
                    Ok(allocation) => Message::ImageAllocated(allocation),
                    Err(error) => Message::ErrorReported(Arc::new(error.into())),
                })
            }
            Message::ImageAllocated(allocation) => {
                self.status = None;
                self.slicing = Some(SliceEditor::new(allocation.size()));
                self.image = Some(allocation);
                self.error = None;
                Task::none()
            }
            Message::ErrorReported(error) => {
                self.status = None;
                self.error = Some(error);
                Task::none()
            }
            Message::MoveGuide(end) => {
                self.status = None;
                if let Some(slices) = &mut self.slicing {
                    slices.move_guide(end);
                }
                Task::none()
            }
            Message::NudgeGuide(delta) => {
                self.status = None;
                if let Some(slices) = &mut self.slicing {
                    slices.nudge(delta);
                }
                Task::none()
            }
            Message::ConfirmCut => {
                self.status = None;
                if self.slicing.as_mut().is_some_and(SliceEditor::confirm) {
                    return self.scroll_to_selected().into();
                }
                Task::none()
            }
            Message::SelectPage(index) => {
                if self
                    .slicing
                    .as_mut()
                    .is_some_and(|slices| slices.select(index))
                {
                    return self.scroll_to_selected().into();
                }
                Task::none()
            }
            Message::PreviewScale(scale) => {
                self.preview_scale = scale;
                self.scroll_to_selected()
            }
            Message::Restart => {
                self.status = None;
                if let Some(slices) = &mut self.slicing {
                    slices.restart();
                }
                self.scroll_to_selected()
            }
        }
        .into()
    }

    fn view(&self) -> Element<'_, Message> {
        let content = if let Some(error) = &self.error {
            self.error_screen(error)
        } else {
            match (&self.image, &self.slicing) {
                (Some(allocation), Some(slices)) => Self::picker(allocation, slices),
                _ => self.placeholder(),
            }
        };
        let content = content
            .apply(widget::container)
            .padding(16)
            .width(Fill)
            .height(Fill)
            .style(theme::container::card);
        let status = self
            .status
            .as_deref()
            .or_else(|| self.slicing.as_ref().and_then(SliceEditor::export_hint))
            .map(|status| widget::text(status).size(13));

        widget::column![self.toolbar(), status, content]
            .spacing(12)
            .padding(16)
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn toolbar(&self) -> Element<'_, Message> {
        let open = widget::text("Open image…")
            .apply(widget::button)
            .on_press(Message::PickImage);
        let can_export = self.error.is_none()
            && self.source.is_some()
            && self.slicing.as_ref().is_some_and(SliceEditor::export_ready);
        let export = widget::text("Export PDF…")
            .apply(widget::button)
            .style(theme::button::suggested)
            .on_press_maybe(can_export.then_some(Message::ExportPdf));
        let title = match &self.source {
            Some(widget::image::Handle::Path(_, path)) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            _ => "Longshot".into(),
        };
        widget::row![
            open,
            widget::text(title),
            widget::space().width(Fill),
            export
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn scroll_to_selected(&self) -> Task<Message> {
        self.slicing.as_ref().map_or_else(Task::none, |slices| {
            preview::scroll_to(slices.start(), self.preview_scale)
        })
    }

    fn picker<'a>(
        allocation: &'a widget::image::Allocation,
        slices: &'a SliceEditor,
    ) -> Element<'a, Message> {
        let sidebar = sidebar::pages(allocation, slices);
        let editor = if slices.current().is_some() {
            Self::editor(allocation, slices)
        } else {
            Self::completed(slices)
        };

        widget::row![sidebar, editor]
            .spacing(16)
            .height(Fill)
            .into()
    }

    fn editor<'a>(
        allocation: &'a widget::image::Allocation,
        slices: &'a SliceEditor,
    ) -> Element<'a, Message> {
        let confirm = widget::text(slices.confirm_label())
            .apply(widget::button)
            .style(theme::button::suggested)
            .on_press(Message::ConfirmCut);
        let header = widget::row![
            widget::text(slices.page_label()),
            widget::space().width(Fill),
            confirm,
        ]
        .align_y(iced::Alignment::Center);
        let controls = widget::row![
            widget::text("Cut detail · ↑/↓ 1 px · Shift 5 px").size(13),
            widget::space().width(Fill),
            widget::text("↑ 1 px")
                .apply(widget::button)
                .on_press(Message::NudgeGuide(-1)),
            widget::text("↓ 1 px")
                .apply(widget::button)
                .on_press(Message::NudgeGuide(1)),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        let zoom = preview::zoom(allocation, slices)
            .apply(widget::container)
            .height(150)
            .width(Fill);
        let detail = widget::column![controls, zoom]
            .spacing(8)
            .apply(widget::container)
            .padding(12)
            .style(theme::container::card);

        widget::column![header, preview::image(allocation, slices), detail]
            .spacing(12)
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn completed<'a>(slices: &SliceEditor) -> Element<'a, Message> {
        let restart = widget::text("Start over")
            .apply(widget::button)
            .on_press(Message::Restart);
        widget::column![
            widget::text(slices.completion_label()).size(24),
            widget::text("Select a page in the sidebar to adjust its cut."),
            restart,
        ]
        .spacing(16)
        .apply(widget::container)
        .center(Fill)
        .into()
    }

    fn placeholder(&self) -> Element<'_, Message> {
        let label = if self.source.is_some() {
            "Loading image…"
        } else {
            "Open an image to start choosing page cuts."
        };

        widget::text(label)
            .apply(widget::container)
            .center(Fill)
            .into()
    }

    fn error_screen(&self, error: &eyre::Report) -> Element<'_, Message> {
        let details = widget::text(format!("{error:#}"))
            .apply(widget::scrollable)
            .height(180);
        let retry = if self.image.is_some() {
            Some(
                widget::text("Back to image")
                    .apply(widget::button)
                    .on_press(Message::DismissError),
            )
        } else {
            self.source.as_ref().map(|source| {
                widget::text("Retry")
                    .apply(widget::button)
                    .on_press(Message::AllocateImage(source.clone()))
            })
        };

        widget::column![
            widget::text("Unable to complete operation").size(24),
            details,
            retry
        ]
        .spacing(16)
        .apply(widget::container)
        .width(560)
        .apply(widget::container)
        .padding(24)
        .center(Fill)
        .into()
    }
}
