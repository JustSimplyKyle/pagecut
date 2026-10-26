#![feature(try_trait_v2, try_blocks, iter_map_windows)]

use std::convert::Infallible;
use std::ops::FromResidual;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::sync::Arc;

use vse_ui::iced::{self, Fill, Task, advanced::graphics::image::Buffer};
use vse_ui::{Apply, Element, theme, widget};

mod background;
mod blank;
mod cut_layer;
mod image_load;
mod inspector;
mod layout;
mod metrics;
mod pdf;
mod preview;
mod sidebar;

use cut_layer::{CutAction, CutModel};
use inspector::SelectedCut;

fn main() -> iced::Result {
    // wasm_thread initializes this module again in each worker.
    #[cfg(target_arch = "wasm32")]
    if wasm_thread::is_web_worker_thread() {
        return Ok(());
    }

    iced::application(App::new, App::update, App::view)
        .title("Pagecut")
        .theme(|_: &App| theme::iced_theme())
        .subscription(App::subscription)
        .window_size((1440.0, 940.0))
        .run()
}

struct App {
    image: Option<widget::image::Allocation>,
    source: Option<widget::image::Handle>,
    error: Option<Arc<eyre::Report>>,
    cuts: Option<CutModel>,
    /// The cut the inspector and keyboard adjust.
    selected: Option<usize>,
    /// The cut being dragged, so a drag is undone in one step.
    dragging: Option<usize>,
    preview_scale: f32,
    status: Option<String>,
    opening: bool,
    reading: Option<image_load::ReadProgress>,
    exporting: Option<pdf::ExportProgress>,
    pixels: Option<Arc<Buffer>>,
    cut_detection: blank::CutDetection,
    cut_check_id: u64,
    zoom: f32,
    output: layout::PageSettings,
    load_id: u64,
    fit_width: bool,
    undo: Vec<CutModel>,
    redo: Vec<CutModel>,
}

#[derive(Debug, Clone)]
enum Message {
    PickImage,
    ImageReadProgress(image_load::ReadProgress),
    ImagePicked(Result<Option<Vec<u8>>, Arc<eyre::Report>>),
    ExportPdf,
    PdfProgress(pdf::ExportProgress),
    PdfExported(Result<Option<String>, Arc<eyre::Report>>),
    DismissError,
    AllocateImage(widget::image::Handle),
    ImageAllocated(u64, widget::image::Allocation),
    ImageLoadFailed(u64, Arc<eyre::Report>),
    ErrorReported(Arc<eyre::Report>),
    Cut(CutAction),
    NudgeCut(i32),
    SelectPage(usize),
    PreviewScale(f32),
    PixelsLoaded(u64, Arc<Buffer>),
    CutSettled(u64),
    CutChecked(u64, blank::CutDetection),
    Undo,
    Redo,
    Zoom(i32),
    FitWidth,
    Paper(layout::Paper),
    Orientation(layout::Orientation),
    Margin(layout::Margin),
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
            Key::Named(Named::ArrowUp) => Some(Self::NudgeCut(-step)),
            Key::Named(Named::ArrowDown) => Some(Self::NudgeCut(step)),
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

impl App {
    fn subscription(&self) -> iced::Subscription<Message> {
        if self.error.is_none() && self.selected.is_some() {
            iced::keyboard::listen().filter_map(Message::keyboard)
        } else {
            iced::Subscription::none()
        }
    }

    fn new() -> (Self, Task<Message>) {
        #[cfg(not(target_arch = "wasm32"))]
        let task = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .map(widget::image::Handle::from_path)
            .map_or_else(Task::none, |handle| {
                Task::done(Message::AllocateImage(handle))
            });
        #[cfg(target_arch = "wasm32")]
        let task = Task::none();

        (
            Self {
                image: None,
                source: None,
                error: None,
                cuts: None,
                selected: None,
                dragging: None,
                preview_scale: 1.0,
                status: None,
                opening: false,
                reading: None,
                exporting: None,
                pixels: None,
                cut_detection: blank::CutDetection::Waiting,
                cut_check_id: 0,
                zoom: 1.0,
                output: layout::PageSettings::default(),
                load_id: 0,
                fit_width: false,
                undo: Vec::new(),
                redo: Vec::new(),
            },
            task,
        )
    }
    fn update(&mut self, message: Message) -> Event {
        let previous_cut = self.cut_key();
        let event = self.react(message);
        if previous_cut != self.cut_key() {
            Task::batch([event.0, self.schedule_cut_check()]).into()
        } else {
            event
        }
    }

    fn cut_key(&self) -> Option<(u64, u32)> {
        self.selected_cut()
            .map(|selected| (self.load_id, selected.position))
    }

    fn selected_cut(&self) -> Option<SelectedCut> {
        let index = self.selected?;
        let position = *self.cuts.as_ref()?.cuts().get(index)?;
        Some(SelectedCut {
            index,
            position: position as u32,
        })
    }

    fn schedule_cut_check(&mut self) -> Task<Message> {
        self.cut_check_id += 1;
        self.cut_detection = blank::CutDetection::Waiting;
        if self.cut_key().is_none() || self.pixels.is_none() {
            return Task::none();
        }
        let check_id = self.cut_check_id;
        Task::perform(blank::settled(check_id), Message::CutSettled)
    }

    fn react(&mut self, message: Message) -> Event {
        match message {
            Message::PickImage => {
                if self.opening {
                    return Task::none().into();
                }
                self.opening = true;
                self.status = Some("Choose an image…".into());
                Task::run(
                    iced::stream::channel(32, async |mut output| {
                        use iced::futures::SinkExt;
                        let result = async {
                            let Some(file) = rfd::AsyncFileDialog::new()
                                .set_title("Open image")
                                .add_filter("Images", &["png", "jpg", "jpeg"])
                                .pick_file()
                                .await
                            else {
                                return Ok(None);
                            };
                            let mut progress_output = output.clone();
                            image_load::read(file, move |progress| {
                                let _ =
                                    progress_output.try_send(Message::ImageReadProgress(progress));
                            })
                            .await
                            .map(Some)
                        }
                        .await;
                        let _ = output
                            .send(Message::ImagePicked(result.map_err(Arc::new)))
                            .await;
                    }),
                    std::convert::identity,
                )
            }
            Message::ImageReadProgress(progress) => {
                if self.opening {
                    self.reading = Some(progress);
                    self.status = Some(progress.label());
                }
                Task::none()
            }
            Message::ImagePicked(result) => {
                self.opening = false;
                self.reading = None;
                self.status = None;
                return self.update(Message::AllocateImage(widget::image::Handle::from_bytes(
                    result??,
                )));
            }
            Message::ExportPdf => {
                if self.exporting.is_some() {
                    return Task::none().into();
                }
                let cuts = self.cuts.as_ref().filter(|cuts| cuts.fits())?;
                let source = self.source.clone()?;
                let pixels = self.pixels.clone()?;
                let pages = pdf::regions(cuts, pixels.width());
                let settings = self.output;
                self.status = None;
                self.exporting = Some(pdf::ExportProgress::ChoosingDestination);
                Task::run(
                    iced::stream::channel(32, async move |mut output| {
                        use iced::futures::SinkExt;
                        let progress_output = output.clone();
                        let result = pdf::choose_and_export(
                            source,
                            pixels,
                            pages,
                            settings,
                            move |progress| {
                                let mut sender = progress_output.clone();
                                let _ = sender.try_send(Message::PdfProgress(progress));
                            },
                        )
                        .await;
                        let _ = output
                            .send(Message::PdfExported(result.map_err(Arc::new)))
                            .await;
                    }),
                    std::convert::identity,
                )
            }
            Message::PdfProgress(progress) => {
                if self.exporting.is_some() {
                    self.exporting = Some(progress);
                }
                Task::none()
            }
            Message::PdfExported(result) => {
                self.exporting = None;
                self.status = format!("PDF saved: {}", result??).apply(Some);
                Task::none()
            }
            Message::DismissError => {
                self.error = None;
                Task::none()
            }
            Message::AllocateImage(handle) => {
                self.output = self.settings();
                self.load_id += 1;
                let load_id = self.load_id;
                self.status = Some("Decoding image…".into());
                self.source = Some(handle.clone());
                self.image = None;
                self.error = None;
                self.cuts = None;
                self.selected = None;
                self.dragging = None;
                self.pixels = None;
                self.undo.clear();
                self.redo.clear();
                Task::perform(
                    async move { image_load::decode(handle).await.map_err(Arc::new) },
                    move |result| match result {
                        Ok(pixels) => Message::PixelsLoaded(load_id, pixels),
                        Err(error) => Message::ImageLoadFailed(load_id, error),
                    },
                )
            }
            Message::ImageLoadFailed(load_id, error) => {
                if load_id != self.load_id {
                    return Task::none().into();
                }
                return self.update(Message::ErrorReported(error));
            }
            Message::ImageAllocated(load_id, allocation) => {
                if load_id != self.load_id {
                    return Task::none().into();
                }
                self.status = None;
                if self.pixels.is_some() {
                    self.cuts = Some(Self::paginate(allocation.size(), self.output));
                }
                self.image = Some(allocation);
                self.error = None;
                Task::none()
            }
            Message::PixelsLoaded(load_id, pixels) => {
                if load_id != self.load_id {
                    return Task::none().into();
                }

                let handle = widget::image::Handle::from_rgba(
                    pixels.width(),
                    pixels.height(),
                    pixels.as_raw().clone(),
                );
                self.pixels = Some(pixels);
                self.status = Some("Preparing image preview…".into());
                widget::image::allocate(handle).map(move |result| match result {
                    Ok(allocation) => Message::ImageAllocated(load_id, allocation),
                    Err(error) => Message::ImageLoadFailed(load_id, Arc::new(error.into())),
                })
            }
            Message::CutSettled(check_id) => {
                if check_id != self.cut_check_id {
                    return Task::none().into();
                }
                let pixels = self.pixels.clone()?;
                let cut = self.selected_cut()?.position;
                self.cut_detection = blank::CutDetection::Checking;
                Task::perform(blank::check(pixels, cut), move |result| {
                    Message::CutChecked(check_id, result)
                })
            }
            Message::CutChecked(check_id, result) => {
                if check_id == self.cut_check_id {
                    self.cut_detection = result;
                }
                Task::none()
            }
            Message::Undo => {
                let previous = self.undo.pop()?;
                let current = self.cuts.replace(previous)?;
                self.redo.push(current);
                self.restore_selection()
            }
            Message::Redo => {
                let next = self.redo.pop()?;
                let current = self.cuts.replace(next)?;
                self.undo.push(current);
                self.restore_selection()
            }
            Message::Zoom(delta) => {
                self.zoom = (self.zoom + delta as f32 * 0.1).clamp(0.3, 3.0);
                Task::none()
            }
            Message::FitWidth => {
                self.zoom = 1.0;
                self.fit_width = true;
                Task::none()
            }
            Message::ErrorReported(error) => {
                self.status = None;
                self.error = Some(error);
                Task::none()
            }
            Message::Cut(action) => {
                self.status = None;
                let continues_drag =
                    matches!(action, CutAction::Move(index, _) if self.dragging == Some(index));
                let cuts = self.cuts.as_mut()?;
                let before = cuts.clone();
                cuts.perform(action.clone());
                self.dragging = None;
                match action {
                    CutAction::Introduce(position) => self.selected = cuts.index_of(position),
                    CutAction::Move(index, _) => {
                        self.selected = Some(index);
                        self.dragging = Some(index);
                    }
                    CutAction::Remove(_) => self.selected = None,
                }
                if *cuts != before && !continues_drag {
                    self.undo.push(before);
                    self.redo.clear();
                }
                Task::none()
            }
            Message::NudgeCut(delta) => {
                let selected = self.selected_cut()?;
                let position = selected.position.saturating_add_signed(delta);
                return self.react(Message::Cut(CutAction::Move(
                    selected.index,
                    u64::from(position),
                )));
            }
            Message::SelectPage(index) => {
                let cuts = self.cuts.as_ref()?;
                // A page is selected through the cut at its bottom; the last page has none.
                self.selected = (index < cuts.cuts().len()).then_some(index);
                self.dragging = None;
                self.scroll_to_page(index)
            }
            Message::PreviewScale(scale) => {
                self.preview_scale = scale;
                self.scroll_to_selected()
            }
            Message::Paper(paper) => {
                let settings = layout::PageSettings {
                    paper,
                    ..self.settings()
                };
                self.repaginate(settings)
            }
            Message::Orientation(orientation) => {
                let settings = layout::PageSettings {
                    orientation,
                    ..self.settings()
                };
                self.repaginate(settings)
            }
            Message::Margin(margin) => {
                let settings = layout::PageSettings {
                    margin,
                    ..self.settings()
                };
                self.repaginate(settings)
            }
        }
        .into()
    }

    fn view(&self) -> Element<'_, Message> {
        let content = if let Some(error) = &self.error {
            self.error_screen(error)
        } else {
            match (&self.image, &self.cuts) {
                (Some(allocation), Some(cuts)) => self.picker(allocation, cuts),
                _ => self.placeholder(),
            }
        };
        widget::column![
            self.titlebar(),
            self.toolbar(),
            self.export_status(),
            content
        ]
        .spacing(theme::spacing().space_xxs)
        .padding(theme::spacing().space_xxs)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn export_status(&self) -> Option<Element<'_, Message>> {
        if let Some(progress) = &self.exporting {
            let bar = progress
                .fraction()
                .map(|value| widget::progress_bar(0.0..=1.0, value));
            let hint = if matches!(progress, pdf::ExportProgress::ChoosingDestination) {
                "Select a destination in the save dialog."
            } else {
                "Please wait until the PDF has finished saving."
            };
            Some(
                widget::column![
                    widget::text::title3(progress.label()).style(theme::text::accent),
                    bar,
                    widget::text::caption(hint).style(widget::text::secondary),
                ]
                .spacing(theme::spacing().space_xxs)
                .apply(widget::container)
                .padding(theme::spacing().space_s)
                .width(Fill)
                .style(theme::container::card)
                .into(),
            )
        } else if let Some(progress) = self.reading {
            Some(
                widget::column![
                    widget::text::body(progress.label()),
                    widget::progress_bar(0.0..=1.0, progress.fraction()),
                ]
                .spacing(theme::spacing().space_xxs)
                .into(),
            )
        } else {
            self.status
                .as_deref()
                .map(|label| widget::text::body(label).into())
        }
    }

    fn titlebar(&self) -> Element<'_, Message> {
        let title = match &self.source {
            Some(widget::image::Handle::Path(_, path)) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            _ => "Open an image to begin".into(),
        };
        widget::row![
            widget::text::title3("Pagecut"),
            widget::container(widget::rule::vertical(1)).height(16),
            widget::text(title).style(widget::text::secondary)
        ]
        .spacing(theme::spacing().space_s)
        .padding(theme::spacing().space_xxs)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn toolbar(&self) -> Element<'_, Message> {
        let can_export = !self.opening
            && self.exporting.is_none()
            && self.error.is_none()
            && self.source.is_some()
            && self.cuts.as_ref().is_some_and(CutModel::fits);
        let open = self.control(
            widget::text("Open…"),
            (!self.opening).then_some(Message::PickImage),
        );
        let undo = widget::tooltip(
            self.control(
                widget::icon!("edit-undo-symbolic"),
                (!self.undo.is_empty()).then_some(Message::Undo),
            ),
            "Undo",
            widget::tooltip::Position::Bottom,
        );
        let redo = widget::tooltip(
            self.control(
                widget::icon!("edit-redo-symbolic"),
                (!self.redo.is_empty()).then_some(Message::Redo),
            ),
            "Redo",
            widget::tooltip::Position::Bottom,
        );
        let settings = self.settings();
        let output = widget::row![
            widget::pick_list(
                Some(settings.paper),
                layout::Paper::ALL,
                ToString::to_string
            )
            .on_select(Message::Paper),
            widget::pick_list(
                Some(settings.orientation),
                layout::Orientation::ALL,
                ToString::to_string
            )
            .on_select(Message::Orientation),
            widget::pick_list(
                Some(settings.margin),
                layout::Margin::ALL,
                ToString::to_string
            )
            .on_select(Message::Margin)
        ]
        .spacing(theme::spacing().space_xs)
        .align_y(iced::Alignment::Center);
        let zoom = widget::row![
            self.control(widget::icon!("list-remove-symbolic"), Some(Message::Zoom(-1))),
            widget::text(self.zoom_label()),
            self.control(widget::icon!("list-add-symbolic"), Some(Message::Zoom(1)))
        ]
        .spacing(theme::spacing().space_xxs)
        .align_y(iced::Alignment::Center);
        let export_label = self
            .exporting
            .as_ref()
            .map_or("Export PDF…", pdf::ExportProgress::button_label);
        let export = widget::text(export_label)
            .apply(widget::button)
            .padding([theme::spacing().space_xxs, theme::spacing().space_s])
            .style(theme::button::suggested)
            .on_press_maybe(can_export.then_some(Message::ExportPdf));
        widget::row![
            open,
            undo,
            redo,
            output,
            self.control(
                widget::row![
                    widget::icon!("zoom-fit-best-symbolic"),
                    widget::text("Fit width")
                ]
                .spacing(theme::spacing().space_xxs)
                .align_y(iced::Alignment::Center),
                Some(Message::FitWidth),
            ),
            zoom,
            widget::space().width(Fill),
            export
        ]
        .spacing(theme::spacing().space_xs)
        .align_y(iced::Alignment::Center)
        .apply(widget::container)
        .padding(theme::spacing().space_xs)
        .style(theme::container::card)
        .into()
    }

    fn settings(&self) -> layout::PageSettings {
        self.output
    }
    fn repaginate(&mut self, settings: layout::PageSettings) -> Task<Message> {
        self.remember();
        self.output = settings;
        if let Some(image) = &self.image {
            self.cuts = Some(Self::paginate(image.size(), settings));
        }
        self.selected = None;
        self.dragging = None;
        self.scroll_to_page(0)
    }
    /// Cuts every page, a little short of the limit so cuts can be moved down.
    fn paginate(size: iced::Size<u32>, settings: layout::PageSettings) -> CutModel {
        const MAX_ADJUSTMENT_RESERVE: u32 = 80;
        const ADJUSTMENT_RESERVE_FRACTION: u32 = 20;
        let page_height = settings.height(size.width);
        let reserve = (page_height / ADJUSTMENT_RESERVE_FRACTION).min(MAX_ADJUSTMENT_RESERVE);
        CutModel::paginated(
            u64::from(size.height),
            u64::from(page_height),
            u64::from((page_height - reserve).max(1)),
        )
    }
    fn zoom_label(&self) -> String {
        format!("{:.0}%", self.preview_scale * 100.0)
    }
    fn remember(&mut self) {
        if let Some(current) = &self.cuts {
            self.undo.push(current.clone());
            self.redo.clear();
        }
    }
    /// Keeps the selection on a cut that still exists after undo or redo.
    fn restore_selection(&mut self) -> Task<Message> {
        self.dragging = None;
        let count = self.cuts.as_ref().map_or(0, |cuts| cuts.cuts().len());
        self.selected = self.selected.filter(|&index| index < count);
        self.scroll_to_selected()
    }
    fn control<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
        action: Option<Message>,
    ) -> Element<'a, Message> {
        widget::button(content)
            .padding(theme::spacing().space_xxs)
            .on_press_maybe(action)
            .into()
    }

    fn scroll_to_selected(&self) -> Task<Message> {
        self.selected
            .map_or_else(Task::none, |index| self.scroll_to_page(index))
    }

    fn scroll_to_page(&self, index: usize) -> Task<Message> {
        self.cuts
            .as_ref()
            .and_then(|cuts| cuts.pages().nth(index))
            .map_or_else(Task::none, |page| {
                preview::scroll_to(page.start as u32, self.preview_scale)
            })
    }

    fn picker<'a>(
        &'a self,
        allocation: &'a widget::image::Allocation,
        cuts: &'a CutModel,
    ) -> Element<'a, Message> {
        widget::row![
            sidebar::pages(allocation, cuts, self.selected),
            Self::editor(allocation, cuts, self.zoom, self.fit_width),
            inspector::cut(
                allocation,
                self.selected_cut(),
                &self.cut_detection,
                Self::summary(cuts)
            )
        ]
        .spacing(theme::spacing().space_xxs)
        .height(Fill)
        .into()
    }

    /// Whether the pages are ready to export, and if not, why.
    fn summary(cuts: &CutModel) -> String {
        let count = cuts.pages().count();
        match cuts
            .pages()
            .position(|page| page.end - page.start > cuts.max_length())
        {
            None => format!("All {count} pages will be included in the PDF."),
            Some(index) => format!(
                "Page {} is longer than a page; add a cut above the dotted line.",
                index + 1
            ),
        }
    }

    fn editor<'a>(
        allocation: &'a widget::image::Allocation,
        cuts: &'a CutModel,
        zoom: f32,
        fit_width: bool,
    ) -> Element<'a, Message> {
        let header = widget::row![
            widget::text::title3("Editor"),
            widget::container(widget::rule::vertical(1)).height(16),
            widget::text::caption("Drag a line to move a cut, click to add one, right-click to remove")
                .style(widget::text::secondary)
        ]
        .spacing(theme::spacing().space_xs)
        .align_y(iced::Alignment::Center);
        let footer = widget::text::caption(format!("{} pages", cuts.pages().count()))
            .style(widget::text::secondary);
        widget::column![
            header,
            preview::preview(allocation, cuts, zoom, fit_width),
            footer
        ]
        .spacing(theme::spacing().space_xs)
        .apply(widget::container)
        .padding(theme::spacing().space_s)
        .style(theme::container::card)
        .width(Fill)
        .height(Fill)
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
            .height(metrics::ERROR_HEIGHT);
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
            widget::text::title1("Unable to complete operation"),
            details,
            retry
        ]
        .spacing(theme::spacing().space_s)
        .apply(widget::container)
        .width(metrics::ERROR_WIDTH)
        .apply(widget::container)
        .padding(theme::spacing().space_m)
        .center(Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn image_reads_clear_busy_state_on_cancel_or_failure() {
        let (mut app, _) = super::App::new();
        app.opening = true;
        let progress = super::image_load::ReadProgress {
            completed: 4,
            total: 8,
        };
        let _ = app.update(super::Message::ImageReadProgress(progress));
        assert_eq!(app.reading, Some(progress));
        let _ = app.update(super::Message::PickImage);
        assert_eq!(app.reading, Some(progress));
        let _ = app.update(super::Message::ImagePicked(Ok(None)));
        assert!(!app.opening);
        assert!(app.reading.is_none());
        assert!(app.status.is_none());
        let _ = app.update(super::Message::ImageReadProgress(progress));
        assert!(app.reading.is_none());
        app.opening = true;
        let _ = app.update(super::Message::ImagePicked(Err(std::sync::Arc::new(
            eyre::eyre!("read failed"),
        ))));
        assert!(!app.opening);
        assert!(app.reading.is_none());
    }

    use super::*;

    fn app() -> App {
        let (mut app, _) = App::new();
        app.cuts = Some(App::paginate(
            iced::Size::new(190, 700),
            layout::PageSettings::default(),
        ));
        app
    }

    fn cuts(app: &App) -> Vec<u64> {
        app.cuts.as_ref().unwrap().cuts().to_vec()
    }

    #[test]
    fn undo_and_redo_restore_cuts() {
        let mut app = app();
        let original = cuts(&app);
        let _ = app.update(Message::Cut(CutAction::Introduce(100)));
        assert_eq!(app.selected, Some(0));
        let introduced = cuts(&app);
        assert_ne!(introduced, original);
        let _ = app.update(Message::Undo);
        assert_eq!(cuts(&app), original);
        let _ = app.update(Message::Redo);
        assert_eq!(cuts(&app), introduced);
    }

    #[test]
    fn a_drag_is_undone_in_one_step_and_no_ops_are_not_remembered() {
        let mut app = app();
        let original = cuts(&app);
        for position in [200, 190, 180] {
            let _ = app.update(Message::Cut(CutAction::Move(0, position)));
        }
        let _ = app.update(Message::NudgeCut(-1));
        assert_eq!(cuts(&app)[0], 179);
        assert_eq!(app.undo.len(), 1);
        let _ = app.update(Message::Cut(CutAction::Introduce(5000)));
        assert_eq!(app.undo.len(), 1);
        let _ = app.update(Message::Undo);
        assert_eq!(cuts(&app), original);
    }

    #[test]
    fn selecting_a_page_selects_the_cut_at_its_bottom() {
        let mut app = app();
        let count = app.cuts.as_ref().unwrap().pages().count();
        let _ = app.update(Message::SelectPage(1));
        assert_eq!(app.selected, Some(1));
        let _ = app.update(Message::SelectPage(count - 1));
        assert_eq!(app.selected, None);
        let _ = app.update(Message::SelectPage(0));
        let _ = app.update(Message::Cut(CutAction::Remove(0)));
        assert_eq!(app.selected, None);
    }

    #[test]
    fn exporting_stays_visible_during_edits_and_rejects_duplicate_exports() {
        let mut app = app();
        app.source = Some(widget::image::Handle::from_rgba(
            190,
            700,
            vec![255; 190 * 700 * 4],
        ));
        app.pixels = Some(Arc::new(
            iced::advanced::graphics::image::load(app.source.as_ref().unwrap()).unwrap(),
        ));
        let _ = app.update(Message::ExportPdf);
        assert_eq!(
            app.exporting,
            Some(pdf::ExportProgress::ChoosingDestination)
        );
        let _ = app.update(Message::PdfProgress(pdf::ExportProgress::PreparingDocument));
        let _ = app.update(Message::ExportPdf);
        assert_eq!(app.exporting, Some(pdf::ExportProgress::PreparingDocument));
        let _ = app.update(Message::Cut(CutAction::Move(0, 100)));
        assert!(app.export_status().is_some());
        assert_eq!(app.exporting, Some(pdf::ExportProgress::PreparingDocument));
        let _ = app.update(Message::PdfExported(Ok(Some("notes.pdf".into()))));
        assert!(app.exporting.is_none());
        assert!(app.status.as_ref().unwrap().contains("PDF saved"));
    }

    #[test]
    fn pages_longer_than_the_limit_block_export() {
        let mut app = app();
        assert!(app.cuts.as_ref().unwrap().fits());
        // The cut after it is pulled up, so the last page grows past the limit.
        let _ = app.update(Message::Cut(CutAction::Remove(0)));
        assert!(!app.cuts.as_ref().unwrap().fits());
        assert!(App::summary(app.cuts.as_ref().unwrap()).contains("longer than a page"));
    }

    #[test]
    fn cancelled_or_failed_exports_clear_the_busy_state() {
        let (mut app, _) = App::new();
        app.exporting = Some(pdf::ExportProgress::ChoosingDestination);
        let _ = app.update(Message::PdfExported(Ok(None)));
        assert!(app.exporting.is_none());
        assert!(app.status.is_none());
        app.exporting = Some(pdf::ExportProgress::SavingFile);
        let _ = app.update(Message::PdfExported(Err(Arc::new(eyre::eyre!(
            "Export failed"
        )))));
        assert!(app.exporting.is_none());
    }

    #[test]
    fn detection_waits_for_the_latest_cut_and_ignores_stale_results() {
        let mut app = app();
        let pixels = iced::advanced::graphics::image::load(&widget::image::Handle::from_rgba(
            190,
            700,
            vec![255; 190 * 700 * 4],
        ))
        .unwrap();
        app.pixels = Some(Arc::new(pixels));
        let _ = app.update(Message::Cut(CutAction::Move(0, 200)));
        let first = app.cut_check_id;
        let _ = app.update(Message::Cut(CutAction::Move(0, 150)));
        let latest = app.cut_check_id;
        assert!(latest > first);
        assert_eq!(app.cut_detection, blank::CutDetection::Waiting);
        let _ = app.update(Message::CutSettled(first));
        let _ = app.update(Message::CutChecked(first, blank::CutDetection::Mixed));
        assert_eq!(app.cut_detection, blank::CutDetection::Waiting);
        let _ = app.update(Message::CutSettled(latest));
        assert_eq!(app.cut_detection, blank::CutDetection::Checking);
        let _ = app.update(Message::CutChecked(latest, blank::CutDetection::Uniform));
        assert_eq!(app.cut_detection, blank::CutDetection::Uniform);
        let _ = app.update(Message::Cut(CutAction::Move(0, 150)));
        assert_eq!(app.cut_check_id, latest);
        assert_eq!(app.cut_detection, blank::CutDetection::Uniform);
        let _ = app.update(Message::SelectPage(1));
        assert!(app.cut_check_id > latest);
        assert_eq!(app.cut_detection, blank::CutDetection::Waiting);
        let _ = app.update(Message::CutChecked(latest, blank::CutDetection::Mixed));
        assert_eq!(app.cut_detection, blank::CutDetection::Waiting);
        let _ = app.update(Message::AllocateImage(widget::image::Handle::from_path(
            "replacement.png",
        )));
        let _ = app.update(Message::CutChecked(latest, blank::CutDetection::Mixed));
        assert_eq!(app.cut_detection, blank::CutDetection::Waiting);
    }

    #[test]
    fn stale_image_detection_cannot_replace_the_current_document() {
        let (mut app, _) = App::new();
        app.load_id = 2;
        let pixels = iced::advanced::graphics::image::load(&widget::image::Handle::from_rgba(
            1,
            1,
            vec![255; 4],
        ))
        .unwrap();
        let _ = app.update(Message::PixelsLoaded(1, Arc::new(pixels)));
        assert!(app.pixels.is_none());
    }
}
