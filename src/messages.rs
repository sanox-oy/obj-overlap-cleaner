use std::ffi::OsString;

#[derive(Clone)]
pub enum AssetType {
    HighQuality,
    NormalQuality,
}

pub struct LoadTask {
    pub path: OsString,
    pub asset_type: AssetType,
}

pub enum ModelLoadTask {
    Terminate,
    LoadModel(LoadTask),
}

pub struct LoadedModel {
    pub model: crate::Model,
    pub asset_type: AssetType,
}

pub enum LoadTaskCompleted {
    Terminated,
    Model(LoadedModel),
}
