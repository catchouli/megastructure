//! Hierarchical seeding (`docs/PLAN.md`, "Seeding"): `seed = hash64(parent_seed, stable_id,
//! layer_tag)`. Stable ids are paths such as `"core/a1/j"`, never counters that depend on
//! generation order, so adding a space somewhere does not reshuffle the rest.

/// Layer tags, mixed into every derived seed so two layers asking about the same id do not
/// get the same numbers.
pub mod layer {
    pub const DIRECTOR: u64 = 0x4449_5245;
    pub const GRAPH: u64 = 0x4752_4150;
    pub const LAYOUT: u64 = 0x4c41_594f;
    pub const ELEMENTS: u64 = 0x454c_454d;
}

fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Derive a child seed from a parent seed, a stable id (a path) and a layer tag.
pub fn hash64(parent: u64, id: &str, layer: u64) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in id.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    mix(mix(parent ^ layer.rotate_left(17)) ^ h)
}

/// Hash of integers, for per-cell noise.
pub fn hash_ints(seed: u64, v: &[i64]) -> u64 {
    let mut h = mix(seed);
    for &x in v {
        h = mix(h ^ (x as u64));
    }
    h
}

/// A small deterministic generator (splitmix64).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(mix(seed))
    }

    /// The generator for one stable id in one layer.
    pub fn for_id(parent: u64, id: &str, layer: u64) -> Self {
        Rng::new(hash64(parent, id, layer))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    /// Uniform in 0..1.
    pub fn f(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }

    /// Uniform integer in `a..=b`.
    pub fn int(&mut self, a: i32, b: i32) -> i32 {
        if b <= a {
            return a;
        }
        a + (self.next_u64() % ((b - a + 1) as u64)) as i32
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }

    pub fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[(self.next_u64() % v.len() as u64) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_depends_on_every_part() {
        let a = hash64(1, "core/a1", layer::GRAPH);
        assert_eq!(a, hash64(1, "core/a1", layer::GRAPH));
        assert_ne!(a, hash64(2, "core/a1", layer::GRAPH));
        assert_ne!(a, hash64(1, "core/a2", layer::GRAPH));
        assert_ne!(a, hash64(1, "core/a1", layer::LAYOUT));
    }

    #[test]
    fn rng_ranges() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            let x = r.int(3, 5);
            assert!((3..=5).contains(&x));
            let f = r.f();
            assert!((0.0..1.0).contains(&f));
        }
    }
}
