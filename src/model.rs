use std::ffi::{OsStr, OsString};

use three_d_asset::{AxisAlignedBoundingBox, Positions, TriMesh, Vec3, Vector2};
use tobj::{Material as TobjMaterial, Mesh as TobjMesh};

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

pub struct MeshContainer {
    mesh: TriMesh,
    aabb: AxisAlignedBoundingBox,
    material: TobjMaterial,
    /// List of indices of vertices that are overlapping with other
    /// models
    overlapping_vertice_idxs: Vec<usize>,
    /// Indicates whether this mesh is totally overlapping
    to_be_deleted: bool,
}

impl MeshContainer {
    fn modified(&self) -> bool {
        self.to_be_deleted || self.overlapping_vertice_idxs.is_empty()
    }

    /// Calculates vertice indices from self, which are overlapping with other
    pub fn calc_overlapping_vertice_idxs(&self, other: &Self) -> Vec<usize> {
        let mut overlapping = vec![];
        if let Some(intersection) = self.aabb.intersection(other.aabb) {
            //let vertices =
            match &self.mesh.positions {
                Positions::F32(vertices) => {
                    for (idx, vertex) in vertices.iter().enumerate() {
                        if intersection.is_inside(*vertex) {}
                    }
                }
                _ => panic!("Positions are not F32"),
            }
        }
        return overlapping;
    }
}

pub struct Model {
    pub meshes: Vec<MeshContainer>,
    pub aabb: AxisAlignedBoundingBox,
    source_file: OsString,
}

impl Model {
    pub fn try_new_from_file(path: OsString) -> Result<Self, tobj::LoadError> {
        let (tri_meshes, materials) = try_load_and_process_obj(&path)?;

        let meshes = tri_meshes
            .into_iter()
            .zip(materials)
            .map(|(mesh, material)| {
                let aabb = mesh.compute_aabb();
                MeshContainer {
                    mesh,
                    aabb,
                    material,
                    overlapping_vertice_idxs: vec![],
                    to_be_deleted: false,
                }
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
}
