//! Sun shadow cascades: where each shadow map looks, computed on the CPU every frame so the
//! sun can move.
//!
//! Three cascades follow the camera (bounding spheres of view-frustum slices, snapped to
//! whole texels so they do not shimmer as the camera moves) and the last one covers the
//! whole scene's bounds, which is what lets a 600 m shaft hold its shadows at any range.
//! A remote sun gives orthographic maps, a near sun perspective ones from its position.
//! See "Shadow pass" in docs/features/ink-renderer.md.

use crate::Camera;
use glam::{Mat4, Vec3};

/// Shadow map resolution (each cascade is one layer of a square array texture).
pub const SHADOW_SIZE: u32 = 2048;
/// Number of cascades: the camera-fitted ones plus one fitted to the scene bounds.
pub const SHADOW_CASCADES: usize = 4;
/// Far ends, in metres of view distance, of the camera-fitted cascades.
pub const CAMERA_SPLITS: [f32; SHADOW_CASCADES - 1] = [8.0, 30.0, 110.0];
/// Far end of the last cascade when there are no scene bounds to fit it to.
const FALLBACK_FAR: f32 = 1000.0;
/// Largest half-angle of a near sun's perspective frustum (when the sun is inside or very
/// close to the region a cascade covers).
const MAX_HALF_ANGLE: f32 = 1.2;

/// Where the cascades look this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cascades {
    /// World to shadow clip space, per cascade (depth 0..1, near to far).
    pub view_proj: [Mat4; SHADOW_CASCADES],
    /// Orthographic: world size of one texel. Perspective: texel size per metre of
    /// distance from the sun.
    pub texel: [f32; SHADOW_CASCADES],
    pub perspective: bool,
}

/// Bounding sphere (centre distance along the view axis, radius) of the frustum slice
/// between view distances `n` and `f`, for a frustum whose corner rays leave the axis at
/// `k = sqrt(tan_x^2 + tan_y^2)` per unit distance. It depends only on n, f and the field
/// of view, so it does not change as the camera turns.
fn slice_sphere(n: f32, f: f32, k: f32) -> (f32, f32) {
    let k2 = k * k;
    let x = ((f + n) * 0.5 * (1.0 + k2)).min(f);
    let r2 = ((f - x) * (f - x) + f * f * k2).max((x - n) * (x - n) + n * n * k2);
    (x, r2.sqrt())
}

fn up_for(dir: Vec3) -> Vec3 {
    if dir.y.abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    }
}

fn corners(lo: Vec3, hi: Vec3) -> [Vec3; 8] {
    let mut c = [Vec3::ZERO; 8];
    for (i, p) in c.iter_mut().enumerate() {
        *p = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
    }
    c
}

/// Fit the cascades for one frame. `sun` is the sun's world position, or with
/// `directional` the unit direction towards it. `bounds` is the scene's axis-aligned box.
pub fn fit(camera: &Camera, aspect: f32, sun: Vec3, directional: bool, bounds: Option<(Vec3, Vec3)>) -> Cascades {
    let ty = (camera.fov_y * 0.5).tan();
    let k = (ty * ty * (1.0 + aspect * aspect)).sqrt();
    let fwd = camera.forward();
    let mut spheres = [(Vec3::ZERO, 0.0f32); SHADOW_CASCADES];
    let mut n = crate::camera::NEAR;
    for (i, &f) in CAMERA_SPLITS.iter().enumerate() {
        let (x, r) = slice_sphere(n, f, k);
        spheres[i] = (camera.position + fwd * x, r);
        n = f;
    }
    let (x, r) = slice_sphere(n, FALLBACK_FAR, k);
    spheres[SHADOW_CASCADES - 1] = (camera.position + fwd * x, r);

    let mut out = Cascades { view_proj: [Mat4::IDENTITY; SHADOW_CASCADES], texel: [1.0; SHADOW_CASCADES], perspective: !directional };
    let size = SHADOW_SIZE as f32;
    if directional {
        let rot = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, -sun, up_for(sun));
        // Depth range: everything in the scene can cast, so take the bounds along the light.
        let (zlo, zhi, scene_rect) = match bounds {
            Some((lo, hi)) => {
                let (mut zlo, mut zhi) = (f32::MAX, f32::MIN);
                let (mut rlo, mut rhi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for c in corners(lo, hi) {
                    let l = rot.transform_point3(c);
                    zlo = zlo.min(l.z);
                    zhi = zhi.max(l.z);
                    rlo = rlo.min(l);
                    rhi = rhi.max(l);
                }
                (zlo, zhi, Some((rlo, rhi)))
            }
            None => (f32::MIN, f32::MAX, None),
        };
        for (i, &(c, r)) in spheres.iter().enumerate() {
            let last = i == SHADOW_CASCADES - 1;
            let (x0, x1, y0, y1, texel) = match (last, scene_rect) {
                (true, Some((rlo, rhi))) => {
                    // A one-texel margin so casters on the boundary are not clipped.
                    let t = (rhi.x - rlo.x).max(rhi.y - rlo.y) / (size - 2.0);
                    (rlo.x - t, rhi.x + t, rlo.y - t, rhi.y + t, t)
                }
                _ => {
                    let t = 2.0 * r / size;
                    let l = rot.transform_point3(c);
                    // Snap to whole texels so the map does not crawl as the camera moves.
                    let (sx, sy) = ((l.x / t).round() * t, (l.y / t).round() * t);
                    (sx - r, sx + r, sy - r, sy + r, t)
                }
            };
            let (zlo, zhi) = if bounds.is_some() {
                (zlo, zhi)
            } else {
                let l = rot.transform_point3(c);
                (l.z - 4.0 * r, l.z + 4.0 * r)
            };
            // Light view space looks down -z: near is -zhi, far is -zlo.
            let margin = 1.0 + 0.01 * (zhi - zlo);
            let proj = glam::camera::rh::proj::directx::orthographic(x0, x1, y0, y1, -zhi - margin, -zlo + margin);
            out.view_proj[i] = proj * rot;
            out.texel[i] = texel;
        }
    } else {
        let mut spheres = spheres;
        if let Some((lo, hi)) = bounds {
            spheres[SHADOW_CASCADES - 1] = ((lo + hi) * 0.5, (hi - lo).length() * 0.5);
        }
        // Casters lie between the sun and the receivers: the near plane is the distance
        // from the sun to the scene (or a little in front of it when the sun is inside).
        let near_scene = bounds.map(|(lo, hi)| (sun.clamp(lo, hi) - sun).length()).unwrap_or(0.0);
        for (i, &(c, r)) in spheres.iter().enumerate() {
            let v = c - sun;
            let dist = v.length().max(1e-3);
            let half = if dist > r { (r / dist).asin().min(MAX_HALF_ANGLE) } else { MAX_HALF_ANGLE };
            let near = near_scene.max(0.002 * dist).max(0.1).min((dist - r).max(0.1));
            let far = dist + r;
            let view = glam::camera::rh::view::look_at_mat4(sun, c, up_for(v / dist));
            let proj = glam::camera::rh::proj::directx::perspective(2.0 * half, 1.0, near, far);
            out.view_proj[i] = proj * view;
            out.texel[i] = 2.0 * half.tan() / size;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam() -> Camera {
        Camera::new(Vec3::new(3.0, 400.0, -5.0), 0.7, -0.4)
    }

    fn inside(m: Mat4, p: Vec3) -> bool {
        let c = m * p.extend(1.0);
        let n = c.truncate() / c.w;
        n.x.abs() <= 1.0 && n.y.abs() <= 1.0 && (0.0..=1.0).contains(&n.z)
    }

    #[test]
    fn slice_sphere_contains_its_corners() {
        let k = 0.9;
        for (n, f) in [(0.1, 8.0), (8.0, 30.0), (30.0, 110.0), (110.0, 1000.0)] {
            let (x, r) = slice_sphere(n, f, k);
            for (a, rad) in [(n, n * k), (f, f * k)] {
                assert!(((a - x) * (a - x) + rad * rad).sqrt() <= r * 1.0001);
            }
        }
    }

    #[test]
    fn remote_cascades_cover_the_view_and_the_scene() {
        let c = cam();
        let bounds = (Vec3::new(-38.0, 0.0, -140.0), Vec3::new(38.0, 600.0, 38.0));
        let dir = Vec3::new(-0.3, 0.9, -0.1).normalize();
        let k = fit(&c, 16.0 / 9.0, dir, true, Some(bounds));
        // A point 5 m ahead is in cascade 0, one 20 m ahead in cascade 1.
        assert!(inside(k.view_proj[0], c.position + c.forward() * 5.0));
        assert!(inside(k.view_proj[1], c.position + c.forward() * 20.0));
        // Every scene corner is in the last cascade.
        for p in corners(bounds.0, bounds.1) {
            assert!(inside(k.view_proj[3], p), "{p}");
        }
        assert!(k.texel[0] < 0.015 && k.texel[3] < 0.2, "{:?}", k.texel);
    }

    #[test]
    fn camera_cascades_are_texel_snapped() {
        // Moving the camera by a fraction of a texel moves the map by whole texels only.
        let dir = Vec3::new(-0.3, 0.9, -0.1).normalize();
        let a = fit(&cam(), 1.5, dir, true, None);
        let mut c2 = cam();
        c2.position += Vec3::new(0.0013, 0.0, 0.0007);
        let b = fit(&c2, 1.5, dir, true, None);
        let p = Vec3::new(1.0, 2.0, 3.0);
        let (pa, pb) = (a.view_proj[0] * p.extend(1.0), b.view_proj[0] * p.extend(1.0));
        let texels = (pa.x - pb.x) * 0.5 * SHADOW_SIZE as f32;
        assert!((texels - texels.round()).abs() < 1e-2, "{texels}");
    }

    #[test]
    fn near_sun_frusta_contain_their_spheres() {
        let c = cam();
        let bounds = (Vec3::new(-38.0, 0.0, -140.0), Vec3::new(38.0, 600.0, 38.0));
        let sun = Vec3::new(-200.0, 1500.0, 50.0);
        let k = fit(&c, 1.5, sun, false, Some(bounds));
        assert!(k.perspective);
        assert!(inside(k.view_proj[0], c.position + c.forward() * 4.0));
        for p in corners(bounds.0, bounds.1) {
            assert!(inside(k.view_proj[3], p), "{p}");
        }
    }
}
