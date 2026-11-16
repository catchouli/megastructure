//! The space graph (layer L2 of `docs/PLAN.md`): typed spaces and typed connections, before
//! any geometry. Built by the director and the grammar in `plan.rs`; embedded in 3D there too.

use glam::Vec3;

/// Kinds of space, of very different scales.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpaceKind {
    /// The deep void: a shaft open to the sky through the whole height, crossed by bridges,
    /// with ledges on its walls. Set piece; contains ledges.
    Void,
    /// A narrower shaft open to the sky, starting part way up. Contains ledges.
    LightWell,
    /// The great hall: two rows of giant columns, a giant arch onto the void, a grand stair up
    /// to a gallery along one wall, tall windows in its exterior end. Set piece.
    GreatHall,
    /// The gallery inside the great hall, reached by its grand stair.
    HallGallery,
    /// A ledge on a void wall: a balcony overlooking the void.
    Ledge,
    /// A square tower with a stair turning round an open well, doors at many storeys.
    StairTower,
    /// A tall passage (3.2 m wide, 5.4 m high), with stair halls where it changes storey.
    Corridor,
    /// A low, narrow passage (2 m wide, 2.8 m high).
    ServicePassage,
    /// A low pillared hall at the bottom of the structure, enclosed and dark. Set piece.
    Undercroft,
    /// A small room where passages meet.
    Junction,
    /// A small dark room at the end of a spur.
    Chamber,
    /// A colonnaded room open to the outside of the structure.
    Loggia,
    /// A cistern: a deep basin with a forest of columns, a walkway round its walls and a
    /// stair down to the basin floor. Set piece.
    Cistern,
    /// A long hall climbed by one monumental stair to a landing across its far end. Set piece.
    StairHall,
    /// The landing at the top of the stair hall.
    StairLanding,
    /// A narrow light shaft from the sky down into a room below (no floor of its own).
    Shaft,
    /// A small room before the door of a great space.
    Antechamber,
    /// A small dead-end room off a passage: a cell, a store room, a shrine or a machine
    /// closet (`Room::style` 0 to 3).
    SideRoom,
    /// A vent: a duct 1.2 m square, crawled through; shortcuts and secrets.
    Vent,
}

impl SpaceKind {
    pub fn name(self) -> &'static str {
        match self {
            SpaceKind::Void => "void",
            SpaceKind::LightWell => "light well",
            SpaceKind::GreatHall => "great hall",
            SpaceKind::HallGallery => "hall gallery",
            SpaceKind::Ledge => "ledge",
            SpaceKind::StairTower => "stair tower",
            SpaceKind::Corridor => "corridor",
            SpaceKind::ServicePassage => "service passage",
            SpaceKind::Undercroft => "undercroft",
            SpaceKind::Junction => "junction",
            SpaceKind::Chamber => "chamber",
            SpaceKind::Loggia => "loggia",
            SpaceKind::Cistern => "cistern",
            SpaceKind::StairHall => "stair hall",
            SpaceKind::StairLanding => "stair landing",
            SpaceKind::Shaft => "light shaft",
            SpaceKind::Antechamber => "antechamber",
            SpaceKind::SideRoom => "side room",
            SpaceKind::Vent => "vent",
        }
    }

    pub fn is_passage(self) -> bool {
        matches!(self, SpaceKind::Corridor | SpaceKind::ServicePassage | SpaceKind::Vent)
    }

    /// Containers hold other walkable spaces (ledges) but have no floor on the route.
    pub fn is_container(self) -> bool {
        matches!(self, SpaceKind::Void | SpaceKind::LightWell | SpaceKind::Shaft)
    }

    /// Great spaces get antechambers and heavy door frames.
    pub fn is_great(self) -> bool {
        matches!(self, SpaceKind::GreatHall | SpaceKind::Undercroft | SpaceKind::Cistern | SpaceKind::StairHall)
    }
}

/// Kinds of connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinkKind {
    /// A door at human scale.
    Doorway,
    /// A passage's full width opening into a larger space.
    Opening,
    /// A giant round arch between two great spaces.
    Archway,
    /// A bridge across a void between two ledges.
    Bridge,
    /// A stair climbing a void wall between two ledges.
    WallStair,
    /// The grand stair inside the great hall, up to its gallery.
    HallStair,
    /// A vent mouth (crawled through).
    VentMouth,
}

/// Where a space or connection sits in the structure of the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// On the core loop through the set pieces.
    Core,
    /// On a second route that closes another cycle with the core.
    Loop,
    /// A dead end hanging off the core.
    Spur,
    /// A container (a void), not walked itself.
    Container,
}

#[derive(Clone, Debug)]
pub struct Space {
    /// Stable id: a path, used for seeding.
    pub sid: String,
    pub kind: SpaceKind,
    pub role: Role,
    /// The void a ledge belongs to.
    pub parent: Option<usize>,
    /// Floor storey (the lowest for multi-storey spaces).
    pub level: i32,
    /// Set by the director for set pieces ("deepest void", "biggest hall", ...).
    pub set_piece: Option<&'static str>,
    /// Points on the walkable floor of the space, filled in by the embedding; the
    /// reachability test needs every one reached.
    pub anchors: Vec<Vec3>,
    /// World bounds, filled in by the embedding.
    pub bounds: Option<(Vec3, Vec3)>,
    /// Reached only by crawling (vents and the ledges they open onto).
    pub crawl_only: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub a: usize,
    pub b: usize,
    pub kind: LinkKind,
    pub role: Role,
}

#[derive(Clone, Debug, Default)]
pub struct SpaceGraph {
    pub spaces: Vec<Space>,
    pub links: Vec<Link>,
}

impl SpaceGraph {
    pub fn add(&mut self, sid: impl Into<String>, kind: SpaceKind, role: Role, level: i32) -> usize {
        self.spaces.push(Space { sid: sid.into(), kind, role, parent: None, level, set_piece: None, anchors: Vec::new(), bounds: None, crawl_only: kind == SpaceKind::Vent });
        self.spaces.len() - 1
    }

    pub fn link(&mut self, a: usize, b: usize, kind: LinkKind, role: Role) {
        self.links.push(Link { a, b, kind, role });
    }

    /// Spaces a walker can stand in (everything but containers).
    pub fn walkable(&self) -> impl Iterator<Item = usize> + '_ {
        self.spaces.iter().enumerate().filter(|(_, s)| !s.kind.is_container() && s.role != Role::Container).map(|(i, _)| i)
    }

    pub fn degree(&self, i: usize) -> usize {
        self.links.iter().filter(|l| l.a == i || l.b == i).count()
    }

    /// Indices of links that are bridges (graph-theoretic: removing one disconnects the
    /// graph), among links accepted by `keep`.
    pub fn cut_links(&self, keep: &dyn Fn(&Link) -> bool) -> Vec<usize> {
        let n = self.spaces.len();
        let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
        for (k, l) in self.links.iter().enumerate() {
            if keep(l) {
                adj[l.a].push((l.b, k));
                adj[l.b].push((l.a, k));
            }
        }
        let mut disc = vec![usize::MAX; n];
        let mut low = vec![0usize; n];
        let mut out = Vec::new();
        let mut t = 0;
        for root in 0..n {
            if disc[root] != usize::MAX || adj[root].is_empty() {
                continue;
            }
            // Iterative DFS: (node, parent link, next neighbour index).
            let mut stack: Vec<(usize, usize, usize)> = vec![(root, usize::MAX, 0)];
            disc[root] = t;
            low[root] = t;
            t += 1;
            while !stack.is_empty() {
                let top = stack.len() - 1;
                let (v, pl, i) = stack[top];
                if i < adj[v].len() {
                    let (w, k) = adj[v][i];
                    stack[top].2 += 1;
                    if k == pl {
                        continue;
                    }
                    if disc[w] == usize::MAX {
                        disc[w] = t;
                        low[w] = t;
                        t += 1;
                        stack.push((w, k, 0));
                    } else {
                        low[v] = low[v].min(disc[w]);
                    }
                } else {
                    stack.pop();
                    if let Some(&(u, _, _)) = stack.last() {
                        low[u] = low[u].min(low[v]);
                        if low[v] > disc[u] {
                            out.push(pl);
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// Whether every space reachable from `from` through links accepted by `keep`.
    pub fn connected_from(&self, from: usize, keep: &dyn Fn(&Link) -> bool) -> Vec<bool> {
        let mut seen = vec![false; self.spaces.len()];
        let mut stack = vec![from];
        seen[from] = true;
        while let Some(v) = stack.pop() {
            for l in self.links.iter().filter(|l| keep(l)) {
                let w = if l.a == v {
                    l.b
                } else if l.b == v {
                    l.a
                } else {
                    continue;
                };
                if !seen[w] {
                    seen[w] = true;
                    stack.push(w);
                }
            }
        }
        seen
    }

    /// Number of independent cycles among links accepted by `keep` (edges - nodes + parts).
    pub fn cycle_rank(&self, keep: &dyn Fn(&Link) -> bool) -> usize {
        let used: Vec<usize> = {
            let mut v: Vec<usize> = self.links.iter().filter(|l| keep(l)).flat_map(|l| [l.a, l.b]).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let edges = self.links.iter().filter(|l| keep(l)).count();
        let mut parts = 0;
        let mut seen = vec![false; self.spaces.len()];
        for &s in &used {
            if seen[s] {
                continue;
            }
            parts += 1;
            let c = self.connected_from(s, keep);
            for (i, &x) in c.iter().enumerate() {
                seen[i] |= x;
            }
        }
        (edges + parts).saturating_sub(used.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_links_on_a_theta_with_a_spur() {
        let mut g = SpaceGraph::default();
        let n: Vec<usize> = (0..6).map(|i| g.add(format!("n{i}"), SpaceKind::Junction, Role::Core, 0)).collect();
        // Cycle 0-1-2-3-0, chord 1-3, spur 3-4-5.
        for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0), (1, 3), (3, 4), (4, 5)] {
            g.link(n[a], n[b], LinkKind::Doorway, Role::Core);
        }
        let cuts = g.cut_links(&|_| true);
        assert_eq!(cuts, vec![5, 6]);
        assert_eq!(g.cycle_rank(&|_| true), 2);
    }
}
