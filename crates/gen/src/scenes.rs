//! Named scenes. Binaries select a scene by name so that new scenes need no changes outside
//! this crate.

use crate::MeshData;

/// A scene ready to upload, with a place to put the camera or player.
pub struct Scene {
    pub mesh: MeshData,
    /// Eye position to start at, in metres.
    pub spawn: [f32; 3],
    /// Initial yaw in radians; 0 looks along +Z.
    pub spawn_yaw: f32,
}

/// Names accepted by [`by_name`]. The first is the default.
pub const NAMES: &[&str] = &["test"];

pub const DEFAULT: &str = NAMES[0];

/// Build the named scene, or `None` if the name is unknown.
pub fn by_name(name: &str) -> Option<Scene> {
    match name {
        "test" => Some(Scene {
            mesh: crate::test_scene::build(),
            spawn: [0.0, 1.65, 2.0],
            spawn_yaw: 0.0,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_builds() {
        for name in NAMES {
            assert!(by_name(name).is_some(), "scene {name} did not build");
        }
        assert!(by_name("no such scene").is_none());
    }
}
