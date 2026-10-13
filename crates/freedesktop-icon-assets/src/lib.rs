//! Resolve host SVG icons into separate assets for DX to bundle.
//!
//! The macro returns an Iced SVG widget. The calling crate needs `iced` and
//! `vse-ui`, plus `manganis` and `dioxus-cli-config` for web builds.

use proc_macro::TokenStream;
use quote::quote;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use syn::{LitStr, parse_macro_input};

/// Find a named SVG using the Cosmic theme and `COSMIC_ICONS` search paths,
/// copy it into the calling Cargo project, and return an `iced::widget::Svg`.
///
/// ```ignore
/// let add = freedesktop_icon_assets::icon!("list-add-symbolic").width(20);
/// ```
#[proc_macro]
pub fn icon(input: TokenStream) -> TokenStream {
    let name = parse_macro_input!(input as LitStr);
    let resolved = IconAsset::resolve(&name.value()).and_then(|asset| {
        let path = asset.copy_to_project()?;
        let source = asset
            .source
            .to_str()
            .ok_or("icon source path is not UTF-8")?;
        Ok((path, source.to_owned()))
    });
    match resolved {
        Ok((path, source)) => {
            let path = LitStr::new(&path, name.span());
            let source = LitStr::new(&source, name.span());
            let symbolic = name.value().ends_with("-symbolic");
            quote!({
                #[cfg(target_arch = "wasm32")]
                let svg = {
                    let asset = ::manganis::asset!(#path);
                    let base = ::dioxus_cli_config::base_path().unwrap_or_default();
                    let base = base.trim_matches('/');
                    let prefix = if base.is_empty() {
                        ::std::string::String::new()
                    } else {
                        ::std::format!("/{base}")
                    };
                    ::iced::widget::svg::Svg::from_url(::std::format!("{prefix}{asset}"))
                };
                #[cfg(not(target_arch = "wasm32"))]
                let svg = ::iced::widget::svg(#source);
                svg.width(16).height(16).style(|_: &::vse_ui::Theme, _| {
                    ::iced::widget::svg::Style {
                        color: #symbolic.then(::vse_ui::theme::icon_color),
                    }
                })
            })
            .into()
        }
        Err(error) => syn::Error::new(name.span(), error)
            .into_compile_error()
            .into(),
    }
}

struct IconAsset {
    source: PathBuf,
    name: String,
    bytes: Vec<u8>,
}

impl IconAsset {
    fn resolve(name: &str) -> Result<Self, String> {
        let extra_paths = std::env::var_os("COSMIC_ICONS")
            .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
            .unwrap_or_default();
        Self::find(name, &extra_paths)
    }

    fn find(name: &str, extra_paths: &[PathBuf]) -> Result<Self, String> {
        Self::validate_name(name)?;
        let lookup = |candidate: &str| {
            freedesktop_icons::lookup(candidate)
                .with_theme("Cosmic")
                .with_size(16)
                .with_extra_paths(extra_paths)
                .force_svg()
                .find()
        };
        let source = lookup(name)
            .or_else(|| {
                name.rmatch_indices('-')
                    .find_map(|(position, _)| lookup(&name[..position]))
            })
            .ok_or_else(|| {
                format!(
                    "SVG icon {name:?} was not found; enter `nix develop` or set COSMIC_ICONS to an icon theme's share directory"
                )
            })?;
        let bytes = fs::read(&source)
            .map_err(|error| format!("cannot read icon {}: {error}", source.display()))?;
        Ok(Self {
            source,
            name: name.to_owned(),
            bytes,
        })
    }

    fn validate_name(name: &str) -> Result<(), String> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || name == "."
            || name == ".."
        {
            return Err("expected a Freedesktop icon name, without directory separators".into());
        }
        Ok(())
    }

    fn copy_to_project(&self) -> Result<String, String> {
        let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .ok_or("CARGO_MANIFEST_DIR is not set")?;
        self.copy_into_project(&manifest)
    }

    fn copy_into_project(&self, manifest: &Path) -> Result<String, String> {
        let digest = Sha256::digest(&self.bytes);
        let relative = format!("assets/generated/icons/{}-{digest:x}.svg", self.name);
        let directory = manifest.join("assets/generated/icons");
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let manifest = manifest.canonicalize().map_err(|error| error.to_string())?;
        let directory = directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !directory.starts_with(&manifest) {
            return Err("generated assets directory must stay inside the Cargo project".into());
        }
        self.write_if_changed(&manifest.join(&relative))
            .map_err(|error| format!("cannot copy icon into {}: {error}", manifest.display()))?;
        Ok(format!("/{relative}"))
    }

    fn write_if_changed(&self, destination: &Path) -> std::io::Result<()> {
        if fs::read(destination).ok().as_deref() == Some(self.bytes.as_slice()) {
            return Ok(());
        }
        let directory = destination.parent().expect("icon path has a parent");
        fs::create_dir_all(directory)?;
        // Publish only complete files, including when rustc processes repeat a lookup.
        static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
        let temporary = directory.join(format!(
            ".icon-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&self.bytes)?;
            drop(file);
            fs::rename(&temporary, destination)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::IconAsset;
    use std::fs;

    #[test]
    fn rejects_paths_instead_of_icon_names() {
        for name in ["", ".", "..", "../icon", "icons/icon", "icons\\icon"] {
            assert!(IconAsset::validate_name(name).is_err(), "{name:?}");
        }
    }

    #[test]
    fn fallback_lookup_requires_svg_even_when_png_exists() {
        let theme = tempfile::tempdir().unwrap();
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
        fs::write(theme.path().join("pagecut-test-icon.svg"), svg).unwrap();
        fs::write(theme.path().join("pagecut-test-icon.png"), b"not an SVG").unwrap();
        let asset =
            IconAsset::find("pagecut-test-icon-symbolic", &[theme.path().to_path_buf()]).unwrap();
        assert_eq!(asset.bytes, svg);
        assert_eq!(asset.name, "pagecut-test-icon-symbolic");
    }

    #[test]
    fn content_changes_get_distinct_paths_without_overwriting_old_assets() {
        let project = tempfile::tempdir().unwrap();
        let original = IconAsset {
            source: Default::default(),
            name: "test-symbolic".into(),
            bytes: b"original".to_vec(),
        };
        let updated = IconAsset {
            source: Default::default(),
            name: original.name.clone(),
            bytes: b"updated".to_vec(),
        };
        let first = original.copy_into_project(project.path()).unwrap();
        assert_eq!(first, original.copy_into_project(project.path()).unwrap());
        let second = updated.copy_into_project(project.path()).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            fs::read(project.path().join(first.trim_start_matches('/'))).unwrap(),
            original.bytes,
        );
        assert_eq!(
            fs::read(project.path().join(second.trim_start_matches('/'))).unwrap(),
            updated.bytes,
        );
    }
}
