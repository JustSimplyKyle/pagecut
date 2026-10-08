//! Perceptual cut-row inspection performed by a blocking worker, never by the UI.
use std::{sync::Arc, time::Duration};
use vse_ui::iced::advanced::graphics::image::Buffer;

pub const SETTLE_DELAY: Duration = Duration::from_millis(500);
// Maximum spread in normalized Oklab space. Tune detection here, independently of the UI.
const MAX_COLOR_SPREAD: f32 = 0.05;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CutDetection {
    #[default]
    Waiting,
    Checking,
    Uniform,
    Mixed,
    ImageEnd,
    Failed(String),
}
impl CutDetection {
    pub fn label(&self) -> String {
        match self {
            Self::Waiting => "Waiting for the cut to stop moving…".into(),
            Self::Checking => "Checking cut line…".into(),
            Self::Uniform => "Cut appears clear".into(),
            Self::Mixed => "This cut may cross content".into(),
            Self::ImageEnd => "End of image — no content below this cut".into(),
            Self::Failed(reason) => format!("Could not check cut: {reason}"),
        }
    }
}

pub async fn settled(check_id: u64) -> u64 {
    tokio::time::sleep(SETTLE_DELAY).await;
    check_id
}

pub async fn check(pixels: Arc<Buffer>, cut: u32) -> CutDetection {
    match tokio::task::spawn_blocking(move || inspect(&pixels, cut)).await {
        Ok(result) => result,
        Err(error) => CutDetection::Failed(error.to_string()),
    }
}

fn inspect(pixels: &Buffer, cut: u32) -> CutDetection {
    if cut == pixels.height() {
        return CutDetection::ImageEnd;
    }
    if cut > pixels.height() || pixels.width() == 0 {
        return CutDetection::Failed("Cut is outside the image".into());
    }
    let row_bytes = pixels.width() as usize * 4;
    let offset = cut as usize * row_bytes;
    inspect_line(
        &pixels.as_raw()[offset..offset + row_bytes],
        MAX_COLOR_SPREAD,
    )
}

fn inspect_line(row: &[u8], tolerance: f32) -> CutDetection {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for pixel in row.as_chunks::<4>().0 {
        let color = oklab(pixel);
        for channel in 0..3 {
            min[channel] = min[channel].min(color[channel]);
            max[channel] = max[channel].max(color[channel]);
        }
        // The bounding-box diagonal bounds the perceptual distance between any two pixels.
        let spread_squared: f32 = (0..3).map(|c| (max[c] - min[c]).powi(2)).sum();
        if spread_squared > tolerance * tolerance {
            return CutDetection::Mixed;
        }
    }
    CutDetection::Uniform
}

/// Composite transparency on white, then convert sRGB to perceptual Oklab coordinates.
fn oklab(pixel: &[u8]) -> [f32; 3] {
    let alpha = f32::from(pixel[3]) / 255.0;
    let [r, g, b] = std::array::from_fn(|channel| {
        let srgb = f32::from(pixel[channel]) / 255.0 * alpha + (1.0 - alpha);
        if srgb <= 0.04045 {
            srgb / 12.92
        } else {
            ((srgb + 0.055) / 1.055).powf(2.4)
        }
    });
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_76 * m - 0.808_675_77 * s,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use vse_ui::iced::{advanced::graphics::image::load, widget::image::Handle};
    #[test]
    fn waiting_is_async_and_the_worker_reports_the_requested_row() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            let start = std::time::Instant::now();
            assert_eq!(settled(7).await, 7);
            assert!(start.elapsed() >= SETTLE_DELAY);
            let pixels = load(&Handle::from_rgba(
                2,
                1,
                vec![0, 0, 0, 255, 255, 255, 255, 255],
            ))
            .unwrap();
            assert_eq!(check(Arc::new(pixels), 0).await, CutDetection::Mixed);
        });
    }

    #[test]
    fn accepts_the_measured_paper_background_but_rejects_ink() {
        let background: Vec<u8> = (229..=238)
            .flat_map(|gray| [gray, gray, gray, 255])
            .collect();
        assert_eq!(
            inspect_line(&background, MAX_COLOR_SPREAD),
            CutDetection::Uniform
        );
        let mut ink = background.clone();
        ink.extend([24, 23, 18, 255]);
        assert_eq!(inspect_line(&ink, MAX_COLOR_SPREAD), CutDetection::Mixed);
        assert_eq!(inspect_line(&background, 0.0), CutDetection::Mixed);
    }

    #[test]
    fn preserves_color_contrast_and_compares_transparency_as_displayed() {
        // These colors have similar luminance but visibly different hues.
        assert_eq!(
            inspect_line(&[255, 0, 0, 255, 0, 148, 0, 255], MAX_COLOR_SPREAD),
            CutDetection::Mixed
        );
        assert_eq!(
            inspect_line(&[255, 255, 255, 255, 0, 0, 0, 0], MAX_COLOR_SPREAD),
            CutDetection::Uniform
        );
        assert_eq!(
            inspect_line(&[235, 235, 235, 255, 235, 235, 235, 254], MAX_COLOR_SPREAD),
            CutDetection::Uniform
        );
    }

    #[test]
    fn checks_only_the_requested_row_and_handles_the_image_end() {
        let data = vec![
            10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255, 255, 255, 255, 255, 255, 255, 254,
            255, 255, 255, 255, 255, 10, 20, 30, 255, 10, 20, 30, 254, 10, 20, 30, 255,
        ];
        let pixels = load(&Handle::from_rgba(3, 3, data)).unwrap();
        assert_eq!(inspect(&pixels, 0), CutDetection::Uniform);
        assert_eq!(inspect(&pixels, 1), CutDetection::Uniform);
        assert_eq!(inspect(&pixels, 2), CutDetection::Uniform);
        assert_eq!(inspect(&pixels, 3), CutDetection::ImageEnd);
        assert!(matches!(inspect(&pixels, 4), CutDetection::Failed(_)));
    }
}
