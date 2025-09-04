use clap::Parser;
use std::{ffi::OsString, time::Instant};

mod io;
mod messages;
mod model;
mod world;

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

fn main() {
    let args = Args::parse();
    let start_time = Instant::now();

    println!("Running with args: {args:?}");

    let mut assets =
        world::WorldAssets::new(args.normal_asset_folder, args.hq_asset_folders.clone());

    println!("Finding non-overlapping models");
    assets.process_overlaps();
    assets.mark_vertices_to_delete();

    let duration = (Instant::now() - start_time).as_secs();
    println!("Done in {duration} s");
}
