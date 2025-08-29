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

pub struct Model {
    meshes: Option<Vec<TriMesh>>,
    pub materials: Vec<TobjMaterial>,
    pub aabbs: Vec<AxisAlignedBoundingBox>,
    pub aabb: AxisAlignedBoundingBox,
    source_file: OsString,
}

impl Model {
    pub fn try_new_from_file(path: OsString) -> Result<Self, tobj::LoadError> {
        let (meshes, materials) = try_load_and_process_obj(&path)?;

        let aabbs = meshes.iter().map(|m| m.compute_aabb()).collect::<Vec<_>>();

        let mut aabb = AxisAlignedBoundingBox::EMPTY;
        for ab in aabbs.iter() {
            aabb.expand_with_aabb(*ab);
        }

        Ok(Self {
            meshes: None,
            materials,
            aabbs,
            aabb,
            source_file: path,
        })
    }

    pub fn get_meshes(&mut self) -> &Vec<TriMesh> {
        if self.meshes.is_none() {
            let (models, _) = try_load_and_process_obj(&self.source_file).expect("LoadError");
        }

        self.meshes.as_ref().unwrap()
    }

    pub fn drop_meshes(&mut self) {
        self.meshes = None;
    }
}
