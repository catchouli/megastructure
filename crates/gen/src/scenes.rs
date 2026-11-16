//! Named scenes. Binaries select a scene by name so that new scenes need no changes outside
//! this crate.

use crate::{Collider, MeshData};

/// A scene ready to upload, with a place to put the camera or player.
pub struct Scene {
    /// The name this scene was built under; renderers use it to pick a preset and sun.
    pub name: &'static str,
    pub mesh: MeshData,
    /// Eye position to start at, in metres.
    pub spawn: [f32; 3],
    /// Initial yaw in radians; 0 looks along +Z.
    pub spawn_yaw: f32,
    /// Solids for the walking controller (boxes and stair ramps). Empty for scenes built
    /// before colliders existed (`test`).
    pub colliders: Vec<Collider>,
}

/// Names accepted by [`by_name`]. The first is the default.
pub const NAMES: &[&str] = &["test", "shaft", "labyrinth"];

pub const DEFAULT: &str = NAMES[0];

/// Build the named scene, or `None` if the name is unknown.
pub fn by_name(name: &str) -> Option<Scene> {
    match name {
        "test" => Some(Scene {
            name: "test",
            mesh: crate::test_scene::build(),
            spawn: [0.0, 1.65, 2.0],
            spawn_yaw: 0.0,
            colliders: Vec::new(),
        }),
        "shaft" => {
            let s = crate::shaft::build(crate::shaft::DEFAULT_SEED);
            let (spawn, spawn_yaw) = crate::shaft::spawn();
            Some(Scene { name: "shaft", mesh: s.mesh, spawn, spawn_yaw, colliders: s.colliders })
        }
        "labyrinth" => {
            let l = crate::labyrinth::build(crate::labyrinth::DEFAULT_SEED);
            let (spawn, spawn_yaw) = l.spawn();
            Some(Scene { name: "labyrinth", mesh: l.mesh, spawn, spawn_yaw, colliders: l.colliders })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_builds() {
        for name in NAMES {
            let s = by_name(name).unwrap_or_else(|| panic!("scene {name} did not build"));
            s.mesh.check_invariants().unwrap_or_else(|e| panic!("scene {name}: {e}"));
            assert!(s.spawn.iter().all(|v| v.is_finite()) && s.spawn_yaw.is_finite());
        }
        assert!(by_name("no such scene").is_none());
    }
}
