use std::{
    cell::RefCell,
    iter,
    net::ToSocketAddrs,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};

use anyhow::{Context, Result};
use async_channel::{unbounded, Sender};
use h2::server;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tokio::{
    net::TcpListener,
    runtime::{Builder, Runtime},
};
use tracing::info;

use crate::{
    config::{Config, Scheme, SharedConfig},
    proto::Connection,
    worker::{Command, Worker},
};

#[derive(Debug)]
pub struct Server {
    id: u8,
    runtime: Runtime,
    sender: Sender<Command>,
    config: SharedConfig,
    server_pool_name: String,
}

impl Server {
    pub fn new(config: SharedConfig) -> Result<Server> {
        let (tx, rx) = unbounded();

        let mut runtime_builder = if config.workers > 1 {
            let mut multi_thread_builder = Builder::new_multi_thread();
            multi_thread_builder.worker_threads(config.workers);
            multi_thread_builder
        } else {
            Builder::new_current_thread()
        };

        let server_id = Server::get_id();
        let server_pool_name = get_server_name(&*config);
        let shared_server_name = Arc::new(server_pool_name.clone());

        set_current_server_name(&server_pool_name);

        let runtime = runtime_builder
            .enable_all()
            .thread_name(server_pool_name.clone())
            .thread_name_fn(move || {
                let name = shared_server_name.clone();
                get_next_thread_name(&name, server_id as _)
            })
            .build()
            .context("build Tokio runtime failed")?;

        Ok(Self {
            id: server_id,
            runtime,
            sender: tx,
            config,
            server_pool_name,
        })
    }

    fn get_id() -> u8 {
        static SERVER_ID: AtomicU8 = AtomicU8::new(0);
        SERVER_ID.fetch_add(1, Ordering::Relaxed)
    }

    pub async fn stop(&self) -> Result<()> {
        self.sender.send(Command::Stop).await?;
        Ok(())
    }

    pub async fn serve(mut self) -> Result<()> {
        let (scheme, server_addr) = {
            let addr = format!("{}:{}", self.config.addr, self.config.port);
            let scheme = self.config.scheme;
            (scheme, addr)
        };

        let mut server = TcpListener::bind(&server_addr).await?;

        info!("listen at {}://{}", scheme, server_addr);

        loop {
            match server.accept().await {
                Ok((stream, addr)) => {
                    let conn = Connection::new_h1(addr, stream);
                    self.sender.send(Command::ProcessRequest(conn)).await;
                }
                Err(_) => {}
            }
        }

        Ok(())
    }
}

fn get_server_name(config: &Config) -> String {
    format!("blaze:{}", config.port)
}

fn get_next_thread_name(server_name: &str, server_id: usize) -> String {
    static LAST_WORKER_ID: Lazy<Mutex<Vec<usize>>> = Lazy::new(|| Mutex::new(Vec::new()));

    let mut lock = LAST_WORKER_ID.lock();
    let mut vec = &mut *lock;

    if vec.len() <= server_id {
        let extend_iter = iter::repeat(0).take(server_id - vec.len());

        vec.extend(extend_iter);
    }

    let vec_len = vec.len();
    let mut last_id = vec
        .get_mut(server_id)
        .with_context(|| {
            format!(
                "get_next_thread_name: out of bound server id: has={} request={}",
                vec_len, server_id
            )
        })
        .unwrap();

    *last_id += 1;

    let worker_name = format!("{}-worker:{}", server_name, *last_id - 1);

    set_current_server_name(&server_name);
    set_current_worker_name(&worker_name);

    worker_name
}

thread_local! {
    static CURRENT_SERVER_NAME: RefCell<String> = RefCell::new(String::new());
    static CURRENT_WORKER_NAME: RefCell<String> = RefCell::new(String::new());
}

pub fn current_worker_name() -> String {
    CURRENT_WORKER_NAME.with(|name| name.borrow().clone())
}

pub fn current_server_name() -> String {
    CURRENT_SERVER_NAME.with(|name| name.borrow().clone())
}

fn set_current_server_name(server_name: &str) {
    CURRENT_SERVER_NAME.with(|name| *name.borrow_mut() = server_name.to_string())
}

fn set_current_worker_name(worker_name: &str) {
    CURRENT_WORKER_NAME.with(|name| *name.borrow_mut() = worker_name.to_string())
}
