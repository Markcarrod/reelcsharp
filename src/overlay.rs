use ab_glyph::{Font, FontArc, Glyph, PxScale, ScaleFont};
use encoding_rs::WINDOWS_1252;
use image::{Rgba, RgbaImage};
use std::fs;
use std::path::{Path, PathBuf};
use crate::parser::{Script, slugify, normalize_layout};

pub const WIDTH: u32 = 1080;
pub const HEIGHT: u32 = 1920;

#[derive(Debug, Clone)]
pub struct LayoutParam {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub align: String,
    pub font_size: f32,
}

pub struct LayoutSpec {
    pub title: LayoutParam,
    pub point: LayoutParam, // will be offset by point_index * offset_y
    pub cta: LayoutParam,
    pub marker: String,
}

pub fn get_layout_spec(layout: &str) -> LayoutSpec {
    match layout {
        "two_column_split" => LayoutSpec {
            title: LayoutParam { x: 80.0, y: 320.0, width: 420.0, align: "left".to_string(), font_size: 72.0 },
            point: LayoutParam { x: 560.0, y: 500.0, width: 360.0, align: "left".to_string(), font_size: 44.0 },
            cta: LayoutParam { x: 560.0, y: 1470.0, width: 360.0, align: "left".to_string(), font_size: 30.0 },
            marker: "".to_string(),
        },
        "grid_layout" => LayoutSpec {
            title: LayoutParam { x: 100.0, y: 250.0, width: 880.0, align: "center".to_string(), font_size: 72.0 },
            point: LayoutParam { x: 110.0, y: 640.0, width: 380.0, align: "left".to_string(), font_size: 42.0 },
            cta: LayoutParam { x: 100.0, y: 1520.0, width: 880.0, align: "center".to_string(), font_size: 30.0 },
            marker: "".to_string(),
        },
        "masonry_layout" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 260.0, width: 900.0, align: "left".to_string(), font_size: 70.0 },
            point: LayoutParam { x: 95.0, y: 630.0, width: 430.0, align: "left".to_string(), font_size: 40.0 },
            cta: LayoutParam { x: 100.0, y: 1510.0, width: 880.0, align: "left".to_string(), font_size: 30.0 },
            marker: "- ".to_string(),
        },
        "hero_list" => LayoutSpec {
            title: LayoutParam { x: 80.0, y: 300.0, width: 920.0, align: "center".to_string(), font_size: 82.0 },
            point: LayoutParam { x: 120.0, y: 860.0, width: 840.0, align: "left".to_string(), font_size: 46.0 },
            cta: LayoutParam { x: 110.0, y: 1515.0, width: 860.0, align: "center".to_string(), font_size: 30.0 },
            marker: "- ".to_string(),
        },
        "alternating_rows" => LayoutSpec {
            title: LayoutParam { x: 100.0, y: 260.0, width: 880.0, align: "center".to_string(), font_size: 72.0 },
            point: LayoutParam { x: 110.0, y: 680.0, width: 860.0, align: "left".to_string(), font_size: 44.0 },
            cta: LayoutParam { x: 110.0, y: 1510.0, width: 860.0, align: "center".to_string(), font_size: 30.0 },
            marker: "".to_string(),
        },
        "sidebar_layout" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 320.0, width: 640.0, align: "left".to_string(), font_size: 74.0 },
            point: LayoutParam { x: 120.0, y: 760.0, width: 610.0, align: "left".to_string(), font_size: 42.0 },
            cta: LayoutParam { x: 120.0, y: 1490.0, width: 610.0, align: "left".to_string(), font_size: 30.0 },
            marker: "- ".to_string(),
        },
        "collage_layout" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 280.0, width: 900.0, align: "center".to_string(), font_size: 76.0 },
            point: LayoutParam { x: 120.0, y: 700.0, width: 360.0, align: "left".to_string(), font_size: 38.0 },
            cta: LayoutParam { x: 110.0, y: 1510.0, width: 860.0, align: "center".to_string(), font_size: 30.0 },
            marker: "".to_string(),
        },
        "auto_fit_tiles" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 280.0, width: 900.0, align: "center".to_string(), font_size: 74.0 },
            point: LayoutParam { x: 110.0, y: 680.0, width: 380.0, align: "left".to_string(), font_size: 40.0 },
            cta: LayoutParam { x: 100.0, y: 1510.0, width: 880.0, align: "center".to_string(), font_size: 30.0 },
            marker: "".to_string(),
        },
        "tabbed_layout" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 330.0, width: 900.0, align: "left".to_string(), font_size: 72.0 },
            point: LayoutParam { x: 120.0, y: 770.0, width: 840.0, align: "left".to_string(), font_size: 44.0 },
            cta: LayoutParam { x: 110.0, y: 1495.0, width: 860.0, align: "left".to_string(), font_size: 30.0 },
            marker: "> ".to_string(),
        },
        "magazine_layout" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 250.0, width: 900.0, align: "left".to_string(), font_size: 80.0 },
            point: LayoutParam { x: 120.0, y: 760.0, width: 540.0, align: "left".to_string(), font_size: 42.0 },
            cta: LayoutParam { x: 120.0, y: 1510.0, width: 840.0, align: "left".to_string(), font_size: 30.0 },
            marker: "".to_string(),
        },
        "template_rotation_layout" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 470.0, width: 900.0, align: "center".to_string(), font_size: 80.0 },
            point: LayoutParam { x: 150.0, y: 820.0, width: 780.0, align: "center".to_string(), font_size: 48.0 },
            cta: LayoutParam { x: 110.0, y: 1515.0, width: 860.0, align: "center".to_string(), font_size: 30.0 },
            marker: "".to_string(),
        },
        "priority_based_layout" => LayoutSpec {
            title: LayoutParam { x: 80.0, y: 310.0, width: 920.0, align: "center".to_string(), font_size: 88.0 },
            point: LayoutParam { x: 120.0, y: 920.0, width: 840.0, align: "center".to_string(), font_size: 42.0 },
            cta: LayoutParam { x: 100.0, y: 1510.0, width: 880.0, align: "center".to_string(), font_size: 30.0 },
            marker: "".to_string(),
        },
        "adaptive_smart_layout" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 360.0, width: 900.0, align: "center".to_string(), font_size: 76.0 },
            point: LayoutParam { x: 120.0, y: 760.0, width: 840.0, align: "center".to_string(), font_size: 46.0 },
            cta: LayoutParam { x: 110.0, y: 1510.0, width: 860.0, align: "center".to_string(), font_size: 30.0 },
            marker: "".to_string(),
        },
        "fallback_universal_layout" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 520.0, width: 900.0, align: "center".to_string(), font_size: 82.0 },
            point: LayoutParam { x: 150.0, y: 820.0, width: 780.0, align: "center".to_string(), font_size: 48.0 },
            cta: LayoutParam { x: 110.0, y: 1510.0, width: 860.0, align: "center".to_string(), font_size: 30.0 },
            marker: "".to_string(),
        },
        "question_answer" => LayoutSpec {
            title: LayoutParam { x: 105.0, y: 500.0, width: 870.0, align: "center".to_string(), font_size: 76.0 },
            point: LayoutParam { x: 145.0, y: 825.0, width: 790.0, align: "center".to_string(), font_size: 52.0 },
            cta: LayoutParam { x: 110.0, y: 1515.0, width: 860.0, align: "center".to_string(), font_size: 33.0 },
            marker: "".to_string(),
        },
        "left_stack" => LayoutSpec {
            title: LayoutParam { x: 92.0, y: 350.0, width: 820.0, align: "left".to_string(), font_size: 72.0 },
            point: LayoutParam { x: 105.0, y: 710.0, width: 850.0, align: "left".to_string(), font_size: 48.0 },
            cta: LayoutParam { x: 105.0, y: 1510.0, width: 850.0, align: "left".to_string(), font_size: 33.0 },
            marker: "- ".to_string(),
        },
        "right_stack" => LayoutSpec {
            title: LayoutParam { x: 155.0, y: 680.0, width: 820.0, align: "right".to_string(), font_size: 70.0 },
            point: LayoutParam { x: 155.0, y: 930.0, width: 820.0, align: "right".to_string(), font_size: 46.0 },
            cta: LayoutParam { x: 155.0, y: 1510.0, width: 820.0, align: "right".to_string(), font_size: 33.0 },
            marker: "".to_string(),
        },
        "list_style" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 340.0, width: 900.0, align: "left".to_string(), font_size: 70.0 },
            point: LayoutParam { x: 110.0, y: 690.0, width: 850.0, align: "left".to_string(), font_size: 48.0 },
            cta: LayoutParam { x: 105.0, y: 1510.0, width: 850.0, align: "left".to_string(), font_size: 33.0 },
            marker: "- ".to_string(),
        },
        "top_bottom" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 250.0, width: 900.0, align: "left".to_string(), font_size: 68.0 },
            point: LayoutParam { x: 90.0, y: 1180.0, width: 900.0, align: "left".to_string(), font_size: 54.0 },
            cta: LayoutParam { x: 90.0, y: 1510.0, width: 900.0, align: "left".to_string(), font_size: 33.0 },
            marker: "".to_string(),
        },
        "one_word_hook" => LayoutSpec {
            title: LayoutParam { x: 80.0, y: 420.0, width: 920.0, align: "center".to_string(), font_size: 118.0 },
            point: LayoutParam { x: 145.0, y: 760.0, width: 790.0, align: "left".to_string(), font_size: 48.0 },
            cta: LayoutParam { x: 110.0, y: 1510.0, width: 860.0, align: "center".to_string(), font_size: 33.0 },
            marker: "- ".to_string(),
        },
        "quote_style" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 660.0, width: 900.0, align: "center".to_string(), font_size: 78.0 },
            point: LayoutParam { x: 125.0, y: 1080.0, width: 830.0, align: "center".to_string(), font_size: 42.0 },
            cta: LayoutParam { x: 110.0, y: 1450.0, width: 860.0, align: "center".to_string(), font_size: 33.0 },
            marker: "".to_string(),
        },
        "story_block" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 270.0, width: 900.0, align: "left".to_string(), font_size: 68.0 },
            point: LayoutParam { x: 100.0, y: 560.0, width: 880.0, align: "left".to_string(), font_size: 43.0 },
            cta: LayoutParam { x: 100.0, y: 1510.0, width: 880.0, align: "left".to_string(), font_size: 33.0 },
            marker: "".to_string(),
        },
        "progress_reveal" => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 420.0, width: 900.0, align: "left".to_string(), font_size: 54.0 },
            point: LayoutParam { x: 90.0, y: 770.0, width: 900.0, align: "center".to_string(), font_size: 92.0 },
            cta: LayoutParam { x: 110.0, y: 1510.0, width: 860.0, align: "center".to_string(), font_size: 33.0 },
            marker: "".to_string(),
        },
        "center_card" => LayoutSpec {
            title: LayoutParam { x: 100.0, y: 480.0, width: 880.0, align: "center".to_string(), font_size: 70.0 },
            point: LayoutParam { x: 155.0, y: 830.0, width: 770.0, align: "center".to_string(), font_size: 50.0 },
            cta: LayoutParam { x: 110.0, y: 1510.0, width: 860.0, align: "center".to_string(), font_size: 33.0 },
            marker: "".to_string(),
        },
        // center_stack
        _ => LayoutSpec {
            title: LayoutParam { x: 90.0, y: 520.0, width: 900.0, align: "center".to_string(), font_size: 84.0 },
            point: LayoutParam { x: 150.0, y: 790.0, width: 780.0, align: "center".to_string(), font_size: 50.0 },
            cta: LayoutParam { x: 110.0, y: 1510.0, width: 860.0, align: "center".to_string(), font_size: 33.0 },
            marker: "".to_string(),
        },
    }
}

pub fn load_system_font() -> FontArc {
    let paths = [
        "C:\\Windows\\Fonts\\arialbd.ttf",
        "C:\\Windows\\Fonts\\segoeuib.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
    ];
    for path in &paths {
        if Path::new(path).exists() {
            if let Ok(bytes) = fs::read(path) {
                if let Ok(font) = FontArc::try_from_vec(bytes) {
                    return font;
                }
            }
        }
    }
    // Fallback to embedded/basic default font representation using standard system layouts or static panic
    panic!("No TrueType system fonts (Arial, Segoe UI) found! Please install system fonts.");
}

pub fn first_hook_word(value: &str) -> String {
    let normalized = normalize_render_text(value);
    let re = regex::Regex::new(r"[A-Za-z][A-Za-z'-]{2,}").unwrap();
    if let Some(mat) = re.find(&normalized) {
        format!("{}?", mat.as_str().to_uppercase())
    } else {
        normalized.to_uppercase()
    }
}

fn normalize_render_text(value: &str) -> String {
    repair_mojibake(value)
        .chars()
        .map(|ch| match ch {
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
            '\u{2018}' | '\u{2019}' | '\u{201B}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201F}' => '"',
            '\u{00A0}' => ' ',
            '\u{FE0F}' | '\u{20E3}' | '\u{200D}' => ' ',
            _ => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn repair_mojibake(value: &str) -> String {
    let looks_broken = value.contains('\u{00E2}')
        || value.contains('\u{00F0}')
        || value.contains('\u{00C3}');
    if !looks_broken {
        return value.to_string();
    }

    let (encoded, _, had_unmappables) = WINDOWS_1252.encode(value);
    if had_unmappables {
        return value.to_string();
    }

    match String::from_utf8(encoded.into_owned()) {
        Ok(fixed) if fixed.chars().any(|ch| matches!(ch, '\u{2018}'..='\u{201F}' | '\u{2026}' | '\u{2010}'..='\u{2014}')) => fixed,
        Ok(fixed) if !fixed.contains('\u{00E2}') && !fixed.contains('\u{00C3}') => fixed,
        _ => value.to_string(),
    }
}

fn line_step(font: &FontArc, scale: PxScale) -> f32 {
    font.as_scaled(scale).height() + 12.0
}

fn text_block_height(font: &FontArc, scale: PxScale, lines: &[String]) -> f32 {
    if lines.is_empty() {
        0.0
    } else {
        line_step(font, scale) * lines.len() as f32
    }
}

fn strip_leading_list_number(value: &str) -> Option<String> {
    let digit_end = value
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .last()?;

    let rest = &value[digit_end..];
    if rest.is_empty() {
        return None;
    }

    if !rest.starts_with(char::is_whitespace) && !rest.starts_with('.') && !rest.starts_with(')') {
        return None;
    }

    let rest = rest.trim_start();
    let rest = rest
        .strip_prefix('.')
        .or_else(|| rest.strip_prefix(')'))
        .unwrap_or(rest)
        .trim_start();

    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

fn script_prefers_numbered_list(script: &Script) -> bool {
    normalize_render_text(&script.title)
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
        || script
            .points
            .iter()
            .map(|point| normalize_render_text(point))
            .any(|point| strip_leading_list_number(&point).is_some())
}

fn point_text(script: &Script, spec: &LayoutSpec, layout: &str, point_index: usize) -> String {
    let mut point = normalize_render_text(&script.points[point_index]);

    if layout == "progress_reveal" {
        return format!(
            "{}/{}  {}",
            point_index + 1,
            script.points.len().max(1),
            strip_leading_list_number(&point).unwrap_or(point)
        );
    }

    if spec.marker == "- " && script_prefers_numbered_list(script) {
        point = strip_leading_list_number(&point).unwrap_or(point);
        return format!("{}. {}", point_index + 1, point);
    }

    normalize_render_text(&format!("{}{}", spec.marker, point))
}

fn point_text_lines(script: &Script, spec: &LayoutSpec, font: &FontArc, layout: &str, point_index: usize) -> Vec<String> {
    let scale = PxScale::from(spec.point.font_size);
    let text = point_text(script, spec, layout, point_index);
    wrap_text(font, scale, &text, spec.point.width)
}

fn cta_position_y(
    script: &Script,
    spec: &LayoutSpec,
    font: &FontArc,
    layout: &str,
    cta_lines: &[String],
    cta_scale: PxScale,
) -> f32 {
    let point_scale = PxScale::from(spec.point.font_size);
    let mut point_bottom = 0.0_f32;

    for idx in 0..script.points.len() {
        let lines = point_text_lines(script, spec, font, layout, idx);
        let (_, y, _) = point_position(script, spec, font, layout, idx);
        point_bottom = point_bottom.max(y + text_block_height(font, point_scale, &lines));
    }

    let gap = (spec.point.font_size * 0.55).max(34.0);
    let desired_y = point_bottom + gap;
    let cta_height = text_block_height(font, cta_scale, cta_lines);
    let max_y = HEIGHT as f32 - cta_height - 105.0;

    desired_y.max(spec.cta.y).min(max_y)
}

fn point_position(script: &Script, spec: &LayoutSpec, font: &FontArc, layout: &str, point_index: usize) -> (f32, f32, String) {
    let scale = PxScale::from(spec.point.font_size);
    let block_gap = (spec.point.font_size * 0.45).max(26.0);

    match layout {
        "grid_layout" | "masonry_layout" | "collage_layout" | "auto_fit_tiles" => {
            let column_gap = match layout {
                "masonry_layout" | "collage_layout" => 465.0,
                _ => 470.0,
            };
            let row_offset = match layout {
                "masonry_layout" => 58.0,
                "collage_layout" => 46.0,
                _ => 0.0,
            };
            let mut left_y = spec.point.y;
            let mut right_y = spec.point.y;

            for idx in 0..point_index {
                let prev_lines = point_text_lines(script, spec, font, layout, idx);
                let prev_height = text_block_height(font, scale, &prev_lines) + block_gap;
                if idx.is_multiple_of(2) {
                    left_y += prev_height;
                } else {
                    right_y += prev_height;
                }
            }

            if point_index.is_multiple_of(2) {
                (spec.point.x, left_y, spec.point.align.clone())
            } else {
                (spec.point.x + column_gap, right_y + row_offset, spec.point.align.clone())
            }
        }
        _ => {
            let mut y = spec.point.y;
            for idx in 0..point_index {
                let prev_lines = point_text_lines(script, spec, font, layout, idx);
                y += text_block_height(font, scale, &prev_lines) + block_gap;
            }

            let (x, align) = if layout == "alternating_rows" && point_index % 2 == 1 {
                (spec.point.x + 120.0, "right".to_string())
            } else {
                (spec.point.x, spec.point.align.clone())
            };

            (x, y, align)
        }
    }
}

pub fn get_text_bbox(font: &FontArc, scale: PxScale, text: &str) -> (f32, f32) {
    let scaled_font = font.as_scaled(scale);
    let mut width = 0.0;
    let mut last_glyph_id = None;

    for c in text.chars() {
        let glyph_id = font.glyph_id(c);
        let mut w = scaled_font.h_advance(glyph_id);
        if let Some(last) = last_glyph_id {
            w += scaled_font.kern(last, glyph_id);
        }
        width += w;
        last_glyph_id = Some(glyph_id);
    }
    (width, scaled_font.height())
}

pub fn wrap_text(font: &FontArc, scale: PxScale, text: &str, max_width: f32) -> Vec<String> {
    let words = text.split_whitespace();
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in words {
        let candidate = if current_line.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current_line, word)
        };
        let (width, _) = get_text_bbox(font, scale, &candidate);
        if width > max_width && !current_line.is_empty() {
            lines.push(current_line);
            current_line = word.to_string();
        } else {
            current_line = candidate;
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    lines
}

#[allow(clippy::too_many_arguments)]
pub fn draw_text_lines(
    img: &mut RgbaImage,
    font: &FontArc,
    scale: PxScale,
    lines: &[String],
    start_x: f32,
    start_y: f32,
    max_width: f32,
    align: &str,
    color: Rgba<u8>,
    shadow_color: Rgba<u8>,
) -> f32 {
    let mut cursor_y = start_y;

    for line in lines {
        let (width, height) = get_text_bbox(font, scale, line);
        let x = match align {
            "left" => start_x,
            "right" => start_x + max_width - width,
            _ => start_x + (max_width - width) / 2.0,
        };

        // Draw drop shadow
        draw_line_raw(img, font, scale, line, x + 3.0, cursor_y + 4.0, shadow_color);
        // Draw main text
        draw_line_raw(img, font, scale, line, x, cursor_y, color);

        cursor_y += height + 12.0;
    }
    cursor_y
}

fn draw_line_raw(
    img: &mut RgbaImage,
    font: &FontArc,
    scale: PxScale,
    text: &str,
    x: f32,
    y: f32,
    color: Rgba<u8>,
) {
    let scaled_font = font.as_scaled(scale);
    let mut current_x = x;
    let mut last_glyph_id = None;

    for c in text.chars() {
        let glyph_id = font.glyph_id(c);
        if let Some(last) = last_glyph_id {
            current_x += scaled_font.kern(last, glyph_id);
        }

        let glyph = Glyph {
            id: glyph_id,
            scale,
            position: ab_glyph::point(current_x, y + scaled_font.ascent()),
        };

        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|px, py, v| {
                let gx = (bounds.min.x + px as f32) as i32;
                let gy = (bounds.min.y + py as f32) as i32;
                if gx >= 0 && gx < WIDTH as i32 && gy >= 0 && gy < HEIGHT as i32 {
                    let pixel = img.get_pixel_mut(gx as u32, gy as u32);
                    let alpha = (v * color.0[3] as f32) as u8;
                    if alpha > 0 {
                        // Alpha blend
                        let a_f = alpha as f32 / 255.0;
                        let dst_a = pixel.0[3] as f32 / 255.0;
                        let out_a = a_f + dst_a * (1.0 - a_f);
                        if out_a > 0.0 {
                            let out_r = (color.0[0] as f32 * a_f + pixel.0[0] as f32 * dst_a * (1.0 - a_f)) / out_a;
                            let out_g = (color.0[1] as f32 * a_f + pixel.0[1] as f32 * dst_a * (1.0 - a_f)) / out_a;
                            let out_b = (color.0[2] as f32 * a_f + pixel.0[2] as f32 * dst_a * (1.0 - a_f)) / out_a;
                            *pixel = Rgba([out_r as u8, out_g as u8, out_b as u8, (out_a * 255.0) as u8]);
                        }
                    }
                }
            });
        }

        current_x += scaled_font.h_advance(glyph_id);
        last_glyph_id = Some(glyph_id);
    }
}

pub fn make_overlay(
    script: &Script,
    layer_index: usize,
    output_folder: &Path,
    stamp: &str,
    font: &FontArc,
) -> Result<PathBuf, std::io::Error> {
    let mut image = RgbaImage::from_pixel(WIDTH, HEIGHT, Rgba([0, 0, 0, 0]));

    let layout = normalize_layout(&script.layout);
    let spec = get_layout_spec(&layout);

    if layer_index == 0 {
        let param = &spec.title;
        let scale = PxScale::from(param.font_size);
        let mut title_text = if layout == "one_word_hook" {
            first_hook_word(&script.title)
        } else {
            normalize_render_text(&script.title)
        };
        if layout == "quote_style" {
            title_text = format!("\"{}\"", title_text.trim_matches('"'));
        }
        let lines = wrap_text(font, scale, &title_text, param.width);
        draw_text_lines(
            &mut image,
            font,
            scale,
            &lines,
            param.x,
            param.y,
            param.width,
            &param.align,
            Rgba([255, 255, 255, 255]),
            Rgba([0, 0, 0, 180]),
        );

    } else {
        let point_index = layer_index - 1;
        if point_index < script.points.len() {
            let scale = PxScale::from(spec.point.font_size);
            let lines = point_text_lines(script, &spec, font, &layout, point_index);
            let (point_x, point_y, point_align) = point_position(script, &spec, font, &layout, point_index);
            draw_text_lines(
                &mut image,
                font,
                scale,
                &lines,
                point_x,
                point_y,
                spec.point.width,
                &point_align,
                Rgba([255, 255, 255, 255]),
                Rgba([0, 0, 0, 185]),
            );
        } else if point_index == script.points.len() && !script.cta.is_empty() {
            let scale = PxScale::from(spec.cta.font_size);
            let cta_text = normalize_render_text(&script.cta);
            let lines = wrap_text(font, scale, &cta_text, spec.cta.width);
            let cta_y = cta_position_y(script, &spec, font, &layout, &lines, scale);
            draw_text_lines(
                &mut image,
                font,
                scale,
                &lines,
                spec.cta.x,
                cta_y,
                spec.cta.width,
                &spec.cta.align,
                Rgba([255, 255, 255, 240]),
                Rgba([0, 0, 0, 180]),
            );
        }
    }

    fs::create_dir_all(output_folder)?;
    let file_name = format!("{}-{}-{}.png", slugify(&script.title), stamp, layer_index + 1);
    let output_path = output_folder.join(file_name);
    image.save(&output_path).map_err(std::io::Error::other)?;
    Ok(output_path)
}

