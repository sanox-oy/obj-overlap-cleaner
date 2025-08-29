use clap::Parser;

mod model;

#[derive(Debug, Parser)]
struct Args {
    /// Space separated list of folders containing hq assets
    #[clap(long, value_parser, num_args = 1.., value_delimiter = ' ')]
    hq_asset_folders: Vec<String>,

    /// Folder containing the normal assets
    #[clap(long)]
    normal_asset_folder: String,

    out_folder: String,
}

fn main() {
    let args = Args::parse();

    println!("Running with args: {:?}", args);
}
