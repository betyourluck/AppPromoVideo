//! 見出し (copy) の焼き込み (契約 `caption`、opt-in・既定 OFF)。
//!
//! 「テロップはアプリで焼くか、手で焼くか」はユーザー判断待ち (2026-09-08)。ここは機構だけ:
//! PNG のバイト列に、指定フォントで文字列を中央揃えで描く (複数行は `\n`)。読みやすさのため
//! 文字の下に落ち影 (黒・半透明・2px ずらし) を敷く。長い行は幅に収まるまで自動で縮める。
//! 純粋 (バイト列 + フォントデータ → バイト列)。フォントの選択は [`crate::fonts`]。

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{DynamicImage, RgbaImage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaptionPosition {
    Top,
    #[default]
    Bottom,
}

/// 焼き込みの指定。`size_ratio` = 文字の高さ / canvas の高さ (0.05 前後が見出し向き)。
#[derive(Debug, Clone, PartialEq)]
pub struct Caption<'a> {
    pub text: &'a str,
    pub font_data: &'a [u8],
    pub font_index: u32,
    pub size_ratio: f32,
    pub color: [u8; 4],
    pub position: CaptionPosition,
    /// canvas の高さに対する余白比。
    pub margin_ratio: f32,
}

impl<'a> Caption<'a> {
    pub fn new(text: &'a str, font_data: &'a [u8], font_index: u32) -> Self {
        Caption { text, font_data, font_index, size_ratio: 0.055, color: [255, 255, 255, 255], position: CaptionPosition::Bottom, margin_ratio: 0.06 }
    }
}

fn line_width<F: Font, S: ScaleFont<F>>(font: &S, line: &str) -> f32 {
    let mut w = 0.0;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    for c in line.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            w += font.kern(p, id);
        }
        w += font.h_advance(id);
        prev = Some(id);
    }
    w
}

fn blend(img: &mut RgbaImage, x: i64, y: i64, color: [u8; 4], coverage: f32) {
    if x < 0 || y < 0 || x >= img.width() as i64 || y >= img.height() as i64 {
        return;
    }
    let a = (color[3] as f32 / 255.0) * coverage.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let p = img.get_pixel_mut(x as u32, y as u32);
    for i in 0..3 {
        p[i] = (color[i] as f32 * a + p[i] as f32 * (1.0 - a)).round() as u8;
    }
    p[3] = 255;
}

fn draw_line<F: Font, S: ScaleFont<F>>(img: &mut RgbaImage, font: &S, line: &str, x0: f32, baseline: f32, color: [u8; 4]) {
    let mut x = x0;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    for c in line.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            x += font.kern(p, id);
        }
        let glyph = id.with_scale_and_position(font.scale(), ab_glyph::point(x, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let b = outlined.px_bounds();
            outlined.draw(|gx, gy, cov| blend(img, b.min.x as i64 + gx as i64, b.min.y as i64 + gy as i64, color, cov));
        }
        x += font.h_advance(id);
        prev = Some(id);
    }
}

/// PNG に見出しを焼く。空文字なら入力をそのまま返す。
pub fn burn_caption(png: &[u8], cap: &Caption<'_>) -> Result<Vec<u8>, String> {
    if cap.text.trim().is_empty() {
        return Ok(png.to_vec());
    }
    let font = FontRef::try_from_slice_and_index(cap.font_data, cap.font_index).map_err(|e| format!("フォントを読めません: {e}"))?;
    let mut img = image::load_from_memory(png).map_err(|e| format!("画像を読めません: {e}"))?.to_rgba8();
    let (w, h) = img.dimensions();
    let lines: Vec<&str> = cap.text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Ok(png.to_vec());
    }
    let max_w = w as f32 * 0.9;
    // 幅に収まるまで縮める (下限は canvas 高さの 2%)。
    let mut px = (h as f32 * cap.size_ratio).max(8.0);
    let min_px = (h as f32 * 0.02).max(8.0);
    let scaled_at = |px: f32| font.as_scaled(PxScale::from(px));
    while px > min_px {
        let f = scaled_at(px);
        if lines.iter().all(|l| line_width(&f, l) <= max_w) {
            break;
        }
        px *= 0.92;
    }
    let f = scaled_at(px);
    let line_h = px * 1.3;
    let block_h = line_h * lines.len() as f32;
    let margin = h as f32 * cap.margin_ratio;
    let top = match cap.position {
        CaptionPosition::Top => margin,
        CaptionPosition::Bottom => h as f32 - margin - block_h,
    };
    let shadow = [0, 0, 0, 190];
    let shadow_off = (px * 0.05).max(1.5);
    for (i, line) in lines.iter().enumerate() {
        let lw = line_width(&f, line);
        let x0 = (w as f32 - lw) / 2.0;
        let baseline = top + line_h * i as f32 + f.ascent();
        draw_line(&mut img, &f, line, x0 + shadow_off, baseline + shadow_off, shadow);
        draw_line(&mut img, &f, line, x0, baseline, cap.color);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(img).write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{FontSource, describe_font_file, system_font_dirs};

    fn find_font() -> Option<(Vec<u8>, u32)> {
        let dir = system_font_dirs().into_iter().find(|d| d.is_dir())?;
        for c in ["BIZ-UDGothicB.ttc", "YuGothM.ttc", "meiryo.ttc", "msgothic.ttc", "arial.ttf", "DejaVuSans.ttf"] {
            let p = dir.join(c);
            if p.exists() {
                let faces = describe_font_file(&p, FontSource::System);
                let idx = faces.first().map(|f| f.index).unwrap_or(0);
                return Some((std::fs::read(&p).ok()?, idx));
            }
        }
        None
    }

    fn solid(w: u32, h: u32) -> Vec<u8> {
        crate::compose::solid_backdrop(w, h, [30, 30, 40])
    }

    #[test]
    fn empty_text_is_identity() {
        let png = solid(64, 32);
        let cap = Caption::new("  \n ", b"not a font", 0);
        assert_eq!(burn_caption(&png, &cap).unwrap(), png);
    }

    #[test]
    fn bad_font_is_an_error() {
        let png = solid(64, 32);
        assert!(burn_caption(&png, &Caption::new("x", b"garbage", 0)).is_err());
    }

    /// 実フォントがある機体で: 下部の帯に白い画素が現れ、上部は無傷。長文は縮んで幅に収まる。
    #[test]
    fn burns_text_into_the_bottom_band_and_shrinks_to_fit() {
        let Some((data, idx)) = find_font() else { return };
        let png = solid(800, 450);
        let cap = Caption::new("整理するほど、時間は増える。", &data, idx);
        let out = image::load_from_memory(&burn_caption(&png, &cap).unwrap()).unwrap().to_rgba8();
        let bright = |y0: u32, y1: u32| (y0..y1).flat_map(|y| (0..800).map(move |x| (x, y))).filter(|&(x, y)| out.get_pixel(x, y)[0] > 200).count();
        assert!(bright(380, 450) > 50, "下部に文字が無い");
        assert_eq!(bright(0, 200), 0, "上部は無傷");
        // 極端に長い 1 行でも右端を突き抜けない (右端 2% は無傷)。
        let long = "あ".repeat(200);
        let cap2 = Caption { text: &long, ..Caption::new("", &data, idx) };
        let out2 = image::load_from_memory(&burn_caption(&png, &cap2).unwrap()).unwrap().to_rgba8();
        let edge = (0..450).filter(|&y| out2.get_pixel(799, y)[0] > 200 || out2.get_pixel(0, y)[0] > 200).count();
        assert_eq!(edge, 0);
    }
}
