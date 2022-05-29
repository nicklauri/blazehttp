use std::future::Future;
use tokio::task::JoinHandle;

use super::WORKER_INFO;

#[derive(Debug, Clone)]
pub struct WorkerInfo {
    id: usize,
    name: String,
}

impl WorkerInfo {
    pub fn new(id: usize, name: String) -> Self {
        Self { id, name }
    }
}

pub fn set_worker_info(id: usize, name: String) {
    WORKER_INFO.with(|w| {
        *w.borrow_mut() = Some(WorkerInfo::new(id, name));
    });
}
