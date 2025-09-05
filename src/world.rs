use std::{
    ffi::OsString,
    sync::{Arc, Mutex, RwLock, mpsc},
    thread,
};

use crate::{
    io::WriteToFolder,
    model::{Model, ModelReference, OutAsset},
};

pub struct WorldAssets {
    pub hq_asset_files: Vec<OsString>,
    pub normal_assets: Arc<Vec<Arc<RwLock<Model>>>>,
    out_assets: Vec<OutAsset>,
    num_threads: usize,
}

fn hq_asset_worker(
    hq_asset_files: Arc<Mutex<Vec<OsString>>>,
    normal_assets: Arc<Vec<Arc<RwLock<Model>>>>,
    write_hq_asset_ref: Arc<Mutex<Vec<ModelReference>>>,
) {
    loop {
        let mut files = hq_asset_files.lock().unwrap();

        let hq_asset_path = match files.pop() {
            Some(asset) => asset,
            None => {
                println!("Thread done!");
                return;
            }
        };

        drop(files);

        let hq_asset = Model::try_new_from_file(hq_asset_path.clone(), false, true).unwrap();

        for normal_asset in normal_assets.iter() {
            let asset_clone = normal_asset.clone();
            let asset_read = asset_clone.read().unwrap();

            let mut overlaps: Vec<Vec<usize>> = vec![];

            if asset_read.aabb.intersection(hq_asset.aabb).is_some() {
                for mesh in asset_read.meshes.iter() {
                    let mut mesh_overlaps = vec![];

                    for hq_mesh in hq_asset.meshes.iter() {
                        mesh_overlaps
                            .extend_from_slice(&mesh.calc_overlapping_vertice_idxs(hq_mesh));
                    }
                    overlaps.push(mesh_overlaps);
                }
            }

            let no_overlaps = overlaps.is_empty() || overlaps.iter().all(|o| o.is_empty());

            if !no_overlaps {
                drop(asset_read);
                let mut asset_write = asset_clone.write().unwrap();
                for (idx, overlap) in overlaps.iter().enumerate() {
                    asset_write.meshes[idx]
                        .overlapping_vertice_idxs
                        .extend_from_slice(overlap);
                }
            }
        }

        let hq_asset_ref = ModelReference::from_model(hq_asset, 1);
        let mut write_hq_asset_ref_lock = write_hq_asset_ref.lock().unwrap();
        write_hq_asset_ref_lock.push(hq_asset_ref);
    }
}

impl WorldAssets {
    pub fn new(normal_asset_folder: OsString, hq_asset_folders: Vec<OsString>) -> Self {
        let num_os_threads: usize = match std::thread::available_parallelism() {
            Ok(num_cpus) => num_cpus.into(),
            Err(_) => 1,
        };

        // Create a channel for sending tasks to workers.
        let (tx_task, rx_task) = mpsc::channel::<crate::messages::ModelLoadTask>();
        let receiver_guard_task = Arc::new(Mutex::new(rx_task));

        // Create a channel for workers to send responses.
        let (tx_resp, rx_resp) = mpsc::channel::<crate::messages::ModelLoadTaskResponse>();

        // Load all normal assets to permanent memory
        // Spawn worker threads
        let mut workers = Vec::new();
        for _ in 0..num_os_threads {
            let receiver = receiver_guard_task.clone();
            let sender = tx_resp.clone();
            let w = thread::spawn(move || crate::io::model_load_runner(receiver, sender));
            workers.push(w)
        }
        let mut num_running = num_os_threads;

        crate::io::scan_folder_and_create_tasks(
            &normal_asset_folder,
            crate::messages::AssetType::NormalQuality,
            &tx_task,
        );

        // Create tasks to terminate workers
        for _ in 0..num_os_threads {
            tx_task
                .send(crate::messages::ModelLoadTask::Terminate)
                .expect("Failed to send task");
        }

        let mut normal_assets = vec![];
        let mut hq_asset_files = vec![];

        for hq_asset_folder in hq_asset_folders {
            for f in crate::io::scan_folder_for_objs(&hq_asset_folder) {
                hq_asset_files.push(f);
            }
        }

        // Collect responses
        while num_running > 0 {
            let resp = rx_resp.recv().unwrap();
            match resp {
                crate::messages::ModelLoadTaskResponse::Model(model_resp) => {
                    normal_assets.push(Arc::new(RwLock::new(model_resp.model)));
                }
                crate::messages::ModelLoadTaskResponse::Terminated => num_running -= 1,
            }
        }

        Self {
            hq_asset_files,
            normal_assets: Arc::new(normal_assets),
            out_assets: vec![],
            num_threads: num_os_threads,
        }
    }

    pub fn process_overlaps(&mut self) {
        let process_queue = Arc::new(Mutex::new(self.hq_asset_files.clone()));
        let hq_asset_references: Arc<Mutex<Vec<ModelReference>>> = Arc::new(Mutex::new(Vec::new()));

        let mut workers = vec![];

        for _ in 0..self.num_threads {
            let normal_assets = self.normal_assets.clone();
            let hq_assets = process_queue.clone();
            let hq_asset_references_clone = hq_asset_references.clone();

            workers.push(thread::spawn(move || {
                hq_asset_worker(hq_assets, normal_assets, hq_asset_references_clone)
            }));
        }

        // wait for threads to finish
        for t in workers {
            t.join().expect("Failed to join thread");
        }

        let mut hq_asset_references_lock = hq_asset_references.lock().unwrap();
        self.out_assets
            .extend(hq_asset_references_lock.drain(..).map(OutAsset::AssetRef));

        for hq_asset in self.hq_asset_files.iter() {
            println!("Threads done, {hq_asset:?}");
        }
    }

    pub fn mark_vertices_to_delete(&mut self) {
        for model in self.normal_assets.iter() {
            let mut model_write = model.write().unwrap();
            model_write.mark_vertices_to_delete();
        }
    }

    pub fn do_delete_vertices(&mut self) {
        let normal_assets = Arc::try_unwrap(std::mem::take(&mut self.normal_assets)).unwrap();

        for model_guarded in normal_assets {
            let mut model = Arc::try_unwrap(model_guarded)
                .expect("Still references")
                .into_inner()
                .unwrap();
            if model.to_be_deleted() {
                continue;
            }

            if !model.modified() {
                let model_ref = ModelReference::from_model(model, 2);
                self.out_assets.push(OutAsset::AssetRef(model_ref));
                continue;
            }

            model.do_delete_vertices();
            self.out_assets.push(OutAsset::Asset(model));
        }
    }

    pub fn write_to_folder(&self, dest: &OsString) {
        for out_asset in &self.out_assets {
            out_asset.write_to_folder(dest);
        }
    }
}
