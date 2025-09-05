use std::{
    ffi::OsString,
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
};

use image::ImageReader;
use three_d_asset::{Vec2, Vec3};

use crate::messages;
use crate::messages::ModelLoadTask;
use crate::model::{Model, ModelReference, OutAsset};

pub fn model_load_runner(
    rx: Arc<Mutex<mpsc::Receiver<ModelLoadTask>>>,
    tx: mpsc::Sender<messages::ModelLoadTaskResponse>,
) {
    loop {
        let msg = {
            let Ok(receiver) = rx.lock() else {
                continue;
            };
            receiver.recv()
        };
        match msg {
            Ok(task) => match task {
                ModelLoadTask::Task(task) => {
                    let path = task.path;
                    let model = Model::try_new_from_file(path.clone(), true, false)
                        .unwrap_or_else(|_| panic!("Failed loading model from {path:?}"));

                    println!("Successfully loaded model from: {path:?}");

                    for (idx, mesh) in model.meshes.iter().enumerate() {
                        println!(
                            "Mesh {idx} has {} vertices and {:?} indices, uvs: {}",
                            mesh.mesh.positions.len(),
                            mesh.mesh.indices.len(),
                            mesh.mesh.uvs.as_ref().unwrap().len(),
                        );
                    }

                    tx.send(messages::ModelLoadTaskResponse::Model(
                        messages::ModelContainer { model },
                    ))
                    .expect("Failed to send result");
                }
                ModelLoadTask::Terminate => {
                    println!("Model load runner done");
                    tx.send(messages::ModelLoadTaskResponse::Terminated)
                        .expect("Failed to send result");
                    return;
                }
            },
            Err(e) => println!("Error: {e} encountered while waiting for messages"),
        };
    }
}

pub fn scan_folder_for_objs(folder: &OsString) -> impl Iterator<Item = OsString> {
    let folder = std::path::Path::new(&folder);

    let mut read_dir = folder.read_dir().unwrap();

    std::iter::from_fn(move || {
        loop {
            let entry = read_dir.next()?;
            let p = entry.unwrap().path();

            if let Some(extension) = p.extension()
                && extension.eq_ignore_ascii_case("obj")
            {
                return Some(p.into_os_string());
            }
        }
    })
}

pub fn scan_folder_and_create_tasks(
    folder: &OsString,
    tx: &mpsc::Sender<crate::messages::ModelLoadTask>,
) {
    for obj_file in scan_folder_for_objs(folder) {
        tx.send(crate::messages::ModelLoadTask::Task(
            crate::messages::TaskContainer { path: obj_file },
        ))
        .expect("Error while sending task");
    }
}

fn copy_texture(
    texture_file: &str,
    source_folder: &Path,
    dest_folder: &Path,
    downscale_factor: u32,
) {
    let texture_src = source_folder.join(texture_file);
    let texture_dst = dest_folder.join(texture_file);
    if texture_dst.exists() {
        return;
    }

    if !texture_src.exists() {
        panic!("Unable to load texture: {texture_src:?}");
    }

    if downscale_factor == 1 {
        std::fs::copy(texture_src, texture_dst).expect("Failed to copy texture");
    } else {
        let img = ImageReader::open(texture_src)
            .expect("Couldnt open image")
            .decode()
            .expect("Couldnt decode image");

        let resized = img.resize_exact(
            img.width() / downscale_factor,
            img.height() / downscale_factor,
            image::imageops::FilterType::Triangle,
        );

        resized.save(texture_dst).expect("Couldnt save image");
    }
}

fn write_header(writer: &mut BufWriter<File>) {
    writeln!(writer, "#").expect("Failed to write mesh");
    writeln!(writer, "# Wavefront OBJ file").expect("Failed to write mesh");
    writeln!(writer, "# Created by obj-overlap-cleaner").expect("Failed to write mesh");
    writeln!(writer, "# https://github.com/sanox-oy/obj-overlap-cleaner")
        .expect("Failed to write mesh");
    writeln!(writer, "#").expect("Failed to write mesh");
}

fn write_mtllib(
    source_folder: &Path,
    dest_folder: &Path,
    dest: PathBuf,
    materials: &[&tobj::Material],
) {
    let file = File::create(dest).expect("Couldnt create file");
    let mut file_buf = BufWriter::new(file);

    write_header(&mut file_buf);

    for material in materials {
        writeln!(file_buf).expect("Failed to write mesh");
        writeln!(file_buf, "newmtl {}", material.name).expect("Failed to write mesh");
        if let Some(ka) = material.ambient {
            writeln!(file_buf, "Ka {} {} {}", ka[0], ka[1], ka[2]).expect("Failed to write mesh");
        }
        if let Some(kd) = material.diffuse {
            writeln!(file_buf, "Kd {} {} {}", kd[0], kd[1], kd[2]).expect("Failed to write mesh");
        }
        if let Some(d) = material.dissolve {
            writeln!(file_buf, "d {}", d).expect("Failed to write mesh");
        }
        if let Some(ns) = material.shininess {
            writeln!(file_buf, "Ns {}", ns).expect("Failed to write mesh");
        }
        if let Some(illum) = material.illumination_model {
            writeln!(file_buf, "illum {}", illum).expect("Failed to write mesh");
        }
        if let Some(map_kd) = &material.diffuse_texture {
            writeln!(file_buf, "map_Kd {}", map_kd).expect("Failed to write mesh");

            // Also process the texture
            copy_texture(map_kd, source_folder, dest_folder, 2);
        }
    }

    file_buf.flush().expect("Failed to write to disk");
}

pub trait WriteToFolder {
    fn write_to_folder(&self, folder: &OsString);
}

impl WriteToFolder for Model {
    fn write_to_folder(&self, folder: &OsString) {
        println!("Writing model to disk");

        let source = std::path::PathBuf::from(self.source_file.clone());
        let source_folder = source.parent().expect("File doesnt have parent path");
        let filename = source.file_name().expect("No filename");

        let dest_folder = std::path::PathBuf::from(folder);
        let dest = dest_folder.clone().join(filename);

        let mut dest_mtl = dest.clone();
        dest_mtl.set_extension("mtl");

        let out_obj_file = File::create(dest).expect("Unable to create file");
        let mut out_obj_writer = BufWriter::new(out_obj_file);

        write_header(&mut out_obj_writer);

        writeln!(
            out_obj_writer,
            "mtllib {}",
            dest_mtl.file_name().unwrap().to_string_lossy()
        )
        .expect("Failed to write mesh");
        writeln!(out_obj_writer).expect("Failed to write mesh");

        let mut vertices = vec![];
        let mut uvs: Vec<Vec2> = vec![];
        let mut normals: Vec<Vec3> = vec![];

        for mesh in &self.meshes {
            vertices.extend_from_slice(&mesh.mesh.positions.to_f32());

            if let Some(mesh_uvs) = &mesh.mesh.uvs {
                uvs.extend_from_slice(mesh_uvs);
            }

            if let Some(mesh_normals) = &mesh.mesh.normals {
                normals.extend_from_slice(mesh_normals);
            }
        }

        for vertex in vertices.iter() {
            writeln!(
                out_obj_writer,
                "v {:.15} {:.15} {:.15}",
                vertex.x, vertex.y, vertex.z
            )
            .expect("Failed to write mesh");
        }

        for uv in uvs.iter() {
            writeln!(out_obj_writer, "vt {:.15} {:.15}", uv.x, uv.y).expect("Failed to write mesh");
        }

        for normal in normals.iter() {
            writeln!(
                out_obj_writer,
                "vn {:.15} {:.15} {:.15}",
                normal.x, normal.y, normal.z
            )
            .expect("Failed to write mesh");
        }

        let mut written_vertex_cnt = 0;

        for mesh in self.meshes.iter() {
            writeln!(out_obj_writer, "g default").expect("Failed to write mesh");
            writeln!(out_obj_writer, "usemtl {}", mesh.material.name)
                .expect("Failed to write mesh");

            mesh.mesh.for_each_triangle(|i0, i1, i2| {
                writeln!(
                    out_obj_writer,
                    "f {}/{} {}/{} {}/{}",
                    i0 + written_vertex_cnt + 1,
                    i0 + written_vertex_cnt + 1,
                    i1 + written_vertex_cnt + 1,
                    i1 + written_vertex_cnt + 1,
                    i2 + written_vertex_cnt + 1,
                    i2 + written_vertex_cnt + 1
                )
                .expect("Failed to write mesh");
            });

            written_vertex_cnt += mesh.mesh.positions.len();
        }

        out_obj_writer.flush().expect("Failed to write to disk");

        // Write materials
        let materials = self.meshes.iter().map(|m| &m.material).collect::<Vec<_>>();
        write_mtllib(source_folder, dest_folder.as_path(), dest_mtl, &materials);
    }
}

impl WriteToFolder for ModelReference {
    fn write_to_folder(&self, folder: &OsString) {
        let source = std::path::PathBuf::from(self.source_file.clone());
        let source_folder = source.parent().expect("File doesnt have parent path");
        let filename = source.file_name().expect("No filename");

        let mut source_mtl = source.clone();
        source_mtl.set_extension("mtl");

        let dest_folder = std::path::PathBuf::from(folder);
        let dest = dest_folder.clone().join(filename);

        if source_mtl.exists() {
            let mut dest_mtl = dest.clone();
            dest_mtl.set_extension("mtl");
            std::fs::copy(source_mtl, dest_mtl).expect("Failed to copy");
        }

        println!("Copying from: {source:?}, to: {dest:?}");
        std::fs::copy(&source, &dest).expect("Failed to copy");

        for material in &self.materials {
            let textures = vec![
                &material.diffuse_texture,
                &material.ambient_texture,
                &material.dissolve_texture,
                &material.specular_texture,
                &material.normal_texture,
                &material.shininess_texture,
            ];

            for texture_file in textures.into_iter().flatten() {
                copy_texture(
                    texture_file,
                    source_folder,
                    &dest_folder,
                    self.texture_downscale_factor,
                );
            }
        }
    }
}

impl WriteToFolder for OutAsset {
    fn write_to_folder(&self, folder: &OsString) {
        match self {
            OutAsset::Asset(model) => model.write_to_folder(folder),
            OutAsset::AssetRef(model_ref) => model_ref.write_to_folder(folder),
        }
    }
}
