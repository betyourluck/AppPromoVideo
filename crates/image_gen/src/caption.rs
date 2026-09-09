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
    /// 縦位置を直接指定する (rev14、canvas 高さ比 0.0〜1.0 = 文字ブロックの上端)。
    /// `None` なら `position` + `margin_ratio` の従来どおり。上下に収まらない値は端で丸める。
    pub y_ratio: Option<f32>,
}

impl<'a> Caption<'a> {
    pub fn new(text: &'a str, font_data: &'a [u8], font_index: u32) -> Self {
        Caption { text, font_data, font_index, size_ratio: 0.055, color: [255, 255, 255, 255], position: CaptionPosition::Bottom, margin_ratio: 0.06, y_ratio: None }
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
/// 文字ブロックの上端 (canvas 座標、純粋)。枠のプレビューが焼き込みと同じ数式を使うために公開する。
pub fn caption_top(canvas_h: u32, block_h: f32, cap: &Caption<'_>) -> f32 {
    let h = canvas_h as f32;
    match cap.y_ratio {
        Some(r) => (h * r).clamp(0.0, (h - block_h).max(0.0)),
        None => match cap.position {
            CaptionPosition::Top => h * cap.margin_ratio,
            CaptionPosition::Bottom => h - h * cap.margin_ratio - block_h,
        },
    }
}

/// 1 行の版組み (canvas 座標)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaidOutLine {
    pub text: String,
    /// 左端 (中央揃えの結果)。
    pub x: f32,
    pub width: f32,
    /// 上端。高さは [`CaptionLayout::line_h`]。
    pub top: f32,
}

/// 見出しの版組み (契約 `caption_layout`、rev18)。**焼き込みとプレビューはこの 1 つの結果を共有する。**
/// `plate_quad` と同じ作法 — 数式を TS 側へ写すと必ず食い違うので、寸法は Rust が出す。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptionLayout {
    /// 自動縮小の後に実際に使う文字の高さ (px)。
    pub px: f32,
    pub line_h: f32,
    /// 文字ブロックの上端。
    pub top: f32,
    pub block_h: f32,
    pub lines: Vec<LaidOutLine>,
}

/// 版組みだけを計算する (**画素を触らない**ので速い — フォントの幅送りを測るだけ)。
///
/// 空文字・空行だけなら `None`。`burn_caption` はこの結果をそのまま描くので、
/// ここが返す矩形は**焼き上がりの文字の位置と大きさそのもの**である
/// (グリフの墨は矩形よりわずかに内側に入る。行の高さは字面ではなく行送り)。
pub fn caption_layout(canvas_w: u32, canvas_h: u32, cap: &Caption<'_>) -> Result<Option<CaptionLayout>, String> {
    if cap.text.trim().is_empty() {
        return Ok(None);
    }
    let font = FontRef::try_from_slice_and_index(cap.font_data, cap.font_index).map_err(|e| format!("フォントを読めません: {e}"))?;
    let lines: Vec<&str> = cap.text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Ok(None);
    }
    let (w, h) = (canvas_w, canvas_h);
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
    let top = caption_top(h, block_h, cap);
    let laid = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let width = line_width(&f, line);
            LaidOutLine { text: (*line).to_string(), x: (w as f32 - width) / 2.0, width, top: top + line_h * i as f32 }
        })
        .collect();
    Ok(Some(CaptionLayout { px, line_h, top, block_h, lines: laid }))
}

pub fn burn_caption(png: &[u8], cap: &Caption<'_>) -> Result<Vec<u8>, String> {
    let mut img = image::load_from_memory(png).map_err(|e| format!("画像を読めません: {e}"))?.to_rgba8();
    let (w, h) = img.dimensions();
    // 版組みはプレビューと同じ関数から取る (数式の写しを持たない)。
    let Some(layout) = caption_layout(w, h, cap)? else {
        return Ok(png.to_vec());
    };
    let font = FontRef::try_from_slice_and_index(cap.font_data, cap.font_index).map_err(|e| format!("フォントを読めません: {e}"))?;
    let f = font.as_scaled(PxScale::from(layout.px));
    let shadow = [0, 0, 0, 190];
    let shadow_off = (layout.px * 0.05).max(1.5);
    for l in &layout.lines {
        let baseline = l.top + f.ascent();
        draw_line(&mut img, &f, &l.text, l.x + shadow_off, baseline + shadow_off, shadow);
        draw_line(&mut img, &f, &l.text, l.x, baseline, cap.color);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(img).write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{FontSource, describe_font_file, system_font_dirs};

    pub(super) fn find_font() -> Option<(Vec<u8>, u32)> {
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

    pub(super) fn solid(w: u32, h: u32) -> Vec<u8> {
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

#[cfg(test)]
mod layout_tests {
    use super::tests::{find_font, solid};
    use super::*;

    fn cap_with<'a>(text: &'a str, font: &'a [u8], index: u32) -> Caption<'a> {
        let mut c = Caption::new(text, font, index);
        c.size_ratio = 0.12;
        c
    }

    /// **版組みは焼き上がりと一致していなければ意味がない** (枠の用途はそれだけ)。
    /// 焼いた墨の外接矩形が、返した矩形の中に収まることを見る。
    #[test]
    fn the_box_contains_the_ink_it_promises() {
        let Some((font, index)) = find_font() else {
            eprintln!("システムフォントが見つからないので飛ばします");
            return;
        };
        let png = solid(400, 300);
        let mut cap = cap_with("Ag", &font, index);
        cap.y_ratio = Some(0.30);
        let l = caption_layout(400, 300, &cap).unwrap().unwrap();
        let line = &l.lines[0];

        let burned = burn_caption(&png, &cap).unwrap();
        let img = image::load_from_memory(&burned).unwrap().to_rgba8();
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        for (x, y, p) in img.enumerate_pixels() {
            // 落ち影も含めた「元の背景ではない画素」。
            if *p != image::Rgba([30, 30, 40, 255]) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
        assert!(x1 > x0, "何か焼かれている");
        // 落ち影のぶん右下にはみ出すので、そのぶんだけ緩める。
        let slack = (l.px * 0.05).max(1.5) + 1.0;
        assert!(x0 as f32 >= line.x - 1.0, "左: 墨 {x0} vs 枠 {}", line.x);
        assert!(x1 as f32 <= line.x + line.width + slack, "右: 墨 {x1} vs 枠 {}", line.x + line.width);
        assert!(y0 as f32 >= line.top - 1.0, "上: 墨 {y0} vs 枠 {}", line.top);
        assert!(y1 as f32 <= line.top + l.line_h + slack, "下: 墨 {y1} vs 枠 {}", line.top + l.line_h);
    }

    /// 縦位置を動かすと枠も動く (焼き込みと同じ `caption_top` を通っている証拠)。
    #[test]
    fn the_box_follows_y_ratio() {
        let Some((font, index)) = find_font() else { return };
        let mut cap = cap_with("A", &font, index);
        cap.y_ratio = Some(0.10);
        let high = caption_layout(400, 300, &cap).unwrap().unwrap();
        cap.y_ratio = Some(0.70);
        let low = caption_layout(400, 300, &cap).unwrap().unwrap();
        assert!(high.top < low.top);
        assert_eq!(high.px, low.px, "縦位置は大きさに影響しない");
    }

    /// 幅からはみ出す行は縮む。**縮んだ後の値**が返る (指定値をそのまま返すと枠が嘘をつく)。
    #[test]
    fn a_long_line_shrinks_and_the_box_reports_the_shrunk_size() {
        let Some((font, index)) = find_font() else { return };
        let short = caption_layout(400, 300, &cap_with("A", &font, index)).unwrap().unwrap();
        let long = caption_layout(400, 300, &cap_with(&"A".repeat(60), &font, index)).unwrap().unwrap();
        assert!(long.px < short.px, "はみ出す行は縮む");
        assert!(long.lines[0].width <= 400.0 * 0.9 + 1.0, "縮んだ後は 90% に収まる");
    }

    #[test]
    fn empty_text_has_no_layout() {
        assert!(caption_layout(400, 300, &Caption::new("  \n ", b"not a font", 0)).unwrap().is_none());
    }
}
