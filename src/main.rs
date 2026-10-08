#![feature(try_trait_v2, try_blocks)]

use std::convert::Infallible;
use std::ops::FromResidual;
use std::path::PathBuf;
use std::sync::Arc;

use vse_ui::iced::{self, Fill, Task, advanced::graphics::image::Buffer};
use vse_ui::{Apply, Element, theme, widget};

mod blank;
mod inspector;
mod layout;
mod metrics;
mod pdf;
mod preview;
mod sidebar;
mod slices;

use slices::SliceEditor;

fn main() -> iced::Result {
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
    slice_editor: Option<SliceEditor>,
    preview_scale: f32,
    status: Option<String>,
    exporting: Option<pdf::ExportProgress>,
    pixels: Option<Arc<Buffer>>,
    cut_detection: blank::CutDetection,
    cut_check_id: u64,
    zoom: f32,
    output: layout::PageSettings,
    load_id: u64,
    fit_width: bool,
    undo: Vec<SliceEditor>,
    redo: Vec<SliceEditor>,
}

#[derive(Debug, Clone)]
enum Message {
    PickImage,
    ImagePicked(Option<PathBuf>),
    ExportPdf,
    PdfProgress(pdf::ExportProgress),
    PdfExported(Result<Option<PathBuf>, Arc<eyre::Report>>),
    DismissError,
    AllocateImage(widget::image::Handle),
    ImageAllocated(u64, widget::image::Allocation),
    ImageLoadFailed(u64, Arc<eyre::Report>),
    ErrorReported(Arc<eyre::Report>),
    MoveGuide(u32),
    NudgeGuide(i32),
    ConfirmCut,
    SelectPage(usize),
    PreviewScale(f32),
    PixelsLoaded(u64, Arc<Buffer>),
    CutSettled(u64),
    CutChecked(u64, blank::CutDetection),
    ResetCut,
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

impl App {
    fn subscription(&self) -> iced::Subscription<Message> {
        if self.error.is_none() && self.slice_editor.as_ref().is_some() {
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
                slice_editor: None,
                preview_scale: 1.0,
                status: None,
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

    fn cut_key(&self) -> Option<(u64, usize, u32)> {
        self.slice_editor
            .as_ref()
            .map(|slices| (self.load_id, slices.selected(), slices.draft()))
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
                if self.exporting.is_some() {
                    return Task::none().into();
                }
                let slices = self
                    .slice_editor
                    .as_ref()
                    .filter(|slices| slices.export_ready())?;
                let source = self.source.clone()?;
                let pixels = self.pixels.clone()?;
                let pages = slices.pages().map(|(_, region)| region).collect();
                let settings = slices.settings;
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
                self.status = format!("PDF saved: {}", result??.display()).apply(Some);
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
                self.status = Some("Loading image…".into());
                self.source = Some(handle.clone());
                self.image = None;
                self.error = None;
                self.slice_editor = None;
                self.pixels = None;
                self.undo.clear();
                self.redo.clear();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let pixels = iced::advanced::graphics::image::load(&handle)?;
                            Ok::<_, eyre::Report>(Arc::new(pixels))
                        })
                        .await
                        .map_err(eyre::Report::from)?
                        .map_err(Arc::new)
                    },
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
                    self.slice_editor = Some(SliceEditor::prepared(allocation.size(), self.output));
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
                let cut = self.slice_editor.as_ref()?.draft();
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
            Message::ResetCut => {
                self.slice_editor.as_mut()?.reset_cut();
                Task::none()
            }
            Message::Undo => {
                let previous = self.undo.pop()?;
                let current = self.slice_editor.replace(previous)?;
                self.redo.push(current);
                self.scroll_to_selected()
            }
            Message::Redo => {
                let next = self.redo.pop()?;
                let current = self.slice_editor.replace(next)?;
                self.undo.push(current);
                self.scroll_to_selected()
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
            Message::MoveGuide(end) => {
                self.status = None;
                self.slice_editor.as_mut()?.move_guide(end);
                Task::none()
            }
            Message::NudgeGuide(delta) => {
                self.status = None;
                self.slice_editor.as_mut()?.nudge(delta);
                Task::none()
            }
            Message::ConfirmCut => {
                self.status = None;
                self.remember();
                let editor = self.slice_editor.as_mut()?;
                if !editor.commit_cut() {
                    return Task::none().into();
                }
                self.scroll_to_selected()
            }
            Message::SelectPage(index) => {
                let slices = self.slice_editor.as_ref()?;
                if index == slices.selected() || !slices.can_select(index) {
                    return Task::none().into();
                }
                if slices.has_pending_cut() {
                    self.remember();
                    if !self.slice_editor.as_mut()?.commit_cut() {
                        return Task::none().into();
                    }
                }
                self.slice_editor.as_mut()?.select(index);
                self.scroll_to_selected()
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
            match (&self.image, &self.slice_editor) {
                (Some(allocation), Some(slices)) => self.picker(allocation, slices),
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
            widget::text("│").style(widget::text::secondary),
            widget::text(title).style(widget::text::secondary)
        ]
        .spacing(theme::spacing().space_s)
        .padding(theme::spacing().space_xxs)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn toolbar(&self) -> Element<'_, Message> {
        let can_export = self.exporting.is_none()
            && self.error.is_none()
            && self.source.is_some()
            && self
                .slice_editor
                .as_ref()
                .is_some_and(SliceEditor::export_ready);
        let open = self.control("Open…", Some(Message::PickImage));
        let undo = self.control("↶", (!self.undo.is_empty()).then_some(Message::Undo));
        let redo = self.control("↷", (!self.redo.is_empty()).then_some(Message::Redo));
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
            self.control("−", Some(Message::Zoom(-1))),
            widget::text(self.zoom_label()),
            self.control("＋", Some(Message::Zoom(1)))
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
            self.control("↔  Fit width", Some(Message::FitWidth)),
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
        self.slice_editor
            .as_ref()
            .map_or(self.output, |s| s.settings)
    }
    fn repaginate(&mut self, settings: layout::PageSettings) -> Task<Message> {
        self.remember();
        self.output = settings;
        if let Some(image) = &self.image {
            self.slice_editor = Some(SliceEditor::prepared(image.size(), settings));
        }
        self.scroll_to_selected()
    }
    fn zoom_label(&self) -> String {
        format!("{:.0}%", self.preview_scale * 100.0)
    }
    fn remember(&mut self) {
        if let Some(current) = &self.slice_editor {
            let mut saved = current.clone();
            saved.reset_cut();
            self.undo.push(saved);
            self.redo.clear();
        }
    }
    fn control<'a>(&self, label: &'a str, action: Option<Message>) -> Element<'a, Message> {
        widget::text(label)
            .apply(widget::button)
            .padding(theme::spacing().space_xxs)
            .on_press_maybe(action)
            .into()
    }

    fn scroll_to_selected(&self) -> Task<Message> {
        self.slice_editor
            .as_ref()
            .map_or_else(Task::none, |slices| {
                preview::scroll_to(slices.current().y, self.preview_scale)
            })
    }

    fn picker<'a>(
        &'a self,
        allocation: &'a widget::image::Allocation,
        slices: &'a SliceEditor,
    ) -> Element<'a, Message> {
        widget::row![
            sidebar::pages(allocation, slices),
            Self::editor(allocation, slices, self.zoom, self.fit_width),
            inspector::cut(allocation, slices, &self.cut_detection)
        ]
        .spacing(theme::spacing().space_xxs)
        .height(Fill)
        .into()
    }

    fn editor<'a>(
        allocation: &'a widget::image::Allocation,
        slices: &'a SliceEditor,
        zoom: f32,
        fit_width: bool,
    ) -> Element<'a, Message> {
        let header = widget::row![
            widget::text::title3("Editor"),
            widget::text("│").style(widget::text::secondary),
            widget::text::caption("Drag the line to adjust the page break")
                .style(widget::text::secondary)
        ]
        .spacing(theme::spacing().space_xs)
        .align_y(iced::Alignment::Center);
        let footer = widget::text::caption(slices.progress_label()).style(widget::text::secondary);
        widget::column![
            header,
            preview::preview(allocation, slices, zoom, fit_width),
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
    use super::*;
    #[test]
    fn undo_and_redo_restore_saved_boundaries() {
        let (mut app, _) = App::new();
        app.slice_editor = Some(SliceEditor::prepared(
            iced::Size::new(190, 700),
            layout::PageSettings::default(),
        ));
        let original = app.slice_editor.as_ref().unwrap().draft();
        let _ = app.update(Message::NudgeGuide(-1));
        let _ = app.update(Message::ConfirmCut);
        assert_eq!(app.slice_editor.as_ref().unwrap().selected(), 1);
        assert!(app.slice_editor.as_ref().unwrap().export_ready());
        let _ = app.update(Message::Undo);
        assert_eq!(app.slice_editor.as_ref().unwrap().draft(), original);
        assert!(app.slice_editor.as_ref().unwrap().export_ready());
        let _ = app.update(Message::Redo);
        app.slice_editor.as_mut().unwrap().select(0);
        assert_eq!(app.slice_editor.as_ref().unwrap().draft(), original - 1);
        assert!(app.slice_editor.as_ref().unwrap().export_ready());
    }
    #[test]
    fn switching_pages_saves_the_cut_and_keeps_the_clicked_page_selected() {
        let (mut app, _) = App::new();
        app.slice_editor = Some(SliceEditor::prepared(
            iced::Size::new(190, 700),
            layout::PageSettings::default(),
        ));
        let original = app.slice_editor.as_ref().unwrap().draft();
        let _ = app.update(Message::MoveGuide(100));
        let _ = app.update(Message::SelectPage(2));
        let slices = app.slice_editor.as_ref().unwrap();
        assert_eq!(slices.selected(), 2);
        assert_eq!(slices.pages().next().unwrap().1.rectangle.height, 100);
        assert!(slices.export_ready());
        assert_eq!(app.undo.len(), 1);
        let _ = app.update(Message::SelectPage(0));
        assert_eq!(app.slice_editor.as_ref().unwrap().draft(), 100);
        assert_eq!(app.undo.len(), 1);
        let _ = app.update(Message::Undo);
        assert_eq!(app.slice_editor.as_ref().unwrap().draft(), original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.slice_editor.as_ref().unwrap().draft(), 100);
    }

    #[test]
    fn clicking_the_current_or_invalid_page_preserves_the_draft() {
        let (mut app, _) = App::new();
        app.slice_editor = Some(SliceEditor::prepared(
            iced::Size::new(190, 700),
            layout::PageSettings::default(),
        ));
        let _ = app.update(Message::MoveGuide(150));
        let _ = app.update(Message::SelectPage(0));
        let _ = app.update(Message::SelectPage(99));
        assert_eq!(app.slice_editor.as_ref().unwrap().draft(), 150);
        assert!(app.slice_editor.as_ref().unwrap().has_pending_cut());
        assert!(app.undo.is_empty());
    }

    #[test]
    fn exporting_stays_visible_during_edits_and_rejects_duplicate_exports() {
        let (mut app, _) = App::new();
        app.slice_editor = Some(SliceEditor::prepared(
            iced::Size::new(190, 700),
            layout::PageSettings::default(),
        ));
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
        let _ = app.update(Message::NudgeGuide(-1));
        assert!(app.export_status().is_some());
        assert_eq!(app.exporting, Some(pdf::ExportProgress::PreparingDocument));
        let _ = app.update(Message::PdfExported(Ok(Some(PathBuf::from("notes.pdf")))));
        assert!(app.exporting.is_none());
        assert!(app.status.as_ref().unwrap().contains("PDF saved"));
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
        let (mut app, _) = App::new();
        app.slice_editor = Some(SliceEditor::prepared(
            iced::Size::new(190, 700),
            layout::PageSettings::default(),
        ));
        let pixels = iced::advanced::graphics::image::load(&widget::image::Handle::from_rgba(
            190,
            700,
            vec![255; 190 * 700 * 4],
        ))
        .unwrap();
        app.pixels = Some(Arc::new(pixels));
        let _ = app.update(Message::MoveGuide(200));
        let first = app.cut_check_id;
        let _ = app.update(Message::MoveGuide(150));
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
        let _ = app.update(Message::MoveGuide(150));
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
