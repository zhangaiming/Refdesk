use eframe::egui::{self, Color32, Stroke};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ThemeId {
    CoolDark,
    CoolLight,
    WarmLight,
    #[default]
    #[serde(other)]
    WarmDark,
}

impl ThemeId {
    pub(crate) const GROUPS: [(&'static str, [Self; 2]); 2] = [
        ("亮色", [Self::CoolLight, Self::WarmLight]),
        ("暗色", [Self::WarmDark, Self::CoolDark]),
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::WarmDark => "暖灰绿",
            Self::CoolDark => "深蓝灰",
            Self::CoolLight => "冷白蓝",
            Self::WarmLight => "暖白棕",
        }
    }

    pub(crate) fn is_dark(self) -> bool {
        matches!(self, Self::WarmDark | Self::CoolDark)
    }

    pub(crate) fn palette(self) -> &'static Palette {
        match self {
            Self::WarmDark => &WARM_DARK,
            Self::CoolDark => &COOL_DARK,
            Self::CoolLight => &COOL_LIGHT,
            Self::WarmLight => &WARM_LIGHT,
        }
    }

    pub(crate) fn apply(self, ctx: &egui::Context) {
        let mode = if self.is_dark() {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        ctx.set_visuals_of(mode, self.palette().visuals(mode));
        ctx.set_theme(mode);
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("workbench-theme"), self));
        ctx.request_repaint();
    }
}

pub(crate) fn palette(ctx: &egui::Context) -> &'static Palette {
    ctx.data(|data| data.get_temp::<ThemeId>(egui::Id::new("workbench-theme")))
        .unwrap_or_default()
        .palette()
}

#[derive(Clone, Copy)]
pub(crate) struct ChangeColors {
    pub foreground: Color32,
    pub background: Color32,
    pub border: Color32,
}

pub(crate) struct Palette {
    pub base: Color32,
    pub panel: Color32,
    pub chrome: Color32,
    pub control: Color32,
    pub hover: Color32,
    pub selection: Color32,
    pub border: Color32,
    pub divider: Color32,
    pub accent: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub subtle: Color32,
    pub warning: Color32,
    pub added: ChangeColors,
    pub removed: ChangeColors,
    pub splitter_hover: Color32,
    pub tree_guide: Color32,
    pub window_active_border: Color32,
    pub close_hover: Color32,
    pub close_pressed: Color32,
    pub close_foreground: Color32,
    shadow: u8,
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
