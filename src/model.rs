use three_d_asset::{AxisAlignedBoundingBox, Positions, TriMesh, Vec3, Vector2};
use tobj::{Material as TobjMaterial, Mesh as TobjMesh};

fn tobj_mesh_to_trimesh(mesh: TobjMesh) -> TriMesh {
    let uvs = if mesh.texcoords.len() > 0 {
        Some(
            mesh.texcoords
                .chunks_exact(2)
                .map(|uv| Vector2::<f32>::new(uv[0], uv[1]))
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };

    let normals = if mesh.normals.len() > 0 {
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

pub struct Model {
    pub meshes: Vec<TriMesh>,
    pub materials: Vec<TobjMaterial>,
    pub aabbs: Vec<AxisAlignedBoundingBox>,
    pub aabb: AxisAlignedBoundingBox,
}

impl TryFrom<tobj::LoadResult> for Model {
    type Error = tobj::LoadError;

    fn try_from(load_result: tobj::LoadResult) -> Result<Self, Self::Error> {
        let (models, materials) = load_result?;

        let meshes = models
            .into_iter()
            .map(|m| tobj_mesh_to_trimesh(m.mesh))
            .collect::<Vec<_>>();

        let aabbs = meshes.iter().map(|m| m.compute_aabb()).collect::<Vec<_>>();

        let mut aabb = AxisAlignedBoundingBox::EMPTY;
        for ab in aabbs.iter() {
            aabb.expand_with_aabb(*ab);
        }

        Ok(Model {
            meshes,
            materials: materials?,
            aabbs,
            aabb,
        })
    }
}
