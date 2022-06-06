mod worker;

use anyhow::{bail, Context, Result};
use async_channel::{unbounded, Receiver};
use std::{
    cell::RefCell,
    net::SocketAddr,
    thread::{self, JoinHandle},
};
use tokio::{
    net::TcpStream,
    runtime::{Builder, Runtime},
    task::LocalSet,
};

use crate::{
    config::SharedConfig,
    proto::{self, Connection},
    util,
};

use self::worker::WorkerInfo;

thread_local! {
    pub static WORKER_INFO: RefCell<Option<WorkerInfo>> = RefCell::new(None);
}

#[derive(Debug)]
pub enum Command {
    ProcessRequest(Connection),
    ProcessResponse,
    Stop,
}

#[derive(Debug)]
pub struct Worker {
    worker_id: usize,
    handle: JoinHandle<Result<()>>,
}

impl Worker {
    pub fn new(rx: Receiver<Command>, config: SharedConfig) -> Worker {
        let worker_id = util::next_id();

        let thread = thread::spawn(move || {
            let worker_name = format!("blazehttp-worker-{}", worker_id);
            let rt = Builder::new_current_thread()
                .enable_all()
                .thread_name(&worker_name)
                .build()
                .context("Build runtime failed")?;

            worker::set_worker_info(worker_id, worker_name);

            let local_set = LocalSet::new();

            let fut = local_set.run_until(async move {
                loop {
                    match rx.recv().await? {
                        Command::ProcessRequest(conn) => {
                            let _ = util::spawn(conn.handle());
                        }
                        Command::ProcessResponse => {
                            bail!("unsupported Command::ProcessResponse")
                        }
                        Command::Stop => break,
                    }
                }

                Ok(())
            });

            rt.block_on(fut);

            Ok(())
        });

        Self {
            worker_id,
            handle: thread,
        }
    }
}
