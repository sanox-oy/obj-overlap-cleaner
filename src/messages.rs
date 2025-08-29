use std::ffi::OsString;

#[derive(Clone)]
pub enum AssetType {
    HighQuality,
    NormalQuality,
}

pub struct TaskContainer {
    pub path: OsString,
    pub asset_type: AssetType,
}

pub enum ModelLoadTask {
    Terminate,
    Task(TaskContainer),
}

pub struct ModelContainer {
    pub model: crate::Model,
    pub asset_type: AssetType,
}

pub enum ModelLoadTaskResponse {
    Terminated,
    Model(ModelContainer),
}
