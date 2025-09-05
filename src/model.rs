use core::f32;
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
};

use three_d_asset::{
    AxisAlignedBoundingBox, Indices, InnerSpace, MetricSpace, Positions, TriMesh, Vec3, Vector2,
};
use tobj::{Material as TobjMaterial, Mesh as TobjMesh};

use crate::grid::IndexGrid;

fn tobj_mesh_to_trimesh(mesh: TobjMesh) -> TriMesh {
    let uvs = if !mesh.texcoords.is_empty() {
        Some(
            mesh.texcoords
                .chunks_exact(2)
                .map(|uv| Vector2::<f32>::new(uv[0], uv[1]))
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };

    let normals = if !mesh.normals.is_empty() {
        Some(
            mesh.normals
                .chunks_exact(3)
                .map(|n| Vec3::new(n[0], n[1], n[2]))
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };

    TriMesh {
        positions: Positions::F32(
            mesh.positions
                .chunks_exact(3)
                .map(|p| Vec3::new(p[0], p[1], p[2]))
                .collect::<Vec<_>>(),
        ),
        indices: three_d_asset::Indices::U32(mesh.indices),
        uvs,
        normals,
        tangents: None,
        colors: None,
    }
}

fn try_load_and_process_obj(
    path: &OsStr,
) -> Result<(Vec<TriMesh>, Vec<TobjMaterial>), tobj::LoadError> {
    let (models, materials) = tobj::load_obj(
        path,
        &tobj::LoadOptions {
            single_index: true,
            ..Default::default()
        },
    )?;

    let meshes = models
        .into_iter()
        .map(|m| tobj_mesh_to_trimesh(m.mesh))
        .collect::<Vec<_>>();

    Ok((meshes, materials?))
}

fn vertex_overlapping(vertex: &Vec3, mesh_container: &MeshContainer, threshold: f32) -> bool {
    let index_grid = mesh_container.index_grid.as_ref().unwrap();

    // TODO: Expand with contents of neighboring cells if closer than threshold to boundary
    let Some(indices) = index_grid.get_indices(vertex.x, vertex.y, vertex.z) else {
        return false;
    };

    let vertices = match &mesh_container.mesh.positions {
        Positions::F32(vertices) => vertices,
        _ => panic!("Not F32"),
    };

    let threshold2 = threshold.powi(2) * 4.0;

    for indexes in indices.chunks_exact(3) {
        let p0 = vertices[indexes[0] as usize];
        if p0.distance2(*vertex) > threshold2 {
            continue;
        }

        let p1 = vertices[indexes[1] as usize];
        let p2 = vertices[indexes[2] as usize];

        let normal = (p1 - p0).cross(p2 - p0).normalize();

        // distance between vertex and plane
        let dist = normal.dot(*vertex - p0).abs();
        if dist > threshold {
            continue;
        }

        // At this point the distance is already less than threshold.
        // Just check that the point lands within the triangle

        // Check against first edge
        let e0n = (p1 - p0).cross(normal);
        if e0n.dot(*vertex - p0) > 0.0 {
            continue;
        }

        // Second edge
        let e1n = (p2 - p1).cross(normal);
        if e1n.dot(*vertex - p1) > 0.0 {
            continue;
        }

        // Final edge
        let e2n = (p0 - p2).cross(normal);
        if e2n.dot(*vertex - p2) > 0.0 {
            continue;
        }

        return true;
    }

    false
}

#[derive(Debug)]
pub struct ModelReference {
    pub source_file: OsString,
    pub materials: Vec<TobjMaterial>,
    pub texture_downscale_factor: u32,
}

#[derive(Debug)]
pub struct MeshContainer {
    mesh: TriMesh,
    aabb: AxisAlignedBoundingBox,
    material: TobjMaterial,
    /// List of indices of vertices that are overlapping with other
    /// models
    pub overlapping_vertice_idxs: Vec<usize>,
    /// Indicates whether this mesh is totally overlapping
    to_be_deleted: bool,
    mean_edge_len: Option<f32>,

    /// List of indices that are to be deleted.
    /// Created from overlapping_vertice_idxs, where
    /// those that are on the edge are removed (i.e. has neigbors that are non-overlapping)
    indices_to_delete: HashSet<usize>,
    index_grid: Option<IndexGrid>,
}

impl MeshContainer {
    fn modified(&self) -> bool {
        self.to_be_deleted || self.overlapping_vertice_idxs.is_empty()
    }

    pub fn new(
        mesh: TriMesh,
        material: TobjMaterial,
        calc_edge_len: bool,
        init_index_grid: bool,
    ) -> Self {
        let aabb = mesh.compute_aabb();

        let mean_edge_len = match calc_edge_len {
            true => {
                let mut len_sum = 0.0;
                let mut len_cnt = 0;
                let positions = match &mesh.positions {
                    Positions::F32(positions) => positions,
                    _ => panic!("Positions not F32"),
                };
                mesh.for_each_triangle(|i0, i1, i2| {
                    let p0 = positions[i0];
                    let p1 = positions[i1];
                    let p2 = positions[i2];

                    len_sum += p0.distance(p1);
                    len_sum += p1.distance(p2);
                    len_sum += p2.distance(p0);
                    len_cnt += 3;
                });
                Some(len_sum / len_cnt as f32)
            }
            false => None,
        };

        let index_grid = match init_index_grid {
            true => {
                let mut index_grid = IndexGrid::new();
                index_grid.populate_from_trimesh(&mesh);
                Some(index_grid)
            }
            false => None,
        };

        Self {
            mesh,
            aabb,
            material,
            overlapping_vertice_idxs: vec![],
            to_be_deleted: false,
            mean_edge_len,
            indices_to_delete: HashSet::new(),
            index_grid,
        }
    }

    /// Calculates vertice indices from self, which are overlapping with other
    pub fn calc_overlapping_vertice_idxs(&self, other: &Self) -> Vec<usize> {
        let mut overlapping = vec![];
        let threshold = self
            .mean_edge_len
            .expect("Trying to calculate overlapping without mean edge len");

        if let Some(intersection) = self.aabb.intersection(other.aabb) {
            match &self.mesh.positions {
                Positions::F32(vertices) => {
                    for (idx, vertex) in vertices.iter().enumerate() {
                        if intersection.is_inside(*vertex)
                            && vertex_overlapping(vertex, other, threshold) {
                                overlapping.push(idx);
                            }
                    }
                }
                _ => panic!("Positions are not F32"),
            }
        }
        overlapping
    }

    /// Mark indices that are to be deleted
    /// If all are deleted, rather set to_be_deleted to true
    fn mark_vertices_to_delete(&mut self) {
        if self.overlapping_vertice_idxs.is_empty() {
            return;
        }

        if self.overlapping_vertice_idxs.len() == self.mesh.indices.len().unwrap() {
            println!("Whole mesh to be deleted");
            self.to_be_deleted = true;
            return;
        }

        let indices = match &self.mesh.indices {
            Indices::U32(indices) => indices,
            _ => panic!("Indices not U32"),
        };

        let mut indices_to_delete =
            HashSet::from_iter(self.overlapping_vertice_idxs.iter().cloned());

        // Iterate over each triangle
        for t_indices in indices.chunks_exact(3) {
            // If all or none are overlapping, just continue
            let overlapping = t_indices
                .iter()
                .map(|i| indices_to_delete.contains(&(*i as usize)))
                .collect::<Vec<_>>();

            if overlapping.iter().all(|v| *v) || overlapping.iter().all(|v| !*v) {
                continue;
            }

            // The remaining case is so that they have non-overlapping neighbors
            for (idx, overlaps) in overlapping.iter().enumerate() {
                if *overlaps {
                    indices_to_delete.remove(&(t_indices[idx] as usize));
                }
            }
        }

        self.indices_to_delete = indices_to_delete;
    }

    fn do_delete_vertices(&mut self) {
        let mut vertices = self.mesh.positions.to_f32();
        let mut indices = self.mesh.indices.to_u32().unwrap();

        let mut vertices_to_delete = Vec::from_iter(self.indices_to_delete.iter().cloned());
        vertices_to_delete.sort();

        for idx in vertices_to_delete.iter().rev() {
            let mut t_indices_to_remove = vec![];

            // First remove the vertex
            vertices.remove(*idx);

            // Then remove the triangle
            for (i, t_indices) in indices.chunks_exact(3).enumerate() {
                if t_indices.contains(&(*idx as u32)) {
                    t_indices_to_remove.push(1);
                }
            }

            for t_index in t_indices_to_remove.iter().rev() {
                indices.remove(*t_index * 3 + 2);
                indices.remove(*t_index * 3 + 1);
                indices.remove(*t_index);
            }

            // Then subtract one from all indices that are > then idx
            for index in indices.iter_mut() {
                if *index > *idx as u32 {
                    *index -= 1;
                }
            }
        }

        self.mesh.positions = Positions::F32(vertices);
        self.mesh.indices = Indices::U32(indices);
    }
}

#[derive(Debug)]
pub struct Model {
    pub meshes: Vec<MeshContainer>,
    pub aabb: AxisAlignedBoundingBox,
    source_file: OsString,
}

impl Model {
    pub fn try_new_from_file(
        path: OsString,
        calc_edge_len: bool,
        init_index_grid: bool,
    ) -> Result<Self, tobj::LoadError> {
        let (tri_meshes, materials) = try_load_and_process_obj(&path)?;

        let meshes = tri_meshes
            .into_iter()
            .zip(materials)
            .map(|(mesh, material)| {
                MeshContainer::new(mesh, material, calc_edge_len, init_index_grid)
            })
            .collect::<Vec<_>>();

        let mut aabb = AxisAlignedBoundingBox::EMPTY;
        for mesh in meshes.iter() {
            aabb.expand_with_aabb(mesh.aabb);
        }

        Ok(Self {
            meshes,
            aabb,
            source_file: path,
        })
    }

    pub fn modified(&self) -> bool {
        self.meshes.iter().any(|m| m.modified())
    }

    pub fn mark_vertices_to_delete(&mut self) {
        for mesh in self.meshes.iter_mut() {
            mesh.mark_vertices_to_delete();
        }
    }

    pub fn do_delete_vertices(&mut self) {
        let mut meshes_to_delete = vec![];

        for (idx, mesh) in self.meshes.iter_mut().enumerate() {
            if mesh.to_be_deleted {
                meshes_to_delete.push(idx);
                continue;
            }
            mesh.do_delete_vertices();
        }

        for idx in meshes_to_delete.iter().rev() {
            self.meshes.remove(*idx);
        }
    }

    pub fn to_be_deleted(&self) -> bool {
        self.meshes.iter().all(|m| m.to_be_deleted)
    }
}

impl ModelReference {
    pub fn from_model(model: Model, texture_downscale_factor: u32) -> Self {
        let materials = model
            .meshes
            .into_iter()
            .map(|m| m.material)
            .collect::<Vec<_>>();
        Self {
            materials,
            texture_downscale_factor,
            source_file: model.source_file,
        }
    }
}

pub enum OutAsset {
    AssetRef(ModelReference),
    Asset(Model),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directly_above_outside_threshold() {
        let trimesh = TriMesh {
            positions: Positions::F32(vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ]),
            indices: Indices::U32(vec![0, 1, 2]),
            normals: None,
            tangents: None,
            uvs: None,
            colors: None,
        };

        let vertex = Vec3::new(0.0, 0.0, 1.1);

        let result = vertex_overlapping(&vertex, &trimesh, 1.0);
        assert_eq!(result, false);
    }

    #[test]
    fn test_directly_above_inside_threshold() {
        let trimesh = TriMesh {
            positions: Positions::F32(vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ]),
            indices: Indices::U32(vec![0, 1, 2]),
            normals: None,
            tangents: None,
            uvs: None,
            colors: None,
        };

        let vertex = Vec3::new(0.0, 0.0, 1.0);

        let result = vertex_overlapping(&vertex, &trimesh, 1.0);
        assert_eq!(result, true);
    }

    #[test]
    fn test_directly_below_outside_threshold() {
        let trimesh = TriMesh {
            positions: Positions::F32(vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ]),
            indices: Indices::U32(vec![0, 1, 2]),
            normals: None,
            tangents: None,
            uvs: None,
            colors: None,
        };

        let vertex = Vec3::new(0.0, 0.0, -1.1);

        let result = vertex_overlapping(&vertex, &trimesh, 1.0);
        assert_eq!(result, false);
    }

    #[test]
    fn test_directly_below_inside_threshold() {
        let trimesh = TriMesh {
            positions: Positions::F32(vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ]),
            indices: Indices::U32(vec![0, 1, 2]),
            normals: None,
            tangents: None,
            uvs: None,
            colors: None,
        };

        let vertex = Vec3::new(0.0, 0.0, -1.0);

        let result = vertex_overlapping(&vertex, &trimesh, 1.0);
        assert_eq!(result, true);
    }
}
