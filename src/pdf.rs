use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eyre::{Context, Result, ensure};
use flate2::{Compression, write::ZlibEncoder};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};
use vse_ui::iced::{Rectangle, advanced::graphics::image::Buffer, widget::image::Handle};

use crate::{layout::PageSettings, slices::Slice};

#[cfg(test)]
use vse_ui::iced::advanced::graphics::image as raster;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportProgress {
    ChoosingDestination,
    PreparingDocument,
    Pages { completed: usize, total: usize },
    SavingFile,
}

impl ExportProgress {
    pub fn label(&self) -> String {
        match self {
            Self::ChoosingDestination => "Choose where to save your PDF…".into(),
            Self::PreparingDocument => "Exporting PDF — preparing document…".into(),
            Self::Pages { completed, total } => {
                format!("Exporting PDF — {completed} of {total} pages processed…")
            }
            Self::SavingFile => "Exporting PDF — saving file…".into(),
        }
    }
    pub fn fraction(&self) -> Option<f32> {
        match self {
            Self::Pages { completed, total } => Some(*completed as f32 / (*total).max(1) as f32),
            _ => None,
        }
    }
    pub fn button_label(&self) -> &'static str {
        match self {
            Self::ChoosingDestination => "Choose destination…",
            _ => "Exporting PDF…",
        }
    }
}

pub async fn choose_and_export(
    source: Handle,
    pixels: Arc<Buffer>,
    pages: Vec<Slice>,
    settings: PageSettings,
    progress: impl Fn(ExportProgress) + Send + 'static,
) -> Result<Option<PathBuf>> {
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title("Export PDF")
        .add_filter("PDF", &["pdf"])
        .set_file_name(default_name(&source));
    if let Handle::Path(_, path) = &source
        && let Some(parent) = path.parent()
    {
        dialog = dialog.set_directory(parent);
    }
    let Some(file) = dialog.save_file().await else {
        return Ok(None);
    };
    let mut target = file.path().to_path_buf();
    if target.extension().is_none() {
        target.set_extension("pdf");
    }
    tokio::task::spawn_blocking(move || {
        write_with_progress(&source, &pixels, &pages, &target, settings, &progress)?;
        Ok(Some(target))
    })
    .await
    .wrap_err("PDF export worker failed")?
}

fn default_name(source: &Handle) -> String {
    let stem = match source {
        Handle::Path(_, path) => path.file_stem().unwrap_or_default().to_string_lossy(),
        _ => "image".into(),
    };
    format!("{stem}.pdf")
}

#[cfg(test)]
fn write(source: &Handle, pages: &[Slice], target: &Path) -> Result<()> {
    write_with_settings(source, pages, target, PageSettings::default())
}
#[cfg(test)]
fn write_with_settings(
    source: &Handle,
    pages: &[Slice],
    target: &Path,
    settings: PageSettings,
) -> Result<()> {
    let pixels = raster::load(source)?;
    write_with_progress(source, &pixels, pages, target, settings, &|_| {})
}

fn write_with_progress(
    source: &Handle,
    pixels: &Buffer,
    pages: &[Slice],
    target: &Path,
    settings: PageSettings,
    progress: &impl Fn(ExportProgress),
) -> Result<()> {
    progress(ExportProgress::PreparingDocument);
    if let Handle::Path(_, path) = source {
        if target.exists() {
            ensure!(
                path.canonicalize()? != target.canonicalize()?,
                "The PDF must not replace the source image."
            );
        }
    }
    let pdf = render_with_progress(pixels, pages, settings, progress)?;
    progress(ExportProgress::SavingFile);
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).wrap_err("Could not create the PDF")?;
    temporary
        .write_all(&pdf)
        .wrap_err("Could not write the PDF")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(target)
        .map_err(|error| error.error)
        .wrap_err("Could not save the PDF")?;
    Ok(())
}

#[cfg(test)]
fn render(pixels: &Buffer, pages: &[Slice]) -> Result<Vec<u8>> {
    render_with_settings(pixels, pages, PageSettings::default())
}
#[cfg(test)]
fn render_with_settings(
    pixels: &Buffer,
    pages: &[Slice],
    settings: PageSettings,
) -> Result<Vec<u8>> {
    render_with_progress(pixels, pages, settings, &|_| {})
}

fn render_with_progress(
    pixels: &Buffer,
    pages: &[Slice],
    settings: PageSettings,
    progress: &impl Fn(ExportProgress),
) -> Result<Vec<u8>> {
    validate_pages(pixels, pages, settings)?;
    let layout = PageLayout::configured(pixels.width(), settings);
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let page_refs: Vec<_> = (0..pages.len())
        .map(|index| Ref::new(3 + index as i32 * 3))
        .collect();
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(page_refs.iter().copied())
        .count(pages.len() as i32);

    progress(ExportProgress::Pages {
        completed: 0,
        total: pages.len(),
    });
    for (index, (slice, page_ref)) in pages.iter().zip(page_refs).enumerate() {
        let region = slice.region();
        let image_ref = Ref::new(page_ref.get() + 1);
        let content_ref = Ref::new(page_ref.get() + 2);
        let name = Name(b"Image");
        let mut page = pdf.page(page_ref);
        page.parent(tree)
            .media_box(layout.media_box())
            .contents(content_ref);
        page.resources().x_objects().pair(name, image_ref);
        page.finish();

        let compressed = compress_slice(pixels, region)?;
        let mut image = pdf.image_xobject(image_ref, &compressed);
        image.filter(Filter::FlateDecode);
        image
            .width(region.width as i32)
            .height(region.height as i32)
            .bits_per_component(8);
        image.color_space().device_rgb();
        image.finish();

        let mut content = Content::new();
        content.save_state();
        content.transform(layout.image_transform(region.height));
        content.x_object(name);
        content.restore_state();
        pdf.stream(content_ref, &content.finish());
        progress(ExportProgress::Pages {
            completed: index + 1,
            total: pages.len(),
        });
    }
    Ok(pdf.finish())
}

fn validate_pages(pixels: &Buffer, pages: &[Slice], settings: PageSettings) -> Result<()> {
    ensure!(!pages.is_empty(), "There are no confirmed pages to export.");
    ensure!(
        pages.len() <= (i32::MAX as usize - 3) / 3,
        "Too many PDF pages."
    );
    ensure!(
        pixels.width() > 0 && pixels.width() <= i32::MAX as u32,
        "Invalid image width."
    );
    let mut next_y = 0;
    for slice in pages {
        let page = slice.region();
        ensure!(
            page.x == 0
                && page.width == pixels.width()
                && page.y == next_y
                && page.height > 0
                && page.height <= settings.height(pixels.width()),
            "The confirmed cuts are invalid or do not fit the page."
        );
        next_y = next_y
            .checked_add(page.height)
            .ok_or_else(|| eyre::eyre!("Invalid page height"))?;
        ensure!(
            next_y <= pixels.height(),
            "A cut is outside the source image."
        );
    }
    ensure!(
        next_y == pixels.height(),
        "The pages must cover the entire image."
    );
    Ok(())
}

fn compress_slice(pixels: &Buffer, region: Rectangle<u32>) -> Result<Vec<u8>> {
    let mut encoded = ZlibEncoder::new(Vec::new(), Compression::default());
    let mut row = Vec::with_capacity(region.width as usize * 3);
    for y in region.y..region.y + region.height {
        row.clear();
        for x in 0..region.width {
            let [r, g, b, a] = pixels.get_pixel(x, y).0;
            row.extend([over_white(r, a), over_white(g, a), over_white(b, a)]);
        }
        encoded.write_all(&row)?;
    }
    Ok(encoded.finish()?)
}

fn over_white(channel: u8, alpha: u8) -> u8 {
    ((u32::from(channel) * u32::from(alpha) + 255 * (255 - u32::from(alpha)) + 127) / 255) as u8
}

struct PageLayout {
    width: f32,
    height: f32,
    margin: f32,
    image_width: f32,
    source_scale: f32,
}

impl PageLayout {
    #[cfg(test)]
    fn new(source_width: u32) -> Self {
        Self::configured(source_width, PageSettings::default())
    }
    fn configured(source_width: u32, settings: PageSettings) -> Self {
        const POINTS_PER_MM: f64 = 72.0 / 25.4;
        let (width_mm, height_mm) = settings.dimensions();
        let width = (width_mm * POINTS_PER_MM) as f32;
        let height = (height_mm * POINTS_PER_MM) as f32;
        let margin = (f64::from(settings.margin.0) * POINTS_PER_MM) as f32;
        let image_width = width - 2.0 * margin;
        Self {
            width,
            height,
            margin,
            image_width,
            source_scale: image_width / source_width as f32,
        }
    }
    fn media_box(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width, self.height)
    }
    fn image_transform(&self, source_height: u32) -> [f32; 6] {
        let height = source_height as f32 * self.source_scale;
        [
            self.image_width,
            0.0,
            0.0,
            height,
            self.margin,
            self.height - self.margin - height,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn source() -> Handle {
        let mut pixels = Vec::new();
        for y in 0..400 {
            let color = if y < 200 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            for _ in 0..190 {
                pixels.extend(color);
            }
        }
        Handle::from_rgba(190, 400, pixels)
    }
    fn pages() -> Vec<Slice> {
        vec![
            Slice {
                y_offset: 0,
                rectangle: Rectangle {
                    x: 0,
                    y: 0,
                    width: 190,
                    height: 200,
                },
            },
            Slice {
                y_offset: 200,
                rectangle: Rectangle {
                    x: 0,
                    y: 0,
                    width: 190,
                    height: 200,
                },
            },
        ]
    }

    #[test]
    fn exports_two_full_resolution_a4_pages_and_exact_slice_pixels() {
        let pixels = raster::load(&source()).unwrap();
        let pages = pages();
        let pdf = render(&pixels, &pages).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(text.contains("/Count 2"));
        assert_eq!(text.matches("/Type /Page\n").count(), 2);
        assert!(text.contains("/Width 190"));
        assert!(text.contains("/Height 200"));
        for (page, expected) in pages.into_iter().zip([[255, 0, 0], [0, 0, 255]]) {
            let compressed = compress_slice(&pixels, page.region()).unwrap();
            let mut raw = Vec::new();
            flate2::read::ZlibDecoder::new(compressed.as_slice())
                .read_to_end(&mut raw)
                .unwrap();
            assert_eq!(raw.len(), 190 * 200 * 3);
            assert!(raw.chunks_exact(3).all(|p| p == expected));
        }
        let layout = PageLayout::new(190);
        assert!((layout.width - 595.2756).abs() < 0.001);
        assert!((layout.height - 841.8898).abs() < 0.001);
        let transform = layout.image_transform(200);
        assert!((transform[4] - 28.34646).abs() < 0.001);
        assert!(transform[5] >= layout.margin);
    }

    #[test]
    fn rejects_incomplete_overlapping_or_oversized_pages() {
        let pixels = raster::load(&source()).unwrap();
        assert!(render(&pixels, &pages()[..1]).is_err());
        let mut invalid = pages();
        invalid[1].y_offset = 199;
        assert!(render(&pixels, &invalid).is_err());
        invalid[0].rectangle.height = 300;
        assert!(render(&pixels, &invalid).is_err());
    }

    #[test]
    fn export_uses_the_selected_paper_orientation_and_margins() {
        let settings = PageSettings {
            paper: crate::layout::Paper::A5,
            orientation: crate::layout::Orientation::Landscape,
            margin: crate::layout::Margin(5),
        };
        let editor =
            crate::slices::SliceEditor::prepared(vse_ui::iced::Size::new(190, 400), settings);
        let pages: Vec<_> = editor.pages().map(|(_, page)| page).collect();
        let pixels = raster::load(&source()).unwrap();
        let pdf = render_with_settings(&pixels, &pages, settings).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let layout = PageLayout::configured(190, settings);
        assert!((layout.width - 595.2756).abs() < 0.001);
        assert!((layout.height - 419.52756).abs() < 0.001);
        for page in pages {
            assert!(layout.image_transform(page.rectangle.height)[5] >= layout.margin);
        }
    }

    #[test]
    fn alpha_is_composited_on_white() {
        assert_eq!(over_white(0, 0), 255);
        assert_eq!(over_white(0, 128), 127);
        assert_eq!(over_white(25, 255), 25);
    }

    #[test]
    fn export_reports_actual_completed_pages_and_the_file_saving_stage() {
        use std::cell::RefCell;
        let stages = RefCell::new(Vec::new());
        let dir = tempfile::tempdir().unwrap();
        write_with_progress(
            &source(),
            &raster::load(&source()).unwrap(),
            &pages(),
            &dir.path().join("progress.pdf"),
            PageSettings::default(),
            &|stage| stages.borrow_mut().push(stage),
        )
        .unwrap();
        assert_eq!(
            stages.into_inner(),
            vec![
                ExportProgress::PreparingDocument,
                ExportProgress::Pages {
                    completed: 0,
                    total: 2
                },
                ExportProgress::Pages {
                    completed: 1,
                    total: 2
                },
                ExportProgress::Pages {
                    completed: 2,
                    total: 2
                },
                ExportProgress::SavingFile,
            ]
        );
    }

    #[test]
    fn writes_an_atomic_pdf_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pages.pdf");
        std::fs::write(&path, b"previous PDF").unwrap();
        assert!(write(&source(), &pages()[..1], &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous PDF");
        write(&source(), &pages(), &path).unwrap();
        assert!(std::fs::read(path).unwrap().starts_with(b"%PDF-"));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn exports_loaded_pixels_even_if_the_source_changes_or_disappears() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.png");
        let pixels = raster::load(&source()).unwrap();
        pixels.save(&path).unwrap();
        let handle = Handle::from_path(&path);
        let loaded = raster::load(&handle).unwrap();
        let expected = render(&loaded, &pages()).unwrap();

        std::fs::write(&path, b"changed source").unwrap();
        for name in ["changed.pdf", "deleted.pdf"] {
            let target = dir.path().join(name);
            write_with_progress(
                &handle,
                &loaded,
                &pages(),
                &target,
                PageSettings::default(),
                &|_| {},
            )
            .unwrap();
            assert_eq!(std::fs::read(target).unwrap(), expected);
            if path.exists() {
                assert_eq!(std::fs::read(&path).unwrap(), b"changed source");
                std::fs::remove_file(&path).unwrap();
            }
        }
    }

    #[test]
    fn exporting_cannot_replace_the_source_image() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.png");
        raster::load(&source()).unwrap().save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(write(&Handle::from_path(&path), &pages(), &path).is_err());
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
}
