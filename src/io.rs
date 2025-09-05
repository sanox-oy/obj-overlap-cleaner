use std::{
    ffi::OsString,
    sync::{Arc, Mutex, mpsc},
};

use image::ImageReader;

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
                    tx.send(messages::ModelLoadTaskResponse::Model(
                        messages::ModelContainer {
                            model,
                            asset_type: task.asset_type,
                        },
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
    asset_type: messages::AssetType,
    tx: &mpsc::Sender<crate::messages::ModelLoadTask>,
) {
    for obj_file in scan_folder_for_objs(folder) {
        tx.send(crate::messages::ModelLoadTask::Task(
            crate::messages::TaskContainer {
                path: obj_file,
                asset_type: asset_type.clone(),
            },
        ))
        .expect("Error while sending task");
    }
}

pub trait WriteToFolder {
    fn write_to_folder(&self, folder: &OsString);
}

impl WriteToFolder for Model {
    fn write_to_folder(&self, folder: &OsString) {}
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

            for texture in textures {
                if let Some(texture_file) = texture {
                    let texture_src = source_folder.join(texture_file.clone());
                    let texture_dst = dest_folder.clone().join(texture_file.clone());
                    if texture_dst.exists() {
                        continue;
                    }

                    if !texture_src.exists() {
                        panic!("Unable to load texture: {texture_src:?}");
                    }

                    if self.texture_downscale_factor == 1 {
                        std::fs::copy(texture_src, texture_dst).expect("Failed to copy texture");
                    } else {
                        let mut img = ImageReader::open(texture_src)
                            .expect("Couldnt open image")
                            .decode()
                            .expect("Couldnt decode image");

                        let resized = img.resize_exact(
                            img.width() / self.texture_downscale_factor,
                            img.height() / self.texture_downscale_factor,
                            image::imageops::FilterType::Triangle,
                        );

                        resized.save(texture_dst).expect("Couldnt save image");
                    }
                }
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
