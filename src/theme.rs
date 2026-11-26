use eframe::egui::{self, Color32, Stroke};
use serde::{Deserialize, Deserializer, de};
use std::{fs, io, path::Path};

pub(crate) const DEFAULT_THEME_ID: &str = "warm-dark";
const EXAMPLES: [(&str, &str); 2] = [
    (
        "neutral-light.json",
        include_str!("../assets/themes/neutral-light.json"),
    ),
    (
        "neutral-dark.json",
        include_str!("../assets/themes/neutral-dark.json"),
    ),
];

pub(crate) fn install_examples(directory: &Path) -> io::Result<()> {
    use io::{ErrorKind, Write};

    let marker = directory.with_file_name(".theme-examples-installed");
    if marker.try_exists()? {
        return Ok(());
    }
    fs::create_dir_all(directory)?;
    for (name, contents) in EXAMPLES {
        let path = directory.join(name);
        if path.try_exists()? {
            continue;
        }

        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        file.write_all(contents.as_bytes())?;
        match file.persist_noclobber(path) {
            Ok(_) => {}
            Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.error),
        }
    }
    match fs::File::create_new(marker) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
pub(crate) fn open_directory(directory: &Path) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null;
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

    fs::create_dir_all(directory)?;
    let absolute = std::path::absolute(directory)?;
    let path: Vec<_> = absolute.as_os_str().encode_wide().chain([0]).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            null(),
            path.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    anyhow::ensure!(result as isize > 32, "Windows 返回错误 {}", result as isize);
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Theme {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    #[serde(deserialize_with = "deserialize_mode")]
    pub mode: egui::Theme,
    pub palette: Palette,
}

impl Theme {
    pub(crate) fn load(directory: &Path) -> Vec<Self> {
        use egui::Theme::{Dark, Light};

        let mut themes: Vec<_> = [
            (DEFAULT_THEME_ID, "暖灰绿", Dark, WARM_DARK),
            ("cool-dark", "深蓝灰", Dark, COOL_DARK),
            ("cool-light", "冷白蓝", Light, COOL_LIGHT),
            ("warm-light", "暖白棕", Light, WARM_LIGHT),
        ]
        .into_iter()
        .map(|(id, name, mode, palette)| Self {
            id: id.into(),
            name: name.into(),
            mode,
            palette,
        })
        .collect();

        let Ok(entries) = fs::read_dir(directory) else {
            return themes;
        };
        let mut files: Vec<_> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            })
            .collect();
        files.sort_unstable();

        for path in files {
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let id = stem.to_ascii_lowercase();
            if id.trim().is_empty() || themes.iter().any(|theme| theme.id == id) {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
            let Ok(mut theme) = serde_json::from_slice::<Self>(bytes) else {
                continue;
            };
            theme.name = theme.name.trim().to_owned();
            if !theme.name.is_empty() {
                theme.id = id;
                themes.push(theme);
            }
        }
        themes
    }

    pub(crate) fn apply(&self, ctx: &egui::Context) {
        ctx.set_visuals_of(self.mode, self.palette.visuals(self.mode));
        ctx.set_theme(self.mode);
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("workbench-theme"), self.palette));
        ctx.request_repaint();
    }
}

pub(crate) fn palette(ctx: &egui::Context) -> Palette {
    ctx.data(|data| data.get_temp(egui::Id::new("workbench-theme")))
        .unwrap_or(WARM_DARK)
}

fn deserialize_mode<'de, D: Deserializer<'de>>(deserializer: D) -> Result<egui::Theme, D::Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "lowercase")]
    enum Mode {
        Light,
        Dark,
    }

    Ok(match Mode::deserialize(deserializer)? {
        Mode::Light => egui::Theme::Light,
        Mode::Dark => egui::Theme::Dark,
    })
}

fn deserialize_color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color32, D::Error> {
    let value = String::deserialize(deserializer)?;
    let hex = value
        .strip_prefix('#')
        .filter(|hex| hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| de::Error::custom("expected a #RRGGBB color"))?;
    u32::from_str_radix(hex, 16)
        .map(rgb)
        .map_err(de::Error::custom)
}

fn deserialize_lanes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[Color32; 8], D::Error> {
    #[derive(Deserialize)]
    struct HexColor(#[serde(deserialize_with = "deserialize_color")] Color32);

    Ok(<[HexColor; 8]>::deserialize(deserializer)?.map(|color| color.0))
}

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangeColors {
    #[serde(deserialize_with = "deserialize_color")]
    pub foreground: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub background: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub border: Color32,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Palette {
    #[serde(deserialize_with = "deserialize_color")]
    pub base: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub panel: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub chrome: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub control: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub hover: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub selection: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub border: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub divider: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub accent: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub text: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub muted: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub subtle: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub warning: Color32,
    pub added: ChangeColors,
    pub removed: ChangeColors,
    #[serde(deserialize_with = "deserialize_color")]
    pub splitter_hover: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub tree_guide: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub window_active_border: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub close_hover: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub close_pressed: Color32,
    #[serde(deserialize_with = "deserialize_color")]
    pub close_foreground: Color32,
    shadow: u8,
    #[serde(deserialize_with = "deserialize_lanes")]
    lanes: [Color32; 8],
}

impl Palette {
    pub(crate) fn lane(&self, index: usize) -> Color32 {
        self.lanes[index % self.lanes.len()]
    }

    fn visuals(&self, mode: egui::Theme) -> egui::Visuals {
        let mut visuals = mode.default_visuals();
        visuals.override_text_color = Some(self.text);
        visuals.window_fill = self.panel;
        visuals.window_stroke = Stroke::new(1.0_f32, self.border);
        visuals.window_shadow.color = Color32::from_black_alpha(self.shadow);
        visuals.popup_shadow.color = visuals.window_shadow.color;
        visuals.panel_fill = self.base;
        visuals.extreme_bg_color = self.base;
        visuals.faint_bg_color = self.control;
        visuals.code_bg_color = self.control;
        visuals.hyperlink_color = self.accent;
        visuals.warn_fg_color = self.warning;
        visuals.error_fg_color = self.removed.foreground;
        visuals.text_cursor.stroke.color = self.accent;
        visuals.selection.bg_fill = self.selection;
        visuals.selection.stroke = Stroke::new(1.0_f32, self.accent);

        let widgets = &mut visuals.widgets;
        for (widget, fill, edge, ink) in [
            (
                &mut widgets.noninteractive,
                self.panel,
                Stroke::new(1.0_f32, self.border),
                self.subtle,
            ),
            (
                &mut widgets.inactive,
                self.control,
                Stroke::new(0.6_f32, self.border),
                self.muted,
            ),
            (
                &mut widgets.hovered,
                self.hover,
                Stroke::new(0.8_f32, self.splitter_hover),
                self.text,
            ),
            (
                &mut widgets.active,
                self.selection,
                Stroke::new(1.0_f32, self.accent),
                self.text,
            ),
            (
                &mut widgets.open,
                self.selection,
                Stroke::new(1.0_f32, self.border),
                self.text,
            ),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.bg_stroke = edge;
            widget.fg_stroke.color = ink;
        }
        visuals
    }
}

const fn rgb(hex: u32) -> Color32 {
    let [_, red, green, blue] = hex.to_be_bytes();
    Color32::from_rgb(red, green, blue)
}

const WARM_DARK: Palette = Palette {
    base: rgb(0x181817),
    panel: rgb(0x1F1F1E),
    chrome: rgb(0x252422),
    control: rgb(0x2C2B28),
    hover: rgb(0x32312E),
    selection: rgb(0x343A31),
    border: rgb(0x3E3C37),
    divider: rgb(0x252422),
    accent: rgb(0xB2CDA0),
    text: rgb(0xECE9E1),
    muted: rgb(0xC1BBB1),
    subtle: rgb(0xA6A197),
    warning: rgb(0xE5B780),
    added: ChangeColors {
        foreground: rgb(0x97D09B),
        background: rgb(0x293C2B),
        border: rgb(0x74A777),
    },
    removed: ChangeColors {
        foreground: rgb(0xEF958F),
        background: rgb(0x432D2A),
        border: rgb(0xC57771),
    },
    splitter_hover: rgb(0x54514B),
    tree_guide: rgb(0x4A4843),
    window_active_border: rgb(0x56524B),
    close_hover: rgb(0xC42B1C),
    close_pressed: rgb(0xAA2318),
    close_foreground: Color32::WHITE,
    shadow: 96,
    lanes: [
        rgb(0xB2CDA0),
        rgb(0x90B2DE),
        rgb(0xE2B483),
        rgb(0xBF94D7),
        rgb(0xF5C45B),
        rgb(0x76C9E2),
        rgb(0xEC8498),
        rgb(0xB0CB68),
    ],
};

const COOL_DARK: Palette = Palette {
    base: rgb(0x151B26),
    panel: rgb(0x1D2634),
    chrome: rgb(0x222E40),
    control: rgb(0x283548),
    hover: rgb(0x324258),
    selection: rgb(0x2B3D5D),
    border: rgb(0x43536B),
    divider: rgb(0x293649),
    accent: rgb(0x98B9F2),
    text: rgb(0xE6EDF6),
    muted: rgb(0xB9C6D9),
    subtle: rgb(0xA0AEC3),
    warning: rgb(0xE9BE79),
    added: ChangeColors {
        foreground: rgb(0x87D4A7),
        background: rgb(0x203D37),
        border: rgb(0x6EB798),
    },
    removed: ChangeColors {
        foreground: rgb(0xF3A4AD),
        background: rgb(0x462C3C),
        border: rgb(0xD08799),
    },
    splitter_hover: rgb(0x62799C),
    tree_guide: rgb(0x4B5D78),
    window_active_border: rgb(0x6581A8),
    close_hover: rgb(0xBA3341),
    close_pressed: rgb(0x9F2836),
    close_foreground: rgb(0xFFF6F7),
    shadow: 105,
    lanes: [
        rgb(0x98B9F2),
        rgb(0x79D5C8),
        rgb(0xF4B386),
        rgb(0xC4A5F2),
        rgb(0xE8CF74),
        rgb(0x78CADF),
        rgb(0xF398AD),
        rgb(0xB4D783),
    ],
};

const COOL_LIGHT: Palette = Palette {
    base: rgb(0xFFFFFF),
    panel: rgb(0xF2F5FA),
    chrome: rgb(0xE9EFF7),
    control: rgb(0xE6EDF7),
    hover: rgb(0xE2EBF8),
    selection: rgb(0xDCE7F8),
    border: rgb(0xBAC8DD),
    divider: rgb(0xE1E8F2),
    accent: rgb(0x2859A8),
    text: rgb(0x172335),
    muted: rgb(0x455975),
    subtle: rgb(0x52647D),
    warning: rgb(0x805007),
    added: ChangeColors {
        foreground: rgb(0x246743),
        background: rgb(0xE0F1E7),
        border: rgb(0x4C8766),
    },
    removed: ChangeColors {
        foreground: rgb(0xA12F47),
        background: rgb(0xF8E5E9),
        border: rgb(0xB86071),
    },
    splitter_hover: rgb(0x7C94B6),
    tree_guide: rgb(0x9BADCA),
    window_active_border: rgb(0x7E98BD),
    close_hover: rgb(0xB82738),
    close_pressed: rgb(0x9B2030),
    close_foreground: Color32::WHITE,
    shadow: 39,
    lanes: [
        rgb(0x2859A8),
        rgb(0x0C6D65),
        rgb(0x914711),
        rgb(0x7545A1),
        rgb(0x7B5B0C),
        rgb(0x166681),
        rgb(0xA53762),
        rgb(0x476D20),
    ],
};

const WARM_LIGHT: Palette = Palette {
    base: rgb(0xFFFCF5),
    panel: rgb(0xF4EFE5),
    chrome: rgb(0xEEE6D7),
    control: rgb(0xEDE4D5),
    hover: rgb(0xEDE0CC),
    selection: rgb(0xF0E2CD),
    border: rgb(0xCEC0AD),
    divider: rgb(0xEAE0CF),
    accent: rgb(0x855219),
    text: rgb(0x2C261E),
    muted: rgb(0x5C4D3D),
    subtle: rgb(0x695B4A),
    warning: rgb(0x7F4D0B),
    added: ChangeColors {
        foreground: rgb(0x316342),
        background: rgb(0xE5EFD9),
        border: rgb(0x65865A),
    },
    removed: ChangeColors {
        foreground: rgb(0x9A3B3F),
        background: rgb(0xF5E1DA),
        border: rgb(0xB67469),
    },
    splitter_hover: rgb(0xA59175),
    tree_guide: rgb(0xAF9B82),
    window_active_border: rgb(0xAA9272),
    close_hover: rgb(0xB43128),
    close_pressed: rgb(0x97251F),
    close_foreground: rgb(0xFFFAF2),
    shadow: 39,
    lanes: [
        rgb(0x855219),
        rgb(0x345E9F),
        rgb(0x9F4235),
        rgb(0x784594),
        rgb(0x765E10),
        rgb(0x24687C),
        rgb(0x9B3960),
        rgb(0x496A27),
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    const LIGHT: &str = EXAMPLES[0].1;
    const DARK: &str = EXAMPLES[1].1;

    #[test]
    fn installs_examples_once_and_preserves_existing_examples() {
        let data = tempfile::tempdir().unwrap();
        let directory = data.path().join("themes");
        fs::create_dir(&directory).unwrap();
        let light = directory.join(EXAMPLES[0].0);
        let dark = directory.join(EXAMPLES[1].0);
        let custom = LIGHT.replace("浅灰蓝", "我的浅色");
        fs::write(&light, &custom).unwrap();

        install_examples(&directory).unwrap();
        assert_eq!(fs::read_to_string(&light).unwrap(), custom);
        assert_eq!(fs::read_to_string(&dark).unwrap(), DARK);
        fs::remove_file(&dark).unwrap();
        fs::remove_file(&light).unwrap();
        fs::remove_dir(&directory).unwrap();

        install_examples(&directory).unwrap();
        assert!(!directory.exists());
    }

    #[test]
    fn four_built_in_themes_work_without_a_theme_directory() {
        let directory = tempfile::tempdir().unwrap();
        let themes = Theme::load(&directory.path().join("missing"));
        let ids: Vec<_> = themes.iter().map(|theme| theme.id.as_str()).collect();
        assert_eq!(
            ids,
            [DEFAULT_THEME_ID, "cool-dark", "cool-light", "warm-light"]
        );
    }

    #[test]
    fn loads_valid_theme_files_and_skips_invalid_or_reserved_files() {
        let directory = tempfile::tempdir().unwrap();
        for (file, json) in [
            ("neutral-light.json", LIGHT),
            ("我的主题.JSON", &format!("\u{feff}{DARK}")),
            ("WARM-DARK.json", LIGHT),
            ("broken.json", "{"),
            ("bad-color.json", &LIGHT.replace("#FFFFFF", "#ZZZZZZ")),
            (
                "bad-lanes.json",
                &LIGHT.replace("\"#526727\"", "\"#526727\", \"#FFFFFF\""),
            ),
            ("notes.txt", LIGHT),
        ] {
            fs::write(directory.path().join(file), json).unwrap();
        }
        let themes = Theme::load(directory.path());
        assert_eq!(themes.len(), 6);
        assert_eq!(themes[0].palette.base, WARM_DARK.base);
        let light = themes
            .iter()
            .find(|theme| theme.id == "neutral-light")
            .unwrap();
        assert_eq!(light.name, "浅灰蓝");
        assert_eq!(light.mode, egui::Theme::Light);
        assert_eq!(light.palette.selection, rgb(0xCCE6FF));
        let dark = themes.iter().find(|theme| theme.id == "我的主题").unwrap();
        assert_eq!(dark.name, "炭灰紫");
        assert_eq!(dark.mode, egui::Theme::Dark);
        assert_eq!(dark.palette.lane(7), rgb(0xB8CE86));
        let ctx = egui::Context::default();
        dark.apply(&ctx);
        assert_eq!(palette(&ctx).base, rgb(0x282828));
        assert_eq!(
            ctx.style_of(dark.mode).visuals.selection.bg_fill,
            dark.palette.selection
        );
    }
}
