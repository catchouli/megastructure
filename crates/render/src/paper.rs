//! Paper relief textures for the surface-anchored paper: one generated (E044) and scanned CC0
//! sheets from ambientCG (E049, `assets/paper/README.md`), all prepared at startup into
//! square, seamless, zero-mean, unit-variance tiles of `PAPER_TILE` texels with mips.
//!
//! The generated tile, below:
//!
//! Short fibres at random orientations, some lighter and some darker than the sheet, laid
//! partly in loose clumps; fine pits; no low-frequency cloud. The forward pass samples it in
//! the surface frame at several fixed world scales (see "Paper and halftone" in
//! docs/features/ink-renderer.md). Values are stored as `0.5 + 0.25 h` with `h` normalised to
//! zero mean and unit standard deviation, with a full mip chain so every layer is sampled
//! band-limited.

/// Side of the tile in texels.
pub const PAPER_TILE: usize = 512;

/// Deterministic generator (the tile is the same on every run).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32) / (1u64 << 24) as f32
    }
}

/// The relief, `PAPER_TILE^2` values, zero mean and unit standard deviation.
pub fn paper_relief() -> Vec<f32> {
    let n = PAPER_TILE;
    let mut h = vec![0.0f32; n * n];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    // Splat a soft disc of radius r (texels) at (x, y), wrapping, amount a at the centre.
    let splat = |h: &mut Vec<f32>, x: f32, y: f32, r: f32, a: f32| {
        let ri = r.ceil() as i32 + 1;
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                let px = x.floor() as i32 + dx;
                let py = y.floor() as i32 + dy;
                let fx = px as f32 + 0.5 - x;
                let fy = py as f32 + 0.5 - y;
                let d = (fx * fx + fy * fy).sqrt();
                let w = (1.0 - (d - r + 0.5).clamp(0.0, 1.0)).max(0.0);
                if w > 0.0 {
                    let ix = px.rem_euclid(n as i32) as usize;
                    let iy = py.rem_euclid(n as i32) as usize;
                    h[iy * n + ix] += a * w;
                }
            }
        }
    };
    // Clump centres: a third of the fibres gather around these.
    let clumps: Vec<(f32, f32)> = (0..40).map(|_| (rng.next() * n as f32, rng.next() * n as f32)).collect();
    for i in 0..5200 {
        let (cx, cy) = if i % 3 == 0 {
            let c = clumps[(rng.next() * clumps.len() as f32) as usize % clumps.len()];
            let a = rng.next() * std::f32::consts::TAU;
            let r = 18.0 * rng.next().sqrt();
            (c.0 + r * a.cos(), c.1 + r * a.sin())
        } else {
            (rng.next() * n as f32, rng.next() * n as f32)
        };
        let ang = rng.next() * std::f32::consts::PI;
        // Fibres 5 to 22 texels long, slightly curved, mostly fine.
        let len = 5.0 + 17.0 * rng.next() * rng.next();
        let width = 0.45 + 0.5 * rng.next() * rng.next();
        let curve = (rng.next() - 0.5) * 0.08;
        let amp = if rng.next() < 0.55 { 0.9 } else { -0.7 } * (0.5 + rng.next());
        let steps = (len * 2.0) as i32;
        let (mut x, mut y, mut a) = (cx - 0.5 * len * ang.cos(), cy - 0.5 * len * ang.sin(), ang);
        for _ in 0..steps {
            splat(&mut h, x, y, width, amp * 0.5);
            x += 0.5 * a.cos();
            y += 0.5 * a.sin();
            a += curve;
        }
    }
    // Fine pits.
    for _ in 0..9000 {
        let (x, y) = (rng.next() * n as f32, rng.next() * n as f32);
        splat(&mut h, x, y, 0.4 + 0.6 * rng.next(), -0.8 * (0.4 + rng.next()));
    }
    // A very faint tooth so the sheet between fibres is not dead flat: white noise blurred
    // over about a texel.
    let mut t: Vec<f32> = (0..n * n).map(|_| rng.next() - 0.5).collect();
    for _ in 0..2 {
        let src = t.clone();
        for y in 0..n {
            for x in 0..n {
                let mut s = 0.0;
                for (dx, dy) in [(0, 0), (1, 0), (n - 1, 0), (0, 1), (0, n - 1)] {
                    s += src[((y + dy) % n) * n + (x + dx) % n];
                }
                t[y * n + x] = s / 5.0;
            }
        }
    }
    for (v, w) in h.iter_mut().zip(&t) {
        *v += 0.6 * w;
    }
    let mean = h.iter().sum::<f32>() / h.len() as f32;
    let var = h.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / h.len() as f32;
    let sd = var.sqrt().max(1e-6);
    h.iter_mut().for_each(|v| *v = (*v - mean) / sd);
    h
}

/// Paper kinds, in the order of the texture array layers and of `StylePreset::paper_kind`.
pub const PAPER_KINDS: &[&str] = &["fibre", "Paper001", "Paper002", "Paper004", "Paper006"];

/// The scanned displacement maps, embedded so the binaries are self-contained.
const SCANS: [&[u8]; 4] = [
    include_bytes!("../../../assets/paper/Paper001/Paper001_1K-JPG_Displacement.jpg"),
    include_bytes!("../../../assets/paper/Paper002/Paper002_1K-JPG_Displacement.jpg"),
    include_bytes!("../../../assets/paper/Paper004/Paper004_1K-JPG_Displacement.jpg"),
    include_bytes!("../../../assets/paper/Paper006/Paper006_1K-JPG_Displacement.jpg"),
];

/// The relief of paper kind `k` (index into `PAPER_KINDS`).
pub fn relief_of(k: usize) -> Vec<f32> {
    if k == 0 {
        paper_relief()
    } else {
        scanned_relief_folded(SCANS[k - 1], PAPER_FOLD[k])
    }
}

/// Width over height of each scan, for the shader to undo the resampling to a square tile.
pub const PAPER_ASPECT: [f32; 5] = [1.0, 1024.0 / 600.0, 794.0 / 1024.0, 1.0, 1.0];

/// A scanned sheet's displacement as a tile (E049):
/// 1. luminance of the whole map, area-resampled to `PAPER_TILE` square (the shader stretches
///    it back by `PAPER_ASPECT`); ambientCG's maps already tile (measured in E049: the
///    wrap-around difference equals the difference between neighbouring rows), so no seam
///    processing is applied and resampling the whole map keeps the tiling;
/// 2. high-pass: minus a wrapping Gaussian blur of 8 texels, so no sheet-wide gradient,
///    creases or cloud survive, only tooth and fibre;
/// 3. normalised to zero mean and unit standard deviation, clamped at +-3 (renormalised).
pub fn scanned_relief(jpg: &[u8]) -> Vec<f32> {
    scanned_relief_folded(jpg, 1)
}

/// How many times each kind's scan is repeated across the tile (E051): the scan is resampled
/// to `PAPER_TILE / fold` and tiled `fold` x `fold` (it tiles natively), so its features are
/// `fold` times fewer texels wide. Paper001's tooth is about 6 to 10 texels across at fold 1,
/// which the 1 to 4 px-per-texel window magnified into 10 to 30 px bumps up close (the
/// owner's "harsh" frame 124923); at fold 4 a bump is about 2 texels.
pub const PAPER_FOLD: [usize; 5] = [1, 2, 1, 2, 1];

/// `scanned_relief` with the scan repeated `fold` x `fold` times across the tile.
pub fn scanned_relief_folded(jpg: &[u8], fold: usize) -> Vec<f32> {
    let full = PAPER_TILE;
    let n = PAPER_TILE / fold.max(1);
    let img = image::load_from_memory(jpg).expect("embedded paper scan decodes").to_luma32f();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let (sx, sy) = (w as f32 / n as f32, h as f32 / n as f32);
    let mut t = vec![0.0f32; n * n];
    for y in 0..n {
        for x in 0..n {
            let (x0, y0) = ((x as f32 * sx) as usize, (y as f32 * sy) as usize);
            let (x1, y1) = ((((x + 1) as f32 * sx) as usize).max(x0 + 1).min(w), (((y + 1) as f32 * sy) as usize).max(y0 + 1).min(h));
            let mut acc = 0.0f32;
            let mut cnt = 0.0f32;
            for yy in y0..y1 {
                for xx in x0..x1 {
                    acc += img.get_pixel(xx as u32, yy as u32).0[0];
                    cnt += 1.0;
                }
            }
            t[y * n + x] = acc / cnt.max(1.0);
        }
    }
    let blur = gaussian_wrap(&t, n, (8.0 / fold as f32).max(2.0));
    let mut out: Vec<f32> = t.iter().zip(&blur).map(|(a, b)| a - b).collect();
    for _ in 0..2 {
        let mean = out.iter().sum::<f32>() / out.len() as f32;
        let sd = (out.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / out.len() as f32).sqrt().max(1e-6);
        out.iter_mut().for_each(|v| *v = ((*v - mean) / sd).clamp(-3.0, 3.0));
    }
    let mean = out.iter().sum::<f32>() / out.len() as f32;
    let sd = (out.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / out.len() as f32).sqrt().max(1e-6);
    out.iter_mut().for_each(|v| *v = (*v - mean) / sd);
    if n == full {
        return out;
    }
    let mut tiled = vec![0.0f32; full * full];
    for y in 0..full {
        for x in 0..full {
            tiled[y * full + x] = out[(y % n) * n + x % n];
        }
    }
    tiled
}

fn gaussian_wrap(src: &[f32], n: usize, sigma: f32) -> Vec<f32> {
    let r = (3.0 * sigma).ceil() as i32;
    let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let ks: f32 = k.iter().sum();
    let mut tmp = vec![0.0f32; n * n];
    for y in 0..n {
        for x in 0..n {
            let mut a = 0.0;
            for (j, kv) in k.iter().enumerate() {
                let xx = (x as i32 + j as i32 - r).rem_euclid(n as i32) as usize;
                a += kv * src[y * n + xx];
            }
            tmp[y * n + x] = a / ks;
        }
    }
    let mut out = vec![0.0f32; n * n];
    for y in 0..n {
        for x in 0..n {
            let mut a = 0.0;
            for (j, kv) in k.iter().enumerate() {
                let yy = (y as i32 + j as i32 - r).rem_euclid(n as i32) as usize;
                a += kv * tmp[yy * n + x];
            }
            out[y * n + x] = a / ks;
        }
    }
    out
}

/// Mip levels of paper kind `k` as R8, largest first: `0.5 + 0.25 h`, clamped.
pub fn kind_mips(k: usize) -> Vec<Vec<u8>> {
    mips_of(relief_of(k))
}

/// The generated tile as R8 mip levels, largest first: `0.5 + 0.25 h`, clamped.
pub fn paper_mips() -> Vec<Vec<u8>> {
    mips_of(paper_relief())
}

fn mips_of(relief: Vec<f32>) -> Vec<Vec<u8>> {
    let mut level: Vec<f32> = relief;
    let mut size = PAPER_TILE;
    let mut out = Vec::new();
    loop {
        out.push(level.iter().map(|v| ((0.5 + 0.25 * v).clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect());
        if size == 1 {
            break;
        }
        let half = size / 2;
        let mut next = vec![0.0f32; half * half];
        for y in 0..half {
            for x in 0..half {
                next[y * half + x] = 0.25 * (level[2 * y * size + 2 * x] + level[2 * y * size + 2 * x + 1] + level[(2 * y + 1) * size + 2 * x] + level[(2 * y + 1) * size + 2 * x + 1]);
            }
        }
        level = next;
        size = half;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relief_is_normalised_and_tiles() {
        let h = paper_relief();
        let mean = h.iter().sum::<f32>() / h.len() as f32;
        let sd = (h.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / h.len() as f32).sqrt();
        assert!(mean.abs() < 1e-3 && (sd - 1.0).abs() < 1e-3, "{mean} {sd}");
        // Seamless: the step across the wrap is no larger than steps inside the tile.
        let n = PAPER_TILE;
        let inner: f32 = (0..n).map(|y| (h[y * n + 1] - h[y * n]).abs()).sum::<f32>() / n as f32;
        let wrap: f32 = (0..n).map(|y| (h[y * n] - h[y * n + n - 1]).abs()).sum::<f32>() / n as f32;
        assert!(wrap < 1.5 * inner, "wrap {wrap} inner {inner}");
        assert_eq!(paper_mips().len(), 10);
    }

    #[test]
    fn scans_are_normalised_and_seamless() {
        let n = PAPER_TILE;
        for k in 1..PAPER_KINDS.len() {
            let h = relief_of(k);
            assert_eq!(h.len(), n * n);
            let mean = h.iter().sum::<f32>() / h.len() as f32;
            let sd = (h.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / h.len() as f32).sqrt();
            assert!(mean.abs() < 1e-3 && (sd - 1.0).abs() < 1e-3, "{}: {mean} {sd}", PAPER_KINDS[k]);
            let inner: f32 = (0..n).map(|y| (h[y * n + 1] - h[y * n]).abs()).sum::<f32>() / n as f32;
            let wrap: f32 = (0..n).map(|y| (h[y * n] - h[y * n + n - 1]).abs()).sum::<f32>() / n as f32;
            assert!(wrap < 1.3 * inner, "{}: wrap {wrap} inner {inner}", PAPER_KINDS[k]);
        }
    }
}
