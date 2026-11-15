use glam::{Mat4, Vec3};

/// Reversed-Z near plane distance; the far plane is at infinity.
pub const NEAR: f32 = 0.1;

/// First-person camera. Yaw rotates about +Y; yaw 0 looks along +Z (down the test corridor)
/// and positive yaw turns towards +X. Pitch is positive looking up. Angles in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// Vertical field of view, radians.
    pub fov_y: f32,
}

impl Camera {
    pub const DEFAULT_FOV_Y: f32 = 70.0 * std::f32::consts::PI / 180.0;

    pub fn new(position: Vec3, yaw: f32, pitch: f32) -> Self {
        Self { position, yaw, pitch, fov_y: Self::DEFAULT_FOV_Y }
    }

    /// Camera at `eye` looking at `target`.
    pub fn look_at(eye: Vec3, target: Vec3) -> Self {
        let d = (target - eye).normalize();
        Self::new(eye, d.x.atan2(d.z), d.y.asin())
    }

    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(sy * cp, sp, cy * cp)
    }

    /// Horizontal right vector (right-handed, so looking along +Z, right is -X).
    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize_or_zero()
    }

    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_to_mat4(self.position, self.forward(), Vec3::Y)
    }

    /// Reversed-Z, infinite far plane, wgpu/DirectX NDC (depth range 0..1, Y up) (near maps to 1, infinity to 0).
    pub fn projection(&self, aspect: f32) -> Mat4 {
        glam::camera::rh::proj::directx::perspective_infinite_reverse(self.fov_y, aspect, NEAR)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reversed_z_maps_near_to_one_and_far_to_zero() {
        let cam = Camera::new(Vec3::ZERO, 0.0, 0.0);
        let vp = cam.view_proj(1.0);
        let near = vp * Vec3::new(0.0, 0.0, NEAR).extend(1.0);
        assert!((near.z / near.w - 1.0).abs() < 1e-5);
        let far = vp * Vec3::new(0.0, 0.0, 1.0e7).extend(1.0);
        assert!(far.z / far.w < 1e-5);
    }

    #[test]
    fn look_at_round_trips() {
        let cam = Camera::look_at(Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 6.0, 20.0));
        let d = (Vec3::new(4.0, 6.0, 20.0) - cam.position).normalize();
        assert!((cam.forward() - d).length() < 1e-5);
    }
}
