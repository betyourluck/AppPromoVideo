//! 製品カットの合成 (spec 01 rev3、2026-09-08 ユーザー FB「スクショに全く従わない」起点)。
//!
//! 画像モデルに実スクショを「参考」で渡すと、色味だけ拾って画面を発明する (Gemini で実測)。
//! 忠実さはプロンプトでは保証できないので、**背景 (舞台) だけをモデルに描かせ、実スクショの画素は
//! Rust がそのまま貼る**。これで「ありえない画面」は構造的に出ない。
//!
//! 合成は純粋 (バイト列 → バイト列): 背景を canvas に cover で敷き、スクショを中央に等比で収め、
//! 角丸 + 落ち影を付ける。見出しの焼き込みは [`crate::caption`] (opt-in)。見出し付きなら `with_caption_band` で帯を空ける。

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage, imageops};

/// 配置 (契約 `compose.layout`)。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
    /// スクショが占める最大比 (canvas の幅・高さに対して)。
    pub screen_ratio: f32,
    pub corner_radius: u32,
    pub shadow: bool,
    /// 縦のずらし (canvas 高さ比、負 = 上へ)。見出しの帯を空けるために使う。
    pub y_offset_ratio: f32,
}

impl Layout {
    pub fn for_canvas(width: u32, height: u32) -> Self {
        Layout { width, height, screen_ratio: 0.78, corner_radius: (width.min(height) / 60).max(6), shadow: true, y_offset_ratio: 0.0 }
    }

    /// 見出しの帯を空けた配置 (`bottom` なら下 / `top` なら上に帯)。スクショは少し小さく、反対側へ寄せる。
    pub fn with_caption_band(mut self, at_top: bool) -> Self {
        self.screen_ratio = 0.68;
        self.y_offset_ratio = if at_top { 0.075 } else { -0.075 };
        self
    }
}

fn load(bytes: &[u8], what: &str) -> Result<DynamicImage, String> {
    image::load_from_memory(bytes).map_err(|e| format!("{what} を読めません: {e}"))
}

/// 角丸マスクを alpha に掛ける (純粋)。
fn round_corners(img: &mut RgbaImage, r: u32) {
    let (w, h) = img.dimensions();
    if r == 0 || w < 2 * r || h < 2 * r {
        return;
    }
    let r_f = r as f32;
    for y in 0..h {
        for x in 0..w {
            let cx = if x < r { r_f - x as f32 - 0.5 } else if x >= w - r { x as f32 + 0.5 - (w - r) as f32 } else { 0.0 };
            let cy = if y < r { r_f - y as f32 - 0.5 } else if y >= h - r { y as f32 + 0.5 - (h - r) as f32 } else { 0.0 };
            if cx > 0.0 && cy > 0.0 {
                let d = (cx * cx + cy * cy).sqrt();
                if d > r_f {
                    let p = img.get_pixel_mut(x, y);
                    // 1px の縁だけ半透明にしてジャギーを和らげる。
                    let a = if d - r_f < 1.0 { ((1.0 - (d - r_f)) * p[3] as f32) as u8 } else { 0 };
                    p[3] = a;
                }
            }
        }
    }
}

/// 単色の背景 (モデルが背景を作れなかった時の fallback。palette の 1 色)。
pub fn solid_backdrop(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let img = RgbaImage::from_pixel(width, height, Rgba([rgb[0], rgb[1], rgb[2], 255]));
    encode_png(&img)
}

fn encode_png(img: &RgbaImage) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(img.clone()).write_to(&mut out, image::ImageFormat::Png).expect("PNG encode");
    out.into_inner()
}

/// 背景 + 実スクショ → 製品カット (PNG)。スクショの画素は等比縮小以外いじらない。
pub fn composite_product_cut(background: &[u8], screenshot: &[u8], layout: &Layout) -> Result<Vec<u8>, String> {
    let bg = load(background, "背景")?.resize_to_fill(layout.width, layout.height, FilterType::Lanczos3).to_rgba8();
    let shot = load(screenshot, "スクリーンショット")?;
    let (sw, sh) = shot.dimensions();
    let max_w = (layout.width as f32 * layout.screen_ratio) as u32;
    let max_h = (layout.height as f32 * layout.screen_ratio) as u32;
    let scale = (max_w as f32 / sw as f32).min(max_h as f32 / sh as f32).min(1.0);
    let tw = ((sw as f32 * scale) as u32).max(1);
    let th = ((sh as f32 * scale) as u32).max(1);
    let mut shot = shot.resize_exact(tw, th, FilterType::Lanczos3).to_rgba8();
    round_corners(&mut shot, layout.corner_radius);
    let x = ((layout.width - tw) / 2) as i64;
    let y = ((layout.height - th) / 2) as i64 + (layout.height as f32 * layout.y_offset_ratio) as i64;

    let mut canvas = bg;
    if layout.shadow {
        let pad = 24u32;
        let mut shadow = RgbaImage::from_pixel(tw + pad * 2, th + pad * 2, Rgba([0, 0, 0, 0]));
        for yy in 0..th {
            for xx in 0..tw {
                shadow.put_pixel(xx + pad, yy + pad, Rgba([0, 0, 0, 150]));
            }
        }
        round_corners(&mut shadow, layout.corner_radius + pad / 2);
        let shadow = imageops::blur(&shadow, 10.0);
        imageops::overlay(&mut canvas, &shadow, x - pad as i64, y - pad as i64 + 14);
    }
    imageops::overlay(&mut canvas, &shot, x, y);
    Ok(encode_png(&canvas))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        solid_backdrop(w, h, rgb)
    }

    fn decode(bytes: &[u8]) -> RgbaImage {
        image::load_from_memory(bytes).unwrap().to_rgba8()
    }

    #[test]
    fn screenshot_pixels_land_unchanged_in_the_center_and_canvas_has_layout_size() {
        let bg = png(400, 300, [40, 40, 40]);
        let shot = png(1920, 1080, [255, 0, 200]); // 16:9 のマゼンタ画面
        let layout = Layout::for_canvas(1344, 768);
        let out = decode(&composite_product_cut(&bg, &shot, &layout).unwrap());
        assert_eq!(out.dimensions(), (1344, 768));
        let c = out.get_pixel(672, 384);
        assert_eq!([c[0], c[1], c[2]], [255, 0, 200], "中央はスクショの画素そのまま");
        // スクショは幅 78% = 1048 に収まり、canvas の左端は背景のまま。
        let e = out.get_pixel(20, 384);
        assert_eq!([e[0], e[1], e[2]], [40, 40, 40]);
    }

    #[test]
    fn corners_are_rounded_and_portrait_screenshot_fits_by_height() {
        let bg = png(100, 100, [10, 20, 30]);
        let shot = png(600, 1200, [0, 255, 0]); // 縦長
        let layout = Layout { width: 800, height: 600, screen_ratio: 0.8, corner_radius: 40, shadow: false, y_offset_ratio: 0.0 };
        let out = decode(&composite_product_cut(&bg, &shot, &layout).unwrap());
        // 高さ 480 に収まる → 幅 240。左上角 (角丸の外) は背景色。
        let x0 = (800 - 240) / 2;
        let y0 = (600 - 480) / 2;
        let corner = out.get_pixel(x0, y0);
        assert_eq!([corner[0], corner[1], corner[2]], [10, 20, 30], "角丸で角は背景");
        let inner = out.get_pixel(x0 + 120, y0 + 240);
        assert_eq!([inner[0], inner[1], inner[2]], [0, 255, 0]);
    }

    #[test]
    fn small_screenshot_is_not_upscaled() {
        let bg = png(100, 100, [0, 0, 0]);
        let shot = png(200, 100, [255, 255, 255]);
        let layout = Layout { width: 1000, height: 500, screen_ratio: 0.9, corner_radius: 0, shadow: false, y_offset_ratio: 0.0 };
        let out = decode(&composite_product_cut(&bg, &shot, &layout).unwrap());
        // 等倍のまま中央 (400..600, 200..300)。
        assert_eq!(out.get_pixel(500, 250)[0], 255);
        assert_eq!(out.get_pixel(390, 250)[0], 0);
    }

    /// 見出しの帯: 下に帯なら中心より上に寄り、下端 12% は背景のまま (文字の置き場)。
    #[test]
    fn caption_band_shifts_the_screenshot_and_leaves_room() {
        let bg = png(200, 100, [5, 5, 5]);
        let shot = png(1920, 1080, [200, 0, 0]);
        let layout = Layout::for_canvas(1344, 768).with_caption_band(false);
        let out = decode(&composite_product_cut(&bg, &shot, &layout).unwrap());
        // 下端 12% の帯 (y >= 676) に赤は無い。
        let red_in_band = (676..768).flat_map(|y| (0..1344).map(move |x| (x, y))).filter(|&(x, y)| out.get_pixel(x, y)[0] > 150).count();
        assert_eq!(red_in_band, 0, "下の帯にスクショが掛かっている");
        // 中央付近は赤 (スクショは残っている)。
        assert!(out.get_pixel(672, 330)[0] > 150);
        let top = Layout::for_canvas(1344, 768).with_caption_band(true);
        let out2 = decode(&composite_product_cut(&bg, &shot, &top).unwrap());
        let red_top = (0..92).flat_map(|y| (0..1344).map(move |x| (x, y))).filter(|&(x, y)| out2.get_pixel(x, y)[0] > 150).count();
        assert_eq!(red_top, 0);
    }

    #[test]
    fn garbage_input_is_an_error_not_a_panic() {
        assert!(composite_product_cut(b"nope", &png(10, 10, [0, 0, 0]), &Layout::for_canvas(100, 100)).is_err());
        assert!(composite_product_cut(&png(10, 10, [0, 0, 0]), b"nope", &Layout::for_canvas(100, 100)).is_err());
    }
}
