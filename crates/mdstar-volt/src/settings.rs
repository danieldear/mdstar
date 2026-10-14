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
    SolarizedLight,
    Latte,
    RoseDawn,
    Graphite,
    Midnight,
    Nord,
    Dracula,
    TokyoNight,
    SolarizedDark,
    Mocha,
    RosePine,
}

/// The colours a theme assigns (opaque `#RRGGBB`).
pub struct Colors {
    pub page: &'static str,
    /// The sidebar's tint; translucent over the window material when the
    /// sidebar is glass.
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

/// (theme, name, page, sidebar, raised, hover, text, muted, border, accent,
/// on accent, mark, current mark)
type Row = (Theme, &'static str, [&'static str; 11]);
const THEMES: [Row; 14] = [
    (
        Theme::Paper,
        "Paper",
        [
            "#FFFFFF", "#F1F1F4", "#F6F7F9", "#EBEBEF", "#1F2023", "#6B6E76", "#E3E4E8", "#0A66D8",
            "#FFFFFF", "#FFE7A0", "#FFBF47",
        ],
    ),
    (
        Theme::Sepia,
        "Sepia",
        [
            "#F8F1E3", "#EEE3CC", "#F0E6D2", "#E6DAC1", "#3D3023", "#84705A", "#E1D3B8", "#A2541C",
            "#FFF8EE", "#F2D07A", "#E79C34",
        ],
    ),
    (
        Theme::Mist,
        "Mist",
        [
            "#F6F8FB", "#E9EEF5", "#ECF0F5", "#E1E7EF", "#1D2632", "#64728A", "#DCE3EC", "#2563EB",
            "#FFFFFF", "#FCE38A", "#F9B728",
        ],
    ),
    (
        Theme::SolarizedLight,
        "Solarized",
        [
            "#FDF6E3", "#EEE8D5", "#F4EDD9", "#E9E2CC", "#3E4D55", "#6C7C80", "#E4DCC5", "#268BD2",
            "#FDF6E3", "#F5E1A4", "#E5B83A",
        ],
    ),
    (
        Theme::Latte,
        "Latte",
        [
            "#EFF1F5", "#E6E9EF", "#E8EBF1", "#DCE0E8", "#4C4F69", "#6C6F85", "#CCD0DA", "#8839EF",
            "#EFF1F5", "#F5DFA0", "#DF8E1D",
        ],
    ),
    (
        Theme::RoseDawn,
        "Rosé Dawn",
        [
            "#FAF4ED", "#F2E9E1", "#FFFAF3", "#EDE4DC", "#575279", "#797593", "#DFDAD9", "#286983",
            "#FAF4ED", "#F6E3C0", "#EA9D34",
        ],
    ),
    (
        Theme::Graphite,
        "Graphite",
        [
            "#1C1C1F", "#232327", "#26262B", "#303036", "#E4E4E7", "#9A9AA3", "#313137", "#6AA8FF",
            "#0B1220", "#5E4C12", "#A87B17",
        ],
    ),
    (
        Theme::Midnight,
        "Midnight",
        [
            "#0B0C0E", "#111317", "#16191E", "#1E2229", "#D8DBE0", "#858C98", "#1F232A", "#7DB3FF",
            "#081120", "#4F400E", "#9C7412",
        ],
    ),
    (
        Theme::Nord,
        "Nord",
        [
            "#2E3440", "#292E39", "#353C4A", "#3E4656", "#E5E9F0", "#A0AABB", "#3B4252", "#88C0D0",
            "#242933", "#6B5A2E", "#B8924A",
        ],
    ),
    (
        Theme::Dracula,
        "Dracula",
        [
            "#282A36", "#21222C", "#303241", "#44475A", "#F8F8F2", "#9AA0C4", "#3B3E51", "#BD93F9",
            "#282A36", "#5D5B2A", "#A89A3B",
        ],
    ),
    (
        Theme::TokyoNight,
        "Tokyo Night",
        [
            "#1A1B26", "#16161E", "#1F2335", "#292E42", "#C0CAF5", "#7A82A8", "#292E42", "#7AA2F7",
            "#1A1B26", "#4A4528", "#9E8A3A",
        ],
    ),
    (
        Theme::SolarizedDark,
        "Solarized Dark",
        [
            "#002B36", "#00242D", "#073642", "#0B404D", "#B9C5C6", "#839496", "#0E3A45", "#268BD2",
            "#FDF6E3", "#5A4A0E", "#B58900",
        ],
    ),
    (
        Theme::Mocha,
        "Mocha",
        [
            "#1E1E2E", "#181825", "#262637", "#313244", "#CDD6F4", "#9399B2", "#313244", "#CBA6F7",
            "#1E1E2E", "#554F2E", "#A08A3E",
        ],
    ),
    (
        Theme::RosePine,
        "Rosé Pine",
        [
            "#191724", "#1F1D2E", "#26233A", "#2A2837", "#E0DEF4", "#908CAA", "#2A2837", "#9CCFD8",
            "#191724", "#554A2B", "#9C7A3E",
        ],
    ),
];

/// An accent colour, like macOS's: the theme's own, the system's, or one
/// of the macOS colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    #[default]
    Theme,
    System,
    Blue,
    Purple,
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    Graphite,
}

impl Accent {
    pub const ALL: [Accent; 10] = [
        Accent::Theme,
        Accent::System,
        Accent::Blue,
        Accent::Purple,
        Accent::Pink,
        Accent::Red,
        Accent::Orange,
        Accent::Yellow,
        Accent::Green,
        Accent::Graphite,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Accent::Theme => "Theme",
            Accent::System => "System",
            Accent::Blue => "Blue",
            Accent::Purple => "Purple",
            Accent::Pink => "Pink",
            Accent::Red => "Red",
            Accent::Orange => "Orange",
            Accent::Yellow => "Yellow",
            Accent::Green => "Green",
            Accent::Graphite => "Graphite",
        }
    }

    /// The macOS colour for light and dark appearances; `None` for the
    /// theme's and the system's accents.
    pub fn fixed(self, dark: bool) -> Option<&'static str> {
        let (light, night) = match self {
            Accent::Theme | Accent::System => return None,
            Accent::Blue => ("#007AFF", "#0A84FF"),
            Accent::Purple => ("#9F3FD0", "#BF5AF2"),
            Accent::Pink => ("#E3245A", "#FF375F"),
            Accent::Red => ("#E5342A", "#FF453A"),
            Accent::Orange => ("#E38400", "#FF9F0A"),
            Accent::Yellow => ("#C99A00", "#FFD60A"),
            Accent::Green => ("#24A148", "#30D158"),
            Accent::Graphite => ("#7A7A80", "#98989D"),
        };
        Some(if dark { night } else { light })
    }
}

/// Text on an accent: white, or near black on light accents.
fn on(accent: [f32; 4]) -> &'static str {
    let luma = 0.2126 * accent[0] + 0.7152 * accent[1] + 0.0722 * accent[2];
    if luma > 0.62 { "#14141A" } else { "#FFFFFF" }
}

impl Theme {
    pub const LIGHT: [Theme; 6] = [
        Theme::Paper,
        Theme::Sepia,
        Theme::Mist,
        Theme::SolarizedLight,
        Theme::Latte,
        Theme::RoseDawn,
    ];
    pub const DARK: [Theme; 8] = [
        Theme::Graphite,
        Theme::Midnight,
        Theme::Nord,
        Theme::Dracula,
        Theme::TokyoNight,
        Theme::SolarizedDark,
        Theme::Mocha,
        Theme::RosePine,
    ];

    fn row(self) -> &'static Row {
        THEMES
            .iter()
            .find(|(t, _, _)| *t == self)
            .expect("every theme has colours")
    }

    pub fn name(self) -> &'static str {
        self.row().1
    }

    pub fn dark(self) -> bool {
        Theme::DARK.contains(&self)
    }

    pub fn colors(self) -> Colors {
        let [
            page,
            sidebar,
            raised,
            hover,
            text,
            muted,
            border,
            accent,
            on_accent,
            mark,
            mark_current,
        ] = self.row().2;
        Colors {
            page,
            sidebar,
            raised,
            hover,
            text,
            muted,
            border,
            accent,
            on_accent,
            mark,
            mark_current,
        }
    }

    /// The kit palette for this theme with `accent` (an `#RRGGBB` override,
    /// or the theme's own).
    pub fn palette(self, accent: Option<[f32; 4]>) -> Palette {
        let c = self.colors();
        let dark = self.dark();
        let mut p = if dark {
            Palette::dark()
        } else {
            Palette::light()
        };
        let accent = accent.or_else(|| parse_hex(c.accent));
        let on_accent = match accent {
            Some(a) if accent != parse_hex(c.accent) => on(a),
            _ => c.on_accent,
        };
        let roles = [
            (Role::Base100, c.page),
            (Role::Base200, c.raised),
            (Role::Base300, c.hover),
            (Role::BaseContent, c.text),
            (Role::Muted, c.muted),
            (Role::Border, c.border),
            (Role::PrimaryContent, on_accent),
            (Role::AccentContent, on_accent),
            (Role::Success, if dark { "#4ADE80" } else { "#16A34A" }),
            (Role::Warning, if dark { "#FBBF24" } else { "#B45309" }),
            (Role::Error, if dark { "#F87171" } else { "#DC2626" }),
        ];
        for (role, hex) in roles {
            if let Some(color) = parse_hex(hex) {
                p.set(role, color);
            }
        }
        if let Some(a) = accent {
            for role in [Role::Primary, Role::Accent, Role::Info] {
                p.set(role, a);
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
    pub accent: Accent,
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
    /// The sidebar shows the desktop through it, blurred.
    pub sidebar_glass: bool,
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
            accent: Accent::Theme,
            typeface: "System".into(),
            code_font: "SF Mono".into(),
            font_size: DEFAULT_SIZE,
            line_height: 1.65,
            width: 700.,
            sidebar: true,
            sidebar_tab: SidebarTab::Files,
            sidebar_width: 264.,
            sidebar_glass: true,
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

    /// The window background for these settings. Under a solid page only
    /// a glass sidebar shows the window's own background, so it is kept light.
    pub fn window_background(&self) -> volt_ui_kit::native::WindowBackground {
        use volt_ui_kit::native::WindowBackground;
        if !self.translucent() && !self.sidebar_glass {
            return WindowBackground::Opaque;
        }
        let opacity = if self.translucent() {
            self.opacity
        } else {
            0.3
        };
        if self.blur {
            WindowBackground::Blur { opacity }
        } else {
            WindowBackground::Transparent { opacity }
        }
    }

    /// Whether the page lets the desktop show through.
    pub fn translucent(&self) -> bool {
        self.opacity < 0.999
    }

    /// Whether anything shows the desktop: the page or the sidebar.
    pub fn glass(&self) -> bool {
        self.translucent() || self.sidebar_glass
    }

    /// The sidebar fill: the theme tint, translucent over glass.
    pub fn sidebar_fill(&self, theme: Theme) -> String {
        let alpha = if self.glass() { "99" } else { "FF" };
        format!("{}{alpha}", theme.colors().sidebar)
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
                c.page,
                c.sidebar,
                c.raised,
                c.hover,
                c.text,
                c.muted,
                c.border,
                c.accent,
                c.on_accent,
                c.mark,
                c.mark_current,
            ] {
                assert!(parse_hex(hex).is_some(), "{}: {hex}", theme.name());
            }
        }
    }
}
