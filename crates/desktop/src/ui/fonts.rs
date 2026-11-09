use egui::{FontFamily, FontId, TextStyle};

#[derive(Clone)]
pub struct FontOption {
    pub name: &'static str,
    pub family_type: FontFamilyType,
    pub path: Option<&'static [u8]>,
    pub size: f32,
}

#[derive(Clone, Copy)]
pub enum FontFamilyType {
    Proportional,
    Monospace,
}

pub const FONTS: [FontOption; 2] = [
    FontOption {
        name: "Default",
        family_type: FontFamilyType::Proportional,
        path: None,
        size: 14.0,
    },
    FontOption {
        name: "Monospace",
        family_type: FontFamilyType::Monospace,
        path: None,
        size: 14.0,
    },
];

impl FontOption {
    pub fn get_family(&self) -> FontFamily {
        match self.family_type {
            FontFamilyType::Proportional => FontFamily::Proportional,
            FontFamilyType::Monospace => FontFamily::Monospace,
        }
    }
}

pub fn setup_custom_fonts(ctx: &egui::Context, font_option: &FontOption) {
    let mut fonts = egui::FontDefinitions::default();
    let family = font_option.get_family();

    if let Some(font_data) = font_option.path {
        let font_name = match font_option.family_type {
            _ => "custom_font".to_string(),
        };

        fonts
            .font_data
            .insert(font_name.clone(), egui::FontData::from_static(font_data));

        match font_option.family_type {
            FontFamilyType::Proportional | FontFamilyType::Monospace => {
                fonts
                    .families
                    .get_mut(&family)
                    .unwrap()
                    .insert(0, font_name);
            }
        }
    }

    let text_styles = [
        (TextStyle::Heading, font_option.size + 4.0),
        (TextStyle::Body, font_option.size),
        (TextStyle::Monospace, font_option.size),
        (TextStyle::Button, font_option.size),
        (TextStyle::Small, font_option.size - 2.0),
    ];

    let mut style = (*ctx.style()).clone();
    for (text_style, size) in text_styles {
        style
            .text_styles
            .insert(text_style, FontId::new(size, family.clone()));
    }

    ctx.set_fonts(fonts);
    ctx.set_style(style);
}
