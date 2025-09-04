use std::{
    ffi::OsString,
    sync::{Arc, Mutex, mpsc},
};

use crate::messages::ModelLoadTask;
use crate::{Model, messages};

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
                    let model = Model::try_new_from_file(path.clone(), true)
                        .unwrap_or_else(|_| panic!("Failed loading model from {path:?}"));

                    println!("Successfully loaded model from: {path:?}");
                    tx.send(messages::ModelLoadTaskResponse::Model(
                        messages::ModelContainer {
                            model,
                            asset_type: task.asset_type,
                        },
                    ));
                }
                ModelLoadTask::Terminate => {
                    println!("Model load runner done");
                    tx.send(messages::ModelLoadTaskResponse::Terminated);
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

            if let Some(extension) = p.extension() {
                if extension.eq_ignore_ascii_case("obj") {
                    return Some(p.into_os_string());
                }
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
