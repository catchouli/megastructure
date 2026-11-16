//! A tileable paper relief texture, generated once at startup (E044).
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

/// The tile as R8 mip levels, largest first: `0.5 + 0.25 h`, clamped.
pub fn paper_mips() -> Vec<Vec<u8>> {
    let mut level: Vec<f32> = paper_relief();
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
}
