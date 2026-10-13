//! Reader preferences: appearance, themes, typography and the workspace.
//! Saved as JSON in the user's config folder whenever they change.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use volt_ui_kit::{ColorToken as Role, Palette, parse_hex};

/// Light, dark, or whatever the system uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

/// A colour theme. Light themes and dark themes are chosen separately, so
/// following the system switches between the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Paper,
    Sepia,
    Mist,
    Graphite,
    Midnight,
    Nord,
}

/// The colours a theme assigns.
pub struct Colors {
    pub page: &'static str,
    /// Sidebar fill, drawn over the window's translucent material.
    pub sidebar: &'static str,
    /// Code blocks and table headers.
    pub raised: &'static str,
    pub hover: &'static str,
    pub text: &'static str,
    pub muted: &'static str,
    pub border: &'static str,
    pub accent: &'static str,
    pub on_accent: &'static str,
    /// Find matches, and the current match.
    pub mark: &'static str,
    pub mark_current: &'static str,
}

impl Theme {
    pub const LIGHT: [Theme; 3] = [Theme::Paper, Theme::Sepia, Theme::Mist];
    pub const DARK: [Theme; 3] = [Theme::Graphite, Theme::Midnight, Theme::Nord];

    pub fn name(self) -> &'static str {
        match self {
            Theme::Paper => "Paper",
            Theme::Sepia => "Sepia",
            Theme::Mist => "Mist",
            Theme::Graphite => "Graphite",
            Theme::Midnight => "Midnight",
            Theme::Nord => "Nord",
        }
    }

    pub fn dark(self) -> bool {
        Theme::DARK.contains(&self)
    }

    pub fn colors(self) -> Colors {
        match self {
            Theme::Paper => Colors {
                page: "#FFFFFF",
                sidebar: "#F3F3F5C8",
                raised: "#F6F7F9",
                hover: "#EBEBEF",
                text: "#1F2023",
                muted: "#6B6E76",
                border: "#E3E4E8",
                accent: "#0A66D8",
                on_accent: "#FFFFFF",
                mark: "#FFE7A0",
                mark_current: "#FFBF47",
            },
            Theme::Sepia => Colors {
                page: "#F8F1E3",
                sidebar: "#EFE5D0D8",
                raised: "#F0E6D2",
                hover: "#E6DAC1",
                text: "#3D3023",
                muted: "#84705A",
                border: "#E1D3B8",
                accent: "#A2541C",
                on_accent: "#FFF8EE",
                mark: "#F2D07A",
                mark_current: "#E79C34",
            },
            Theme::Mist => Colors {
                page: "#F6F8FB",
                sidebar: "#EBEFF5D0",
                raised: "#ECF0F5",
                hover: "#E1E7EF",
                text: "#1D2632",
                muted: "#64728A",
                border: "#DCE3EC",
                accent: "#2563EB",
                on_accent: "#FFFFFF",
                mark: "#FCE38A",
                mark_current: "#F9B728",
            },
            Theme::Graphite => Colors {
                page: "#1C1C1F",
                sidebar: "#232327C8",
                raised: "#26262B",
                hover: "#303036",
                text: "#E4E4E7",
                muted: "#9A9AA3",
                border: "#313137",
                accent: "#6AA8FF",
                on_accent: "#0B1220",
                mark: "#5E4C12",
                mark_current: "#A87B17",
            },
            Theme::Midnight => Colors {
                page: "#0B0C0E",
                sidebar: "#111317D0",
                raised: "#16191E",
                hover: "#1E2229",
                text: "#D8DBE0",
                muted: "#858C98",
                border: "#1F232A",
                accent: "#7DB3FF",
                on_accent: "#081120",
                mark: "#4F400E",
                mark_current: "#9C7412",
            },
            Theme::Nord => Colors {
                page: "#2E3440",
                sidebar: "#292E39D8",
                raised: "#353C4A",
                hover: "#3E4656",
                text: "#E5E9F0",
                muted: "#A0AABB",
                border: "#3B4252",
                accent: "#88C0D0",
                on_accent: "#242933",
                mark: "#6B5A2E",
                mark_current: "#B8924A",
            },
        }
    }

    /// The kit palette for this theme.
    pub fn palette(self) -> Palette {
        let c = self.colors();
        let dark = self.dark();
        let mut p = if dark {
            Palette::dark()
        } else {
            Palette::light()
        };
        let roles = [
            (Role::Base100, c.page),
            (Role::Base200, c.raised),
            (Role::Base300, c.hover),
            (Role::BaseContent, c.text),
            (Role::Muted, c.muted),
            (Role::Border, c.border),
            (Role::Primary, c.accent),
            (Role::PrimaryContent, c.on_accent),
            (Role::Accent, c.accent),
            (Role::AccentContent, c.on_accent),
            (Role::Info, c.accent),
            (Role::Success, if dark { "#4ADE80" } else { "#16A34A" }),
            (Role::Warning, if dark { "#FBBF24" } else { "#B45309" }),
            (Role::Error, if dark { "#F87171" } else { "#DC2626" }),
        ];
        for (role, hex) in roles {
            if let Some(color) = parse_hex(hex) {
                p.set(role, color);
            }
        }
        p
    }
}

/// A reading typeface: a name for the picker and the families to try.
pub struct Typeface {
    pub name: &'static str,
    pub family: &'static str,
}

/// Typefaces offered first. Others installed on the system can be chosen
/// by name; missing ones fall back along each list.
pub const TYPEFACES: [Typeface; 9] = [
    Typeface {
        name: "System",
        family: "SF",
    },
    Typeface {
        name: "New York",
        family: "New York, Charter, Georgia, serif",
    },
    Typeface {
        name: "Charter",
        family: "Charter, Georgia, serif",
    },
    Typeface {
        name: "Iowan Old Style",
        family: "Iowan Old Style, Palatino, Georgia, serif",
    },
    Typeface {
        name: "Georgia",
        family: "Georgia, serif",
    },
    Typeface {
        name: "Avenir Next",
        family: "Avenir Next, SF",
    },
    Typeface {
        name: "Helvetica Neue",
        family: "Helvetica Neue, SF",
    },
    Typeface {
        name: "Rounded",
        family: "SF Pro Rounded, SF",
    },
    Typeface {
        name: "Monospaced",
        family: "SF Mono, Menlo, monospace",
    },
];

/// Code typefaces, with the Nerd Font builds many developers install.
pub const CODE_FONTS: [Typeface; 5] = [
    Typeface {
        name: "SF Mono",
        family: "SF Mono, Menlo, monospace",
    },
    Typeface {
        name: "Menlo",
        family: "Menlo, SF Mono, monospace",
    },
    Typeface {
        name: "JetBrains Mono",
        family: "JetBrains Mono, JetBrainsMono Nerd Font Mono, JetBrainsMono NFM, SF Mono, monospace",
    },
    Typeface {
        name: "Fira Code",
        family: "Fira Code, FiraCode Nerd Font Mono, Fira Mono, FiraMono Nerd Font Mono, SF Mono, monospace",
    },
    Typeface {
        name: "Cascadia",
        family: "Cascadia Code, Cascadia Mono, CaskaydiaMono Nerd Font Mono, SF Mono, monospace",
    },
];

/// Body sizes ⌘= and ⌘− step through.
pub const SIZES: [f32; 12] = [12., 13., 14., 15., 16., 17., 18., 20., 22., 24., 26., 28.];
pub const DEFAULT_SIZE: f32 = 16.;
pub const LINE_HEIGHTS: std::ops::RangeInclusive<f32> = 1.3..=2.1;
pub const WIDTHS: std::ops::RangeInclusive<f32> = 520.0..=1000.0;
pub const SIDEBAR_WIDTHS: std::ops::RangeInclusive<f32> = 200.0..=420.0;
/// The lowest window background opacity: text stays readable above it.
pub const MIN_OPACITY: f32 = 0.3;

/// Which list the sidebar shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SidebarTab {
    #[default]
    Files,
    Outline,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: Appearance,
    pub light_theme: Theme,
    pub dark_theme: Theme,
    /// A [`TYPEFACES`] name, or any installed family.
    pub typeface: String,
    /// A [`CODE_FONTS`] name, or any installed family.
    pub code_font: String,
    pub font_size: f32,
    pub line_height: f32,
    /// Longest line of prose, in points.
    pub width: f32,
    pub sidebar: bool,
    pub sidebar_tab: SidebarTab,
    pub sidebar_width: f32,
    /// Window background opacity, 1 for a solid window.
    pub opacity: f32,
    /// Blur what is behind a translucent window.
    pub blur: bool,
    /// Workspace folders shown in the file tree.
    pub folders: Vec<PathBuf>,
    /// Folders expanded in the file tree.
    pub expanded: Vec<PathBuf>,
    /// Recently opened documents, newest first.
    pub recent: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            light_theme: Theme::Paper,
            dark_theme: Theme::Graphite,
            typeface: "System".into(),
            code_font: "SF Mono".into(),
            font_size: DEFAULT_SIZE,
            line_height: 1.65,
            width: 700.,
            sidebar: true,
            sidebar_tab: SidebarTab::Files,
            sidebar_width: 264.,
            opacity: 1.,
            blur: true,
            folders: Vec::new(),
            expanded: Vec::new(),
            recent: Vec::new(),
        }
    }
}

impl Settings {
    /// The theme in use, given whether the system is dark.
    pub fn theme(&self, system_dark: bool) -> Theme {
        let dark = match self.appearance {
            Appearance::System => system_dark,
            Appearance::Light => false,
            Appearance::Dark => true,
        };
        if dark {
            self.dark_theme
        } else {
            self.light_theme
        }
    }

    /// The window background for these settings.
    pub fn window_background(&self) -> volt_ui_kit::native::WindowBackground {
        use volt_ui_kit::native::WindowBackground;
        if self.opacity >= 0.999 {
            WindowBackground::Opaque
        } else if self.blur {
            WindowBackground::Blur {
                opacity: self.opacity,
            }
        } else {
            WindowBackground::Transparent {
                opacity: self.opacity,
            }
        }
    }

    /// Whether the window lets the desktop show through.
    pub fn translucent(&self) -> bool {
        self.opacity < 0.999
    }

    /// The font family list for body text.
    pub fn body_family(&self) -> String {
        family(&TYPEFACES, &self.typeface, "SF")
    }

    pub fn code_family(&self) -> String {
        family(&CODE_FONTS, &self.code_font, "SF Mono, Menlo, monospace")
    }

    pub fn zoom(&mut self, step: i32) {
        let at = SIZES
            .iter()
            .position(|s| *s >= self.font_size)
            .unwrap_or(SIZES.len() - 1) as i32;
        self.font_size = SIZES[(at + step).clamp(0, SIZES.len() as i32 - 1) as usize];
    }

    pub fn remember(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(12);
    }

    /// Clamp values a hand-edited file may have pushed out of range.
    fn sanitize(mut self) -> Self {
        let defaults = Settings::default();
        if !self.font_size.is_finite() {
            self.font_size = defaults.font_size;
        }
        self.font_size = self.font_size.clamp(SIZES[0], SIZES[SIZES.len() - 1]);
        if !self.line_height.is_finite() {
            self.line_height = defaults.line_height;
        }
        self.line_height = self
            .line_height
            .clamp(*LINE_HEIGHTS.start(), *LINE_HEIGHTS.end());
        if !self.width.is_finite() {
            self.width = defaults.width;
        }
        self.width = self.width.clamp(*WIDTHS.start(), *WIDTHS.end());
        if !self.sidebar_width.is_finite() {
            self.sidebar_width = defaults.sidebar_width;
        }
        self.sidebar_width = self
            .sidebar_width
            .clamp(*SIDEBAR_WIDTHS.start(), *SIDEBAR_WIDTHS.end());
        if !self.opacity.is_finite() {
            self.opacity = defaults.opacity;
        }
        self.opacity = self.opacity.clamp(MIN_OPACITY, 1.);
        if self.light_theme.dark() {
            self.light_theme = defaults.light_theme;
        }
        if !self.dark_theme.dark() {
            self.dark_theme = defaults.dark_theme;
        }
        for name in [&mut self.typeface, &mut self.code_font] {
            if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
                name.clear();
            }
        }
        if self.typeface.is_empty() {
            self.typeface = defaults.typeface;
        }
        if self.code_font.is_empty() {
            self.code_font = defaults.code_font;
        }
        self.folders.truncate(32);
        self.expanded.truncate(4096);
        self.recent.truncate(12);
        self
    }

    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .unwrap_or_default()
            .sanitize()
    }

    /// Write atomically: a crash mid-save keeps the previous file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, path)
    }
}

/// A picker name's family list; unknown names are installed families.
fn family(list: &[Typeface], name: &str, fallback: &str) -> String {
    match list.iter().find(|t| t.name == name) {
        Some(t) => t.family.to_owned(),
        None => format!("{name}, {fallback}"),
    }
}

/// Where settings live: `MDSTAR_CONFIG_DIR`, else the platform's config folder.
pub fn settings_path() -> Option<PathBuf> {
    let dir = if let Some(dir) = std::env::var_os("MDSTAR_CONFIG_DIR") {
        PathBuf::from(dir)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/MD Star")
    } else if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?).join("MD Star")
    } else if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("mdstar")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".config/mdstar")
    };
    Some(dir.join("volt-settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_bad_values_are_clamped() {
        let dir = std::env::temp_dir().join(format!("mdstar-settings-{}", std::process::id()));
        let path = dir.join("volt-settings.json");
        let mut s = Settings {
            typeface: "Charter".into(),
            font_size: 18.,
            ..Settings::default()
        };
        s.remember(Path::new("/a.md"));
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        std::fs::write(
            &path,
            r#"{"font_size": 400, "line_height": -1, "light_theme": "midnight", "typeface": ""}"#,
        )
        .unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.font_size, 28.);
        assert_eq!(loaded.line_height, 1.3);
        assert_eq!(loaded.light_theme, Theme::Paper);
        assert_eq!(loaded.typeface, "System");
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn zoom_steps_through_sizes_and_stops_at_the_ends() {
        let mut s = Settings::default();
        s.zoom(1);
        assert_eq!(s.font_size, 17.);
        s.zoom(-2);
        assert_eq!(s.font_size, 15.);
        for _ in 0..20 {
            s.zoom(1);
        }
        assert_eq!(s.font_size, 28.);
        s.font_size = 16.5;
        s.zoom(1);
        assert_eq!(s.font_size, 18.);
    }

    #[test]
    fn every_theme_has_valid_colours() {
        for theme in Theme::LIGHT.iter().chain(&Theme::DARK) {
            let c = theme.colors();
            for hex in [
                c.page, c.sidebar, c.raised, c.hover, c.text, c.muted, c.border, c.accent,
                c.on_accent, c.mark, c.mark_current,
            ] {
                assert!(parse_hex(hex).is_some(), "{}: {hex}", theme.name());
            }
        }
    }
}
