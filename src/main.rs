use clap::Parser;
use std::{
    ffi::OsString,
    sync::{Arc, Mutex, mpsc},
    thread,
};

mod io;
mod messages;
mod model;

use model::Model;

#[derive(Debug, Parser)]
struct Args {
    /// Space separated list of folders containing hq assets
    #[clap(long, value_parser, num_args = 1.., value_delimiter = ' ')]
    hq_asset_folders: Vec<OsString>,

    /// Folder containing the normal assets
    #[clap(long)]
    normal_asset_folder: OsString,

    out_folder: String,
}

struct LoadedAssets {
    hq_assets: Vec<Model>,
    normal_assets: Vec<Model>,
}

impl LoadedAssets {
    const EMPTY: Self = Self {
        hq_assets: vec![],
        normal_assets: vec![],
    };
}

fn main() {
    let args = Args::parse();

    let num_os_threads: usize = match std::thread::available_parallelism() {
        Ok(num_cpus) => num_cpus.into(),
        Err(_) => 1,
    };

    // Create a channel for sending tasks to workers.
    let (tx_task, rx_task) = mpsc::channel::<messages::ModelLoadTask>();
    let receiver_guard_task = Arc::new(Mutex::new(rx_task));

    // Create a channel for workers to send responses.
    let (tx_resp, rx_resp) = mpsc::channel::<messages::LoadTaskCompleted>();

    // Spawn worker threads
    let mut workers = Vec::new();
    for _ in 0..num_os_threads {
        let receiver = receiver_guard_task.clone();
        let sender = tx_resp.clone();
        let w = thread::spawn(move || io::model_load_runner(receiver, sender));
        workers.push(w)
    }
    let mut num_running = num_os_threads;

    for hq_folder in &args.hq_asset_folders {
        io::scan_folder_and_create_tasks(hq_folder, messages::AssetType::HighQuality, &tx_task);
    }

    io::scan_folder_and_create_tasks(
        &args.normal_asset_folder,
        messages::AssetType::NormalQuality,
        &tx_task,
    );

    // Create tasks to terminate workers
    for _ in 0..num_os_threads {
        tx_task.send(messages::ModelLoadTask::Terminate);
    }

    let mut assets = LoadedAssets::EMPTY;

    // Collect responses
    while num_running > 0 {
        let resp = rx_resp.recv().unwrap();
        match resp {
            messages::LoadTaskCompleted::Model(model_resp) => {
                match model_resp.asset_type {
                    messages::AssetType::HighQuality => {
                        assets.hq_assets.push(model_resp.model);
                    }
                    messages::AssetType::NormalQuality => {
                        assets.normal_assets.push(model_resp.model);
                    }
                };
            }
            messages::LoadTaskCompleted::Terminated => num_running -= 1,
        }
    }

    // Wait for workers to shut down
    for w in workers {
        w.join().expect("Couldn't join thread");
    }

    println!("Running with args: {args:?}");
}
