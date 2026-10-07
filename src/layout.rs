//! Physical page geometry shared by editor, preview, and PDF export.
use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Paper {
    #[default]
    A4,
    A5,
    Letter,
}
impl Paper {
    pub const ALL: [Self; 3] = [Self::A4, Self::A5, Self::Letter];
    fn dimensions(self) -> (f64, f64) {
        match self {
            Self::A4 => (210.0, 297.0),
            Self::A5 => (148.0, 210.0),
            Self::Letter => (215.9, 279.4),
        }
    }
}
impl fmt::Display for Paper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::A4 => "A4",
            Self::A5 => "A5",
            Self::Letter => "Letter",
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}
impl Orientation {
    pub const ALL: [Self; 2] = [Self::Portrait, Self::Landscape];
}
impl fmt::Display for Orientation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Portrait => "Portrait",
            Self::Landscape => "Landscape",
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Margin(pub u8);
impl Margin {
    pub const ALL: [Self; 5] = [Self(0), Self(5), Self(10), Self(15), Self(20)];
}
impl fmt::Display for Margin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Margins  {} mm", self.0)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSettings {
    pub paper: Paper,
    pub orientation: Orientation,
    pub margin: Margin,
}
impl Default for PageSettings {
    fn default() -> Self {
        Self {
            paper: Paper::A4,
            orientation: Orientation::Portrait,
            margin: Margin(10),
        }
    }
}
impl PageSettings {
    pub fn dimensions(self) -> (f64, f64) {
        let (width, height) = self.paper.dimensions();
        match self.orientation {
            Orientation::Portrait => (width, height),
            Orientation::Landscape => (height, width),
        }
    }
    pub fn height(self, source_width: u32) -> u32 {
        let (width, height) = self.dimensions();
        let margin = f64::from(self.margin.0) * 2.0;
        (f64::from(source_width) * (height - margin) / (width - margin))
            .floor()
            .max(1.0) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orientation_and_margins_change_the_source_pixel_limit() {
        let mut settings = PageSettings::default();
        assert_eq!(settings.height(190), 277);
        settings.orientation = Orientation::Landscape;
        assert_eq!(settings.height(277), 190);
        settings.margin = Margin(0);
        assert_eq!(settings.height(297), 210);
        settings.paper = Paper::Letter;
        assert_eq!(settings.dimensions(), (279.4, 215.9));
    }
}
