//! Chunked reads and image decoding for the native and browser executors.
use eyre::{Context, Result};
use std::io::{Seek, SeekFrom};
use std::sync::Arc;
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
    progress: impl FnMut(ReadProgress) + Send + 'static,
) -> Result<Vec<u8>> {
    let read = move || {
        let mut reader = file.open_read().wrap_err("Cannot open image")?;
        let total = reader.seek(SeekFrom::End(0))?;
        reader.rewind()?;
        read_chunks(&mut reader, total, progress)
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::task::spawn_blocking(read).await?
    }
    // The browser picker has already loaded the file into memory.
    #[cfg(target_arch = "wasm32")]
    {
        read()
    }
}

fn read_chunks(
    reader: &mut impl std::io::Read,
    total: u64,
    mut progress: impl FnMut(ReadProgress),
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(usize::try_from(total)?)?;
    let mut chunk = vec![0; READ_CHUNK];
    progress(ReadProgress {
        completed: 0,
        total,
    });
    loop {
        let count = reader.read(&mut chunk).wrap_err("Cannot read image")?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        progress(ReadProgress {
            completed: bytes.len() as u64,
            total,
        });
    }
    Ok(bytes)
}

pub async fn decode(handle: Handle) -> Result<Arc<Buffer>> {
    crate::background::run("image-decode", move || {
        vse_ui::iced::advanced::graphics::image::load(&handle)
            .map(Arc::new)
            .map_err(eyre::Report::from)
    })
    .await
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reading_reports_bytes_and_preserves_the_complete_file() {
        let original = vec![37; READ_CHUNK * 2 + 17];
        let mut updates = Vec::new();
        let bytes = read_chunks(
            &mut Cursor::new(&original),
            original.len() as u64,
            |progress| updates.push(progress),
        )
        .unwrap();
        assert_eq!(bytes, original);
        assert_eq!(updates.first().unwrap().completed, 0);
        assert_eq!(updates.last().unwrap().completed, original.len() as u64);
        assert!(
            updates
                .windows(2)
                .all(|pair| pair[0].completed < pair[1].completed)
        );
        assert_eq!(updates.last().unwrap().fraction(), 1.0);
    }

    #[test]
    fn read_errors_propagate_without_reporting_completion() {
        struct Failing;
        impl std::io::Read for Failing {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("read failed"))
            }
        }
        let mut updates = Vec::new();
        assert!(read_chunks(&mut Failing, 8, |progress| updates.push(progress)).is_err());
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].completed, 0);
    }
}
