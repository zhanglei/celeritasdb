#![allow(dead_code)]

use rand;
use std::sync::Arc;

use redis;

use std::env;
use std::fs;
use std::io::Write;
use std::process;
use std::thread::sleep;
use std::time::Duration;
use tempfile;

use std::path::PathBuf;

use redis::RedisResult;

use cele::Server;
use epaxos::conf::ClusterInfo;
use epaxos::Storage;
use storage::MemEngine;

/// InProcContext setup a small cluster of an in-process server and a client.
pub struct InProcContext {
    pub server: Server,
    pub storage: Storage,
    pub client: redis::Client,
}

impl InProcContext {
    pub fn new() -> Self {
        let cluster = "
nodes:
    127.0.0.1:6666:
        api_addr: 127.0.0.1:6379
        replication: 127.0.0.1:6666
groups:
-   range:
    -   a
    -   z
    replicas:
        1: 127.0.0.1:6666
";

        let node_id = "127.0.0.1:6666";

        let sto = MemEngine::new().unwrap();
        let sto = Arc::new(sto);
        let cluster = ClusterInfo::from_str(cluster).unwrap();
        let mut server = Server::new(sto.clone(), cluster, node_id.into());
        server.start();

        let server_port = 6379;
        let addr = redis::ConnectionAddr::Tcp("127.0.0.1".to_string(), server_port);
        let client = redis::Client::open(redis::ConnectionInfo {
            addr: Box::new(addr),
            db: 0,
            passwd: None,
        })
        .unwrap();

        // wait until connected.
        let millisecond = Duration::from_millis(50);
        loop {
            match client.get_connection() {
                Err(err) => {
                    if err.is_connection_refusal() {
                        sleep(millisecond);
                    } else {
                        panic!("Could not connect: {}", err);
                    }
                }
                Ok(_x) => {
                    break;
                }
            }
        }

        InProcContext {
            server,
            storage: sto,
            client,
        }
    }
}

#[derive(PartialEq)]
enum ServerType {
    Tcp,
    Unix,
}

#[derive(Debug)]
pub struct RedisServer {
    pub process: process::Child,
    pub conff: tempfile::NamedTempFile,
    addr: redis::ConnectionAddr,
}

impl ServerType {
    fn get_intended() -> ServerType {
        match env::var("REDISRS_SERVER_TYPE")
            .ok()
            .as_ref()
            .map(|x| &x[..])
        {
            Some("tcp") => ServerType::Tcp,
            Some("unix") => ServerType::Unix,
            _ => {
                ServerType::Tcp
                // panic!("Unknown server type {:?}", val);
            }
        }
    }
}

impl RedisServer {
    pub fn new() -> RedisServer {
        let server_type = ServerType::get_intended();

        let cluster = "
nodes:
    127.0.0.1:6666:
        api_addr: 127.0.0.1:6379
        replication: 127.0.0.1:6666
groups:
-   range:
    -   a
    -   z
    replicas:
        1: 127.0.0.1:6666
";
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(cluster.as_bytes()).unwrap();
        f.as_file().sync_all().unwrap();

        let mut cmd = process::Command::new("./target/debug/cele");

        let addr = match server_type {
            ServerType::Tcp => {
                let server_port = 6379;

                cmd.arg("--cluster")
                    .arg(f.path())
                    .arg("--id")
                    .arg("127.0.0.1:6666");

                redis::ConnectionAddr::Tcp("127.0.0.1".to_string(), server_port)
            }
            ServerType::Unix => {
                let (a, b) = rand::random::<(u64, u64)>();
                let path = format!("/tmp/redis-rs-test-{}-{}.sock", a, b);
                cmd.arg("--port").arg("0").arg("--unixsocket").arg(&path);
                redis::ConnectionAddr::Unix(PathBuf::from(&path))
            }
        };

        let process = cmd.spawn().unwrap();
        RedisServer {
            process,
            conff: f,
            addr,
        }
    }

    pub fn wait(&mut self) {
        self.process.wait().unwrap();
    }

    pub fn get_client_addr(&self) -> &redis::ConnectionAddr {
        &self.addr
    }

    pub fn stop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        if let redis::ConnectionAddr::Unix(ref path) = *self.get_client_addr() {
            fs::remove_file(&path).ok();
        }
    }
}

impl Drop for RedisServer {
    fn drop(&mut self) {
        self.stop()
    }
}

#[derive(Debug)]
pub struct TestContext {
    pub server: RedisServer,
    pub client: redis::Client,
    pub repl_client: redis::Client,
}

impl TestContext {
    pub fn new() -> TestContext {
        let server = RedisServer::new();

        let client = redis::Client::open(redis::ConnectionInfo {
            addr: Box::new(server.get_client_addr().clone()),
            db: 0,
            passwd: None,
        })
        .unwrap();
        let mut con;

        let millisecond = Duration::from_millis(1);
        loop {
            match client.get_connection() {
                Err(err) => {
                    if err.is_connection_refusal() {
                        sleep(millisecond);
                    } else {
                        panic!("Could not connect: {}", err);
                    }
                }
                Ok(x) => {
                    con = x;
                    break;
                }
            }
        }
        redis::cmd("FLUSHDB").execute(&mut con);

        // TODO temp impl, remove these
        let repl_addr = redis::ConnectionAddr::Tcp("127.0.0.1".to_string(), 6666);
        let repl_client = redis::Client::open(redis::ConnectionInfo {
            addr: Box::new(repl_addr),
            db: 0,
            passwd: None,
        })
        .unwrap();
        loop {
            match repl_client.get_connection() {
                Err(err) => {
                    if err.is_connection_refusal() {
                        sleep(millisecond);
                    } else {
                        panic!("Could not connect: {}", err);
                    }
                }
                Ok(_) => {
                    break;
                }
            }
        }

        TestContext {
            server,
            client,
            repl_client,
        }
    }

    pub fn connection(&self) -> redis::Connection {
        self.client.get_connection().unwrap()
    }
    pub fn repl_connection(&self) -> redis::Connection {
        self.repl_client.get_connection().unwrap()
    }

    pub async fn async_connection(&self) -> RedisResult<redis::aio::Connection> {
        self.client.get_async_connection().await
    }

    pub fn stop_server(&mut self) {
        self.server.stop();
    }

    #[cfg(feature = "tokio-rt-core")]
    pub fn multiplexed_async_connection(
        &self,
    ) -> impl Future<Output = RedisResult<redis::aio::MultiplexedConnection>> {
        let client = self.client.clone();
        async move { client.get_multiplexed_tokio_connection().await }
    }
}
