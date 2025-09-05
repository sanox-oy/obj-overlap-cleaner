use std::collections::HashMap;
use three_d_asset::{Indices, Positions, TriMesh};

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

    pub fn get_indices(&self, x: f32, y: f32, z: f32) -> Option<&[u32]> {
        let x = (x * GRID_RESOLUTION as f32) as i32;
        let y = (y * GRID_RESOLUTION as f32) as i32;
        let z = (z * GRID_RESOLUTION as f32) as i32;

        let yz = self.indices.get(&x)?;
        let z_indices = yz.get(&y)?;
        let indices = z_indices.get(&z)?;

        Some(indices)
    }

    pub fn push_index(&mut self, x: f32, y: f32, z: f32, index: u32) {
        let x = (x * GRID_RESOLUTION as f32) as i32;
        let y = (y * GRID_RESOLUTION as f32) as i32;
        let z = (z * GRID_RESOLUTION as f32) as i32;

        self.indices.entry(x).or_insert_with(|| HashMap::new());

        let yz = self.indices.get_mut(&x).unwrap();

        yz.entry(y).or_insert_with(|| HashMap::new());

        let z_indices = yz.get_mut(&y).unwrap();

        z_indices.entry(z).or_insert_with(|| vec![]);

        let indices = z_indices.get_mut(&z).unwrap();
        indices.push(index);
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

        for index in indices.chunks_exact(3) {
            // TODO: Push to neighboring cells, if some of p1 or p2 falls on neighbor side
            let p0 = positions[index[0] as usize];
            self.push_index(p0.x, p0.y, p0.z, index[0]);
            self.push_index(p0.x, p0.y, p0.z, index[1]);
            self.push_index(p0.x, p0.y, p0.z, index[2]);
        }
    }
}
