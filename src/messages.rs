use std::ffi::OsString;

pub struct TaskContainer {
    pub path: OsString,
}

pub enum ModelLoadTask {
    Terminate,
    Task(TaskContainer),
}

pub struct ModelContainer {
    pub model: crate::Model,
}

pub enum ModelLoadTaskResponse {
    Terminated,
    Model(ModelContainer),
}
