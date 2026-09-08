//! palette の画素算出 (契約 `VisualIdentity.palette`、rev2 査読 11 + 決定 8)。
//!
//! LLM に色を推定させない — 色は画素にある。縮小 → 4 bit/ch 量子化 → ヒストグラム → 出現順に、
//! 既に選んだ色と近すぎるもの (RGB 距離) を飛ばして N 色。純粋関数 [`dominant_colors`] は画素列だけを見る。

use std::collections::HashMap;
use std::path::Path;

/// 既定の色数 (契約: 3〜5)。
pub const DEFAULT_COLORS: usize = 5;
/// 縮小後の長辺 (画素数を抑える。色の分布だけ要る)。
const THUMB: u32 = 96;
/// 「同じ色」とみなす RGB ユークリッド距離 (0..=441)。
const MIN_DISTANCE: f64 = 60.0;

fn quantize(c: u8) -> u8 {
    // 4 bit/ch。代表値はビンの中央。
    (c & 0xF0) | 0x08
}

fn dist(a: [u8; 3], b: [u8; 3]) -> f64 {
    let d = |x: u8, y: u8| (x as f64 - y as f64).powi(2);
    (d(a[0], b[0]) + d(a[1], b[1]) + d(a[2], b[2])).sqrt()
}

/// 画素列から支配色を N 個 (hex、出現頻度順)。空なら空。
pub fn dominant_colors(pixels: &[[u8; 3]], n: usize) -> Vec<String> {
    let mut hist: HashMap<[u8; 3], usize> = HashMap::new();
    for p in pixels {
        *hist.entry([quantize(p[0]), quantize(p[1]), quantize(p[2])]).or_insert(0) += 1;
    }
    let mut bins: Vec<([u8; 3], usize)> = hist.into_iter().collect();
    // 頻度降順、同数は色値で決定論。
    bins.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut chosen: Vec<[u8; 3]> = Vec::new();
    for (c, _) in bins {
        if chosen.len() == n {
            break;
        }
        if chosen.iter().all(|k| dist(*k, c) >= MIN_DISTANCE) {
            chosen.push(c);
        }
    }
    chosen.iter().map(|c| format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])).collect()
}

/// 画像ファイルから (IO)。読めない形式・壊れたファイルは Err。
pub fn palette_from_file(path: &Path, n: usize) -> Result<Vec<String>, String> {
    let img = image::open(path).map_err(|e| format!("画像を読めません {}: {e}", path.display()))?;
    // Nearest: 補間で境界に混色を作らない (thumbnail = triangle は 2 色画像から第 3 の色を生んだ)。
    let thumb = img.resize(THUMB, THUMB, image::imageops::FilterType::Nearest).to_rgb8();
    let pixels: Vec<[u8; 3]> = thumb.pixels().map(|p| [p[0], p[1], p[2]]).collect();
    Ok(dominant_colors(&pixels, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_frequent_distinct_colors_in_order() {
        let mut px = Vec::new();
        px.extend(std::iter::repeat_n([20, 17, 14], 500)); // ink
        px.extend(std::iter::repeat_n([232, 221, 200], 300)); // parchment
        px.extend(std::iter::repeat_n([217, 138, 74], 100)); // ember
        px.extend(std::iter::repeat_n([22, 18, 15], 50)); // ink とほぼ同じ → 統合される
        let out = dominant_colors(&px, 5);
        // 4 bit 量子化の代表値: 20→0x18, 17→0x18, 14→0x08。
        assert_eq!(out, vec!["#181808", "#E8D8C8", "#D88848"]);
    }

    #[test]
    fn respects_n_and_handles_empty() {
        let px = [[0, 0, 0], [255, 255, 255], [255, 0, 0], [0, 255, 0], [0, 0, 255]];
        assert_eq!(dominant_colors(&px, 2).len(), 2);
        assert!(dominant_colors(&[], 5).is_empty());
    }

    #[test]
    fn file_roundtrip_through_png() {
        let dir = std::env::temp_dir().join(format!("palette_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("two_tone.png");
        let mut img = image::RgbImage::new(40, 20);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            *px = if x < 30 { image::Rgb([10, 20, 30]) } else { image::Rgb([240, 200, 100]) };
        }
        img.save(&p).unwrap();
        let out = palette_from_file(&p, 5).unwrap();
        // 10→0x08, 20→0x18, 30→0x18 / 240→0xF8, 200→0xC8, 100→0x68。混色が第 3 の色として出ないこと。
        assert_eq!(out, vec!["#081818", "#F8C868"]);
        assert!(palette_from_file(&dir.join("missing.png"), 5).is_err());
    }
}
