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

/// 面の傾き (契約 `Scene.plate_tilt`、rev5)。度。yaw 正 = 右辺が奥、pitch 正 = 下辺が奥 (見上げる)。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Tilt {
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
}

impl Tilt {
    pub fn is_zero(&self) -> bool {
        self.yaw_degrees.abs() < f32::EPSILON && self.pitch_degrees.abs() < f32::EPSILON
    }
}

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
    /// 面の傾き (rev5)。`None` / ゼロなら従来どおり正対で貼る (画素等価)。
    pub tilt: Option<Tilt>,
}

impl Layout {
    pub fn for_canvas(width: u32, height: u32) -> Self {
        Layout {
            width,
            height,
            screen_ratio: 0.78,
            corner_radius: (width.min(height) / 60).max(6),
            shadow: true,
            y_offset_ratio: 0.0,
            tilt: None,
        }
    }

    /// 面を傾ける (契約 `PlateMode::perspective`)。ゼロなら正対の経路のまま。
    pub fn with_tilt(mut self, tilt: Tilt) -> Self {
        self.tilt = if tilt.is_zero() { None } else { Some(tilt) };
        self
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
            let cx = if x < r {
                r_f - x as f32 - 0.5
            } else if x >= w - r {
                x as f32 + 0.5 - (w - r) as f32
            } else {
                0.0
            };
            let cy = if y < r {
                r_f - y as f32 - 0.5
            } else if y >= h - r {
                y as f32 + 0.5 - (h - r) as f32
            } else {
                0.0
            };
            if cx > 0.0 && cy > 0.0 {
                let d = (cx * cx + cy * cy).sqrt();
                if d > r_f {
                    let p = img.get_pixel_mut(x, y);
                    // 1px の縁だけ半透明にしてジャギーを和らげる。
                    let a = if d - r_f < 1.0 {
                        ((1.0 - (d - r_f)) * p[3] as f32) as u8
                    } else {
                        0
                    };
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
    DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("PNG encode");
    out.into_inner()
}

/// 面の 4 隅を 3D で yaw/pitch 回転 → ピンホール投影し、`max_w x max_h` に収まるよう正規化した
/// 4 点 (左上・右上・右下・左下、原点は面の中心) を返す (純粋)。
///
/// 遠近の強さは焦点距離 = 視距離としてあり、傾き 0 なら恒等 (元の矩形が返る)。
fn project_corners(tw: f32, th: f32, tilt: &Tilt, max_w: f32, max_h: f32) -> [(f32, f32); 4] {
    let (sy, cy) = tilt.yaw_degrees.to_radians().sin_cos();
    let (sp, cp) = tilt.pitch_degrees.to_radians().sin_cos();
    // 視距離。面の長辺の 2.5 倍 = 穏やかな遠近 (広角にすると端が破綻する)。
    let d = tw.max(th) * 2.5;
    let corners = [
        (-tw / 2.0, -th / 2.0),
        (tw / 2.0, -th / 2.0),
        (tw / 2.0, th / 2.0),
        (-tw / 2.0, th / 2.0),
    ];
    let mut out = [(0.0f32, 0.0f32); 4];
    for (i, &(x, y)) in corners.iter().enumerate() {
        // yaw: Y 軸まわり。正で右辺 (x>0) が奥 (z が増える) へ。
        let (x1, z1) = (x * cy, x * sy);
        // pitch: X 軸まわり。正で下辺 (y>0) が奥へ。
        let (y2, z2) = (y * cp, z1 + y * sp);
        let denom = d + z2;
        out[i] = (d * x1 / denom, d * y2 / denom);
    }
    // 投影後の外接矩形を max に収める (拡大はしない)。
    let (mut lo_x, mut hi_x, mut lo_y, mut hi_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for &(x, y) in &out {
        lo_x = lo_x.min(x);
        hi_x = hi_x.max(x);
        lo_y = lo_y.min(y);
        hi_y = hi_y.max(y);
    }
    let k = (max_w / (hi_x - lo_x)).min(max_h / (hi_y - lo_y)).min(1.0);
    for p in &mut out {
        *p = (p.0 * k, p.1 * k);
    }
    out
}

/// 単位矩形 (0,0)-(w,h) → 4 点 の homography の**逆**を返す (dst → src の 3x3、行優先)。
/// 8 元 1 次を Gauss 消去で解く (純粋)。退化したら `None`。
fn inverse_homography(w: f32, h: f32, dst: &[(f32, f32); 4]) -> Option<[f32; 9]> {
    let src = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
    // dst → src の写像を直接解く (逆行列を取らずに済む)。
    let mut a = [[0.0f32; 9]; 8];
    for i in 0..4 {
        let (x, y) = dst[i];
        let (u, v) = src[i];
        a[i * 2] = [x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u, u];
        a[i * 2 + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v, v];
    }
    for col in 0..8 {
        let piv = (col..8).max_by(|&r1, &r2| a[r1][col].abs().total_cmp(&a[r2][col].abs()))?;
        if a[piv][col].abs() < 1e-9 {
            return None;
        }
        a.swap(col, piv);
        let p = a[col][col];
        for v in a[col].iter_mut() {
            *v /= p;
        }
        for r in 0..8 {
            if r != col {
                let f = a[r][col];
                if f != 0.0 {
                    let pivot_row = a[col];
                    for (v, pv) in a[r].iter_mut().zip(pivot_row.iter()) {
                        *v -= f * pv;
                    }
                }
            }
        }
    }
    let mut m = [0.0f32; 9];
    for (i, row) in a.iter().enumerate() {
        m[i] = row[8];
    }
    m[8] = 1.0;
    Some(m)
}

/// バイリニアで 1 画素サンプルする (範囲外は透明)。
fn sample(img: &RgbaImage, x: f32, y: f32) -> Rgba<u8> {
    let (w, h) = img.dimensions();
    if x < -0.5 || y < -0.5 || x > w as f32 - 0.5 || y > h as f32 - 0.5 {
        return Rgba([0, 0, 0, 0]);
    }
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |xx: f32, yy: f32| -> [f32; 4] {
        let xi = (xx.max(0.0) as u32).min(w - 1);
        let yi = (yy.max(0.0) as u32).min(h - 1);
        let p = img.get_pixel(xi, yi).0;
        [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]
    };
    let (a, b, c, d) = (
        at(x0, y0),
        at(x0 + 1.0, y0),
        at(x0, y0 + 1.0),
        at(x0 + 1.0, y0 + 1.0),
    );
    let mut out = [0u8; 4];
    for i in 0..4 {
        let top = a[i] + (b[i] - a[i]) * fx;
        let bot = c[i] + (d[i] - c[i]) * fx;
        out[i] = (top + (bot - top) * fy).round().clamp(0.0, 255.0) as u8;
    }
    Rgba(out)
}

/// 面を傾けて canvas に焼く (純粋)。`center` は canvas 上の面の中心。
fn warp_onto(
    canvas: &mut RgbaImage,
    plate: &RgbaImage,
    quad: &[(f32, f32); 4],
    center: (f32, f32),
) {
    let (pw, ph) = plate.dimensions();
    let Some(m) = inverse_homography(pw as f32, ph as f32, quad) else {
        return;
    };
    let (cx, cy) = center;
    let (mut lo_x, mut hi_x, mut lo_y, mut hi_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for &(x, y) in quad {
        lo_x = lo_x.min(x);
        hi_x = hi_x.max(x);
        lo_y = lo_y.min(y);
        hi_y = hi_y.max(y);
    }
    let x0 = ((cx + lo_x).floor().max(0.0)) as u32;
    let y0 = ((cy + lo_y).floor().max(0.0)) as u32;
    let x1 = ((cx + hi_x).ceil().min(canvas.width() as f32)) as u32;
    let y1 = ((cy + hi_y).ceil().min(canvas.height() as f32)) as u32;
    for py in y0..y1 {
        for px in x0..x1 {
            let (dx, dy) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
            let wz = m[6] * dx + m[7] * dy + m[8];
            if wz.abs() < 1e-9 {
                continue;
            }
            let sx = (m[0] * dx + m[1] * dy + m[2]) / wz;
            let sy = (m[3] * dx + m[4] * dy + m[5]) / wz;
            let s = sample(plate, sx - 0.5, sy - 0.5);
            if s[3] == 0 {
                continue;
            }
            let dst = canvas.get_pixel_mut(px, py);
            let a = s[3] as f32 / 255.0;
            for i in 0..3 {
                dst[i] = (s[i] as f32 * a + dst[i] as f32 * (1.0 - a)).round() as u8;
            }
            dst[3] = 255;
        }
    }
}

/// 背景 + 実スクショ → 製品カット (PNG)。スクショの画素は等比縮小以外いじらない。
pub fn composite_product_cut(
    background: &[u8],
    screenshot: &[u8],
    layout: &Layout,
) -> Result<Vec<u8>, String> {
    let bg = load(background, "背景")?
        .resize_to_fill(layout.width, layout.height, FilterType::Lanczos3)
        .to_rgba8();
    let shot = load(screenshot, "スクリーンショット")?;
    let (sw, sh) = shot.dimensions();
    let max_w = (layout.width as f32 * layout.screen_ratio) as u32;
    let max_h = (layout.height as f32 * layout.screen_ratio) as u32;
    let scale = (max_w as f32 / sw as f32)
        .min(max_h as f32 / sh as f32)
        .min(1.0);
    let tw = ((sw as f32 * scale) as u32).max(1);
    let th = ((sh as f32 * scale) as u32).max(1);
    let mut shot = shot.resize_exact(tw, th, FilterType::Lanczos3).to_rgba8();
    round_corners(&mut shot, layout.corner_radius);
    let x = ((layout.width - tw) / 2) as i64;
    let y =
        ((layout.height - th) / 2) as i64 + (layout.height as f32 * layout.y_offset_ratio) as i64;

    let mut canvas = bg;
    let shadow_plate = |corner: u32, pad: u32| {
        let mut sh = RgbaImage::from_pixel(tw + pad * 2, th + pad * 2, Rgba([0, 0, 0, 0]));
        for yy in 0..th {
            for xx in 0..tw {
                sh.put_pixel(xx + pad, yy + pad, Rgba([0, 0, 0, 150]));
            }
        }
        round_corners(&mut sh, corner);
        imageops::blur(&sh, 10.0)
    };

    match layout.tilt {
        // 傾き無し = 従来の経路。画素等価を PoC で固定している。
        None => {
            if layout.shadow {
                let pad = 24u32;
                let shadow = shadow_plate(layout.corner_radius + pad / 2, pad);
                imageops::overlay(&mut canvas, &shadow, x - pad as i64, y - pad as i64 + 14);
            }
            imageops::overlay(&mut canvas, &shot, x, y);
        }
        // 傾きあり = 面と影を同じ quad で射影変換して焼く (rev5)。
        Some(tilt) => {
            let quad = project_corners(tw as f32, th as f32, &tilt, max_w as f32, max_h as f32);
            let cx = layout.width as f32 / 2.0;
            let cy = layout.height as f32 / 2.0 + layout.height as f32 * layout.y_offset_ratio;
            if layout.shadow {
                let pad = 24u32;
                let shadow = shadow_plate(layout.corner_radius + pad / 2, pad);
                // 影の quad は面より pad ぶん大きい。同じ傾きで作り直す。
                let sq = project_corners(
                    (tw + pad * 2) as f32,
                    (th + pad * 2) as f32,
                    &tilt,
                    (max_w + pad * 2) as f32,
                    (max_h + pad * 2) as f32,
                );
                warp_onto(&mut canvas, &shadow, &sq, (cx, cy + 14.0));
            }
            warp_onto(&mut canvas, &shot, &quad, (cx, cy));
        }
    }
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
        assert_eq!(
            [c[0], c[1], c[2]],
            [255, 0, 200],
            "中央はスクショの画素そのまま"
        );
        // スクショは幅 78% = 1048 に収まり、canvas の左端は背景のまま。
        let e = out.get_pixel(20, 384);
        assert_eq!([e[0], e[1], e[2]], [40, 40, 40]);
    }

    // --- rev5: 面の傾き。Phase E で背景のパースと貼った面のパースが噛み合わなかった ---

    /// 面の縦の伸び (背景色でない画素の高さ) を列 x で測る。
    fn plate_height_at(img: &RgbaImage, x: u32, bg: [u8; 3]) -> u32 {
        (0..img.height())
            .filter(|&y| {
                let p = img.get_pixel(x, y);
                [p[0], p[1], p[2]] != bg
            })
            .count() as u32
    }

    #[test]
    fn zero_tilt_takes_the_untilted_path_and_stays_pixel_identical() {
        let bg = png(400, 300, [40, 40, 40]);
        let shot = png(1920, 1080, [255, 0, 200]);
        let plain = Layout::for_canvas(1344, 768);
        let zero = Layout::for_canvas(1344, 768).with_tilt(Tilt {
            yaw_degrees: 0.0,
            pitch_degrees: 0.0,
        });
        assert_eq!(
            composite_product_cut(&bg, &shot, &plain).unwrap(),
            composite_product_cut(&bg, &shot, &zero).unwrap(),
            "傾き 0 は従来の経路と画素等価"
        );
    }

    #[test]
    fn positive_yaw_pushes_the_right_edge_away_and_keeps_the_pixels() {
        let bg = png(400, 300, [40, 40, 40]);
        let shot = png(1920, 1080, [255, 0, 200]);
        let layout = Layout::for_canvas(1344, 768).with_tilt(Tilt {
            yaw_degrees: 22.0,
            pitch_degrees: 0.0,
        });
        let out = decode(&composite_product_cut(&bg, &shot, &layout).unwrap());
        assert_eq!(out.dimensions(), (1344, 768));
        // 面が乗っている列を左右から探し、遠い側 (右) が縮んでいることを見る。
        let cols: Vec<u32> = (0..1344)
            .filter(|&x| plate_height_at(&out, x, [40, 40, 40]) > 10)
            .collect();
        let (left, right) = (*cols.first().unwrap(), *cols.last().unwrap());
        let hl = plate_height_at(&out, left + 8, [40, 40, 40]);
        let hr = plate_height_at(&out, right - 8, [40, 40, 40]);
        assert!(hl > hr, "yaw 正 = 右が奥に倒れる (左 {hl} / 右 {hr})");
        // 画素そのものは残る (色を作り変えていない)。
        let mid = out.get_pixel((left + right) / 2, 384);
        assert_eq!(
            [mid[0], mid[1], mid[2]],
            [255, 0, 200],
            "中央はスクショの色のまま"
        );
    }

    #[test]
    fn negative_pitch_looks_down_at_the_plate_and_shrinks_the_top() {
        let bg = png(400, 300, [40, 40, 40]);
        let shot = png(1600, 1000, [0, 200, 255]);
        let layout = Layout::for_canvas(1344, 768).with_tilt(Tilt {
            yaw_degrees: 0.0,
            pitch_degrees: -20.0,
        });
        let out = decode(&composite_product_cut(&bg, &shot, &layout).unwrap());
        let width_at = |y: u32| {
            (0..out.width())
                .filter(|&x| {
                    let p = out.get_pixel(x, y);
                    [p[0], p[1], p[2]] != [40, 40, 40]
                })
                .count() as u32
        };
        let rows: Vec<u32> = (0..768).filter(|&y| width_at(y) > 10).collect();
        let (top, bottom) = (*rows.first().unwrap(), *rows.last().unwrap());
        assert!(
            width_at(top + 8) < width_at(bottom - 8),
            "pitch 負 = 上辺が奥 (俯瞰)"
        );
    }

    #[test]
    fn corners_are_rounded_and_portrait_screenshot_fits_by_height() {
        let bg = png(100, 100, [10, 20, 30]);
        let shot = png(600, 1200, [0, 255, 0]); // 縦長
        let layout = Layout {
            width: 800,
            height: 600,
            screen_ratio: 0.8,
            corner_radius: 40,
            shadow: false,
            y_offset_ratio: 0.0,
            tilt: None,
        };
        let out = decode(&composite_product_cut(&bg, &shot, &layout).unwrap());
        // 高さ 480 に収まる → 幅 240。左上角 (角丸の外) は背景色。
        let x0 = (800 - 240) / 2;
        let y0 = (600 - 480) / 2;
        let corner = out.get_pixel(x0, y0);
        assert_eq!(
            [corner[0], corner[1], corner[2]],
            [10, 20, 30],
            "角丸で角は背景"
        );
        let inner = out.get_pixel(x0 + 120, y0 + 240);
        assert_eq!([inner[0], inner[1], inner[2]], [0, 255, 0]);
    }

    #[test]
    fn small_screenshot_is_not_upscaled() {
        let bg = png(100, 100, [0, 0, 0]);
        let shot = png(200, 100, [255, 255, 255]);
        let layout = Layout {
            width: 1000,
            height: 500,
            screen_ratio: 0.9,
            corner_radius: 0,
            shadow: false,
            y_offset_ratio: 0.0,
            tilt: None,
        };
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
        let red_in_band = (676..768)
            .flat_map(|y| (0..1344).map(move |x| (x, y)))
            .filter(|&(x, y)| out.get_pixel(x, y)[0] > 150)
            .count();
        assert_eq!(red_in_band, 0, "下の帯にスクショが掛かっている");
        // 中央付近は赤 (スクショは残っている)。
        assert!(out.get_pixel(672, 330)[0] > 150);
        let top = Layout::for_canvas(1344, 768).with_caption_band(true);
        let out2 = decode(&composite_product_cut(&bg, &shot, &top).unwrap());
        let red_top = (0..92)
            .flat_map(|y| (0..1344).map(move |x| (x, y)))
            .filter(|&(x, y)| out2.get_pixel(x, y)[0] > 150)
            .count();
        assert_eq!(red_top, 0);
    }

    #[test]
    fn garbage_input_is_an_error_not_a_panic() {
        assert!(
            composite_product_cut(
                b"nope",
                &png(10, 10, [0, 0, 0]),
                &Layout::for_canvas(100, 100)
            )
            .is_err()
        );
        assert!(
            composite_product_cut(
                &png(10, 10, [0, 0, 0]),
                b"nope",
                &Layout::for_canvas(100, 100)
            )
            .is_err()
        );
    }
}
