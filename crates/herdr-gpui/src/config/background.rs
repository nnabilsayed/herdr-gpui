//! A picture behind the whole window: the sidebar, the terminal, and the rest
//! of the chrome paint translucent over it, with a dimming veil between so text
//! stays readable on a busy photo.
use super::{Config, LOCAL_CONFIG, write_config};
use crate::{Error, Result};
use serde::Deserialize;
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

/// The darkest veil. Fully black would hide the picture it is meant to frame.
pub const MAX_DIM: f32 = 0.9;
/// The most see-through the panels get; below this the text loses its ground.
pub const MIN_PANEL_OPACITY: f32 = 0.2;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Background {
    /// The picture, shown only when the file exists and can be decoded.
    pub image: Option<PathBuf>,
    /// How dark the veil over the picture is, from 0 (none) to [`MAX_DIM`].
    pub dim: f32,
    /// How solid the panels over the picture are, from [`MIN_PANEL_OPACITY`]
    /// to 1 (opaque, hiding it).
    pub panel_opacity: f32,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            image: None,
            dim: 0.45,
            panel_opacity: 0.55,
        }
    }
}

impl Background {
    pub(super) fn validate(&self) -> Result<()> {
        if !self.dim.is_finite() || !(0.0..=MAX_DIM).contains(&self.dim) {
            return Err(Error::InvalidBackground("dim", 0., MAX_DIM));
        }
        if !self.panel_opacity.is_finite()
            || !(MIN_PANEL_OPACITY..=1.).contains(&self.panel_opacity)
        {
            return Err(Error::InvalidBackground(
                "panel_opacity",
                MIN_PANEL_OPACITY,
                1.,
            ));
        }
        if self
            .image
            .as_deref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(Error::EmptyBackgroundImage);
        }
        Ok(())
    }

    /// Whether a picture is set and its file can be decoded. Reads only the
    /// image header, and runs when the theme is built, not per frame.
    pub fn loads(&self) -> bool {
        self.image.as_deref().is_some_and(|path| {
            image::ImageReader::open(path)
                .and_then(|reader| reader.with_guessed_format())
                .is_ok_and(|reader| reader.into_dimensions().is_ok())
        })
    }
}

impl Config {
    /// Persist one `[background]` key, keeping the rest of the local file.
    pub(crate) fn save_background(edit: Edit) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_background_path(&edit, &local)
    }

    pub(super) fn save_background_path(edit: &Edit, path: &Path) -> Result<()> {
        let result = (|| -> Result<()> {
            let (key, value) = edit.entry()?;
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
                Err(error) => return Err(error.into()),
            };
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            let table = document
                .entry("background")
                .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                .as_table_like_mut()
                .ok_or(crate::herdr_settings::Error::Table("background"))?;
            match value {
                Some(mut value) => {
                    if let Some(previous) = table.get(key).and_then(toml_edit::Item::as_value) {
                        *value.decor_mut() = previous.decor().clone();
                    }
                    table.insert(key, toml_edit::Item::Value(value));
                }
                None => {
                    table.remove(key);
                }
            }
            write_config(path, &document.to_string())
        })();
        result.map_err(|error| error.at_path(path))
    }
}

/// One `[background]` key, as the Settings window saves it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Edit {
    Image(Option<PathBuf>),
    Dim(f32),
    PanelOpacity(f32),
}

impl Edit {
    /// The key and its new value; `None` removes the key.
    pub(super) fn entry(&self) -> Result<(&'static str, Option<toml_edit::Value>)> {
        let mut candidate = Background::default();
        let entry = match self {
            Self::Image(path) => {
                candidate.image = path.clone();
                (
                    "image",
                    path.as_ref()
                        .map(|path| toml_edit::Value::from(path.to_string_lossy().as_ref())),
                )
            }
            Self::Dim(value) => {
                candidate.dim = *value;
                // Written as a rounded number so a slider step reads cleanly.
                (
                    "dim",
                    Some(toml_edit::Value::from(
                        (f64::from(*value) * 100.).round() / 100.,
                    )),
                )
            }
            Self::PanelOpacity(value) => {
                candidate.panel_opacity = *value;
                (
                    "panel_opacity",
                    Some(toml_edit::Value::from(
                        (f64::from(*value) * 100.).round() / 100.,
                    )),
                )
            }
        };
        candidate.validate()?;
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Theme;

    #[test]
    fn panels_are_solid_unless_the_picture_loads() -> anyhow::Result<()> {
        let theme = |background: &Background| Theme::default().with_background(background);
        let mut background = Background::default();
        assert_eq!(theme(&background).panel_alpha, 1.);
        // Missing file: no veil, no translucency.
        background.image = Some("does-not-exist.png".into());
        assert!(!theme(&background).backdrop);
        assert_eq!(theme(&background).panel_alpha, 1.);
        // Present but not an image.
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("broken.png");
        fs::write(&path, "not an image")?;
        background.image = Some(path);
        assert!(!theme(&background).backdrop);
        assert_eq!(theme(&background).panel_alpha, 1.);
        // A real picture.
        let path = directory.path().join("ok.png");
        image::RgbaImage::new(2, 2).save(&path)?;
        background.image = Some(path);
        assert!(theme(&background).backdrop);
        assert_eq!(theme(&background).panel_alpha, 0.55);
        Ok(())
    }

    #[test]
    fn saved_keys_round_trip_and_keep_the_rest_of_the_file() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("local.toml");
        fs::write(
            &path,
            "# mine
layout = 'orca'
",
        )?;
        Config::save_background_path(&Edit::Image(Some("C:/pics/a.png".into())), &path)?;
        Config::save_background_path(&Edit::Dim(0.6), &path)?;
        Config::save_background_path(&Edit::PanelOpacity(0.3), &path)?;
        let text = fs::read_to_string(&path)?;
        assert!(text.contains("# mine"));
        let config = Config::parse(&text)?;
        assert_eq!(config.background.image, Some("C:/pics/a.png".into()));
        assert_eq!(config.background.dim, 0.6);
        assert_eq!(config.background.panel_opacity, 0.3);
        Config::save_background_path(&Edit::Image(None), &path)?;
        let config = Config::parse(&fs::read_to_string(&path)?)?;
        assert_eq!(config.background.image, None);
        assert!(!config.background.loads());
        Ok(())
    }

    #[test]
    fn out_of_range_values_are_refused() {
        for (dim, panel_opacity) in [(0.95, 0.5), (-0.1, 0.5), (0.4, 0.1), (0.4, 1.5)] {
            let background = Background {
                dim,
                panel_opacity,
                ..Background::default()
            };
            assert!(background.validate().is_err(), "{dim} {panel_opacity}");
        }
        assert!(Background::default().validate().is_ok());
        assert!(Edit::Dim(f32::NAN).entry().is_err());
        assert!(Edit::PanelOpacity(0.0).entry().is_err());
        assert!(Edit::Image(Some(PathBuf::new())).entry().is_err());
    }
}
