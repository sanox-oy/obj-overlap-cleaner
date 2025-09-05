use std::{
    ffi::OsString,
    sync::{Arc, Mutex, RwLock, mpsc},
    thread,
    time::Instant,
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
            None => return,
        };

        drop(files);

        let hq_asset = Model::try_new_from_file(hq_asset_path.clone(), false, true, 1).unwrap();

        let hq_asset_name = hq_asset.source_file.clone();
        let start_time = Instant::now();

        println!(
            "Starting to process hq-asset {:?} against normal assets.",
            hq_asset_name
        );

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

        let duration = (Instant::now() - start_time).as_millis();

        println!("Processed hq-asset: {:?} in {} ms", hq_asset_name, duration);

        let hq_asset_ref = ModelReference::from_model(hq_asset, 1);
        let mut write_hq_asset_ref_lock = write_hq_asset_ref.lock().unwrap();
        write_hq_asset_ref_lock.push(hq_asset_ref);
    }
}

fn mark_and_delete_vertices_worker(
    assets: Arc<Mutex<Vec<Model>>>,
    results: Arc<Mutex<Vec<OutAsset>>>,
) {
    loop {
        let mut assets_lock = assets.lock().unwrap();

        let mut model = match assets_lock.pop() {
            Some(model) => model,
            None => return,
        };

        drop(assets_lock);

        let start_time = Instant::now();
        let model_file = model.source_file.clone();
        println!("Deleting overlapping vertices for {:?}", model_file);

        model.mark_vertices_to_delete();

        if model.to_be_deleted() {
            continue;
        }

        if !model.modified() {
            let model_ref = ModelReference::from_model(model, 2);
            let mut results_lock = results.lock().unwrap();
            results_lock.push(OutAsset::AssetRef(model_ref));
            continue;
        }

        model.do_delete_vertices();

        let mut results_lock = results.lock().unwrap();
        results_lock.push(OutAsset::Asset(model));

        let duration = (Instant::now() - start_time).as_millis();

        println!(
            "Deleted overlapping vertices for {:?} in {} msec",
            model_file, duration
        );
    }
}

fn write_to_folder_worker(out_assets: Arc<Mutex<Vec<OutAsset>>>, dest_folder: &OsString) {
    loop {
        let out_asset = {
            let mut lock = out_assets.lock().unwrap();

            match lock.pop() {
                Some(out_asset) => out_asset,
                None => return,
            }
        };
        out_asset.write_to_folder(dest_folder);
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

        crate::io::scan_folder_and_create_tasks(&normal_asset_folder, &tx_task);

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

    pub fn mark_and_delete_vertices(&mut self) {
        let mut models = Vec::new();
        let results: Arc<Mutex<Vec<OutAsset>>> = Arc::new(Mutex::new(Vec::new()));

        let normal_assets = Arc::try_unwrap(std::mem::take(&mut self.normal_assets)).unwrap();

        for model_guarded in normal_assets {
            let model = Arc::try_unwrap(model_guarded)
                .expect("Still references")
                .into_inner()
                .unwrap();

            models.push(model);
        }

        let task_queue = Arc::new(Mutex::new(models));
        let mut handles = Vec::new();

        for _ in 0..self.num_threads {
            let task_queue_clone = task_queue.clone();
            let results_clone = results.clone();
            handles.push(thread::spawn(move || {
                mark_and_delete_vertices_worker(task_queue_clone, results_clone);
            }))
        }

        for h in handles {
            h.join().expect("Failed to join thread");
        }

        let mut results_unguarded = Arc::try_unwrap(results).unwrap().into_inner().unwrap();

        self.out_assets.append(&mut results_unguarded);

        println!("Deleted all overlapping vertices");
    }

    pub fn write_to_folder(&mut self, dest: &OsString) {
        println!("Writing results to: {:?}", dest);
        let out_assets = std::mem::take(&mut self.out_assets);

        let mut handles = Vec::new();
        let tasks = Arc::new(Mutex::new(out_assets));

        for _ in 0..self.num_threads {
            let tasks_clone = tasks.clone();
            let dest_clone = dest.clone();
            handles.push(thread::spawn(move || {
                write_to_folder_worker(tasks_clone, &dest_clone);
            }));
        }

        for h in handles {
            h.join().expect("Failed to join thread");
        }
    }
}
