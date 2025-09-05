use std::collections::HashMap;
use three_d_asset::{AxisAlignedBoundingBox, Indices, Positions, TriMesh, Vec3, Vector3};

const GRID_RESOLUTION: u32 = 10;

#[derive(Debug)]
pub struct IndexGrid {
    indices: HashMap<i32, HashMap<i32, HashMap<i32, Vec<u32>>>>,
}

impl IndexGrid {
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
        }
    }

    fn get_cell(&self, x: i32, y: i32, z: i32) -> Option<&[u32]> {
        let yz = self.indices.get(&x)?;
        let z_indices = yz.get(&y)?;
        let indices = z_indices.get(&z)?;

        Some(indices)
    }

    pub fn get_indices(&self, p: &Vec3, threshold: f32) -> Vec<u32> {
        let p_min = (p - Vec3::new(threshold, threshold, threshold))
            .map(|x| (x * GRID_RESOLUTION as f32) as i32);
        let p_max = (p + Vec3::new(threshold, threshold, threshold))
            .map(|x| (x * GRID_RESOLUTION as f32) as i32);

        let mut indices = Vec::new();

        for x in p_min.x..(p_max.x + 1) {
            for y in p_min.y..(p_max.y + 1) {
                for z in p_min.z..(p_max.z + 1) {
                    if let Some(ind) = self.get_cell(x, y, z) {
                        indices.extend_from_slice(ind);
                    }
                }
            }
        }

        indices
    }

    fn extend(&mut self, p: Vector3<i32>, index_slice: &[u32]) {
        self.indices.entry(p.x).or_default();

        let yz = self.indices.get_mut(&p.x).unwrap();

        yz.entry(p.y).or_default();

        let z_indices = yz.get_mut(&p.y).unwrap();

        z_indices.entry(p.z).or_default();

        let indices = z_indices.get_mut(&p.z).unwrap();
        indices.extend_from_slice(index_slice);
    }

    pub fn populate_from_trimesh(&mut self, mesh: &TriMesh) {
        let positions = match &mesh.positions {
            Positions::F32(postions) => postions,
            _ => panic!("Positions not F32"),
        };

        let indices = match &mesh.indices {
            Indices::U32(indices) => indices,
            _ => panic!("Indices not U32"),
        };

        for tri in indices.chunks_exact(3) {
            // TODO: Push to neighboring cells, if some of p1 or p2 falls on neighbor side
            let p0: Vector3<i32> =
                positions[tri[0] as usize].map(|x| (x * GRID_RESOLUTION as f32) as i32);
            let p1: Vector3<i32> =
                positions[tri[1] as usize].map(|x| (x * GRID_RESOLUTION as f32) as i32);
            let p2: Vector3<i32> =
                positions[tri[2] as usize].map(|x| (x * GRID_RESOLUTION as f32) as i32);
            self.extend(p0, tri);

            if p1 != p0 {
                self.extend(p1, tri);
            }

            if p2 != p1 && p2 != p0 {
                self.extend(p2, tri);
            }
        }
    }
}
