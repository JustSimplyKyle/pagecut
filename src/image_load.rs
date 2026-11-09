//! Chunked reads and image decoding for the native and browser executors.
use eyre::{Context, Result};
use futures::io::{BufReader, Cursor};
use futures::{AsyncReadExt, AsyncSeekExt, StreamExt, TryStreamExt};
use pdf_writer::Finish;
use std::io::SeekFrom;
use std::sync::Arc;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::io::ReaderStream;
use vse_ui::ApplyConditional;
use vse_ui::iced::{advanced::graphics::image::Buffer, widget::image::Handle};

const READ_CHUNK: usize = 1024 * 1024;

/// Image formats the picker offers.
#[cfg(not(target_os = "android"))]
pub const MIME_TYPES: &[&str] = &["image/png", "image/jpeg"];
/// Android's document picker filters on a single MIME type.
#[cfg(target_os = "android")]
pub const MIME_TYPES: &[&str] = &["image/*"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadProgress {
    pub completed: u64,
    pub total: u64,
}

impl ReadProgress {
    pub fn fraction(self) -> f32 {
        if self.total == 0 {
            1.0
        } else {
            (self.completed as f64 / self.total as f64).clamp(0.0, 1.0) as f32
        }
    }
    pub fn label(self) -> String {
        format!("Reading image — {:.0}%", self.fraction() * 100.0)
    }
}

pub async fn read(
    file: loki_file_access::FileAccessToken,
    mut progress: impl FnMut(ReadProgress) + Send + 'static,
) -> Result<Vec<u8>> {
    let read = async move || {
        let mut reader = futures::io::AllowStdIo::new(file.open_read()?);
        let total = reader.seek(SeekFrom::End(0)).await?;
        std::io::Seek::rewind(&mut reader)?;
        let stream = ReaderStream::new(reader.compat());
        Ok(stream
            .try_fold(
                Vec::with_capacity(total as usize),
                move |mut bytes, chunk| {
                    bytes.extend_from_slice(&chunk);
                    progress(ReadProgress {
                        completed: bytes.len() as u64,
                        total,
                    });
                    std::future::ready(Ok(bytes))
                },
            )
            .await?)
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        use futures::executor::block_on;
        use vse_ui::Apply;

        tokio::task::spawn_blocking(|| read().apply(block_on)).await?
    }
    // The browser picker has already loaded the file into memory.
    #[cfg(target_arch = "wasm32")]
    {
        read().await
    }
}

pub async fn decode(handle: Handle) -> Result<Arc<Buffer>> {
    crate::background::run("image-decode", move || {
        vse_ui::iced::advanced::graphics::image::load(&handle)
            .map(Arc::new)
            .map_err(eyre::Report::from)
    })
    .await
}
