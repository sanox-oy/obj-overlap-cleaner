use std::{
    ffi::OsString,
    sync::{Arc, Mutex, mpsc},
};

use crate::messages::ModelLoadTask;
use crate::{Model, messages};

pub fn model_load_runner(
    rx: Arc<Mutex<mpsc::Receiver<ModelLoadTask>>>,
    tx: mpsc::Sender<messages::LoadTaskCompleted>,
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
                ModelLoadTask::LoadModel(task) => {
                    let path = task.path;
                    let model = Model::try_new_from_file(path.clone())
                        .unwrap_or_else(|_| panic!("Failed loading model from {path:?}"));

                    println!("Successfully loaded model from: {path:?}");
                    tx.send(messages::LoadTaskCompleted::Model(messages::LoadedModel {
                        model,
                        asset_type: task.asset_type,
                    }));
                }
                ModelLoadTask::Terminate => {
                    println!("Model load runner done");
                    tx.send(messages::LoadTaskCompleted::Terminated);
                    return;
                }
            },
            Err(e) => println!("Error: {e} encountered while waiting for messages"),
        };
    }
}

pub fn scan_folder_and_create_tasks(
    folder: &OsString,
    asset_type: messages::AssetType,
    tx: &mpsc::Sender<crate::messages::ModelLoadTask>,
) {
    let folder = std::path::Path::new(&folder);

    for file in folder.read_dir().unwrap() {
        let p = file.unwrap().path();

        if let Some(extension) = p.extension() {
            if extension.eq_ignore_ascii_case("obj") {
                tx.send(crate::messages::ModelLoadTask::LoadModel(
                    crate::messages::LoadTask {
                        path: p.into_os_string(),
                        asset_type: asset_type.clone(),
                    },
                ))
                .expect("Error while sending task");
            }
        }
    }
}
