use eframe::egui::{FontData, FontDefinitions};
use objc2_core_foundation::{CFRange, CFString, CFURL};
use objc2_core_text::{CTFont, CTFontUIFontType, kCTFontURLAttribute};
use ttf_parser::{Face, fonts_in_collection, name_id};

const CHINESE_SAMPLE: &str = "简体中文，仓库提交分支差异";

pub(super) fn install(fonts: &mut FontDefinitions) {
    let Some(face) = system_chinese_fallback() else {
        return;
    };
    let name = "macos-chinese";
    fonts.font_data.insert(name.into(), face.into());
    for family in fonts.families.values_mut() {
        family.push(name.into());
    }
}

fn system_chinese_fallback() -> Option<FontData> {
    let language = CFString::from_str("zh-Hans");
    let sample = CFString::from_str(CHINESE_SAMPLE);
    // SAFETY: The language is a live CFString and size zero requests the default UI size.
    let ui_font =
        unsafe { CTFont::new_ui_font_for_language(CTFontUIFontType::System, 0.0, Some(&language)) };
    if let Some(ui_font) = ui_font {
        let range = CFRange {
            location: 0,
            length: sample.length(),
        };
        // SAFETY: The range covers the live sample CFString in UTF-16 code units.
        let fallback = unsafe { ui_font.for_string_with_language(&sample, range, Some(&language)) };
        if let Some(data) = read_face(&fallback) {
            return Some(data);
        }
    }

    ["PingFangSC-Regular", "HiraginoSansGB-W3", "STHeitiSC-Light"]
        .into_iter()
        .find_map(|name| {
            let name = CFString::from_str(name);
            // SAFETY: The name is a live CFString and a null pointer requests the identity transform.
            let font = unsafe { CTFont::with_name(&name, 0.0, std::ptr::null()) };
            read_face(&font)
        })
}

fn read_face(font: &CTFont) -> Option<FontData> {
    // SAFETY: The CTFont is live and kCTFontURLAttribute is a Core Text constant.
    let (url, name) = unsafe { (font.attribute(kCTFontURLAttribute), font.post_script_name()) };
    let path = url?.downcast::<CFURL>().ok()?.to_file_path()?;
    let bytes = std::fs::read(path).ok()?;
    let name = name.to_string();
    let name = name.trim_start_matches('.');
    let index = (0..fonts_in_collection(&bytes).unwrap_or(1)).find(|&index| {
        let Ok(face) = Face::parse(&bytes, index) else {
            return false;
        };
        face.names().into_iter().any(|entry| {
            entry.name_id == name_id::POST_SCRIPT_NAME
                && (entry.name == name.as_bytes()
                    || entry
                        .to_string()
                        .is_some_and(|entry| entry.trim_start_matches('.') == name))
        }) && CHINESE_SAMPLE
            .chars()
            .all(|ch| face.glyph_index(ch).is_some())
    })?;
    let mut data = FontData::from_owned(bytes);
    data.index = index;
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::CHINESE_SAMPLE;
    use eframe::egui::{Context, FontId};

    #[test]
    fn installed_style_renders_chinese_in_both_font_families() {
        let ctx = Context::default();
        crate::ui::install_style(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts(|fonts| {
                for font in [FontId::proportional(14.0), FontId::monospace(14.0)] {
                    assert!(fonts.has_glyphs(&font, CHINESE_SAMPLE), "{font:?}");
                }
            });
        });
    }
}
