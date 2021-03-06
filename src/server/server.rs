use std::mem::replace;
use std::sync::Arc;
use std::time::Duration;

use futures::Future;

use tokio;
use tokio::sync::oneshot::{error::TryRecvError, Receiver, Sender};
use tokio::task::JoinHandle;

use tonic;

use epaxos::conf::ClusterInfo;
use epaxos::conf::NodeId;
use epaxos::qpaxos::QPaxosServer;
use epaxos::MyQPaxos;
use epaxos::ServerData;
use epaxos::Storage;

use crate::RedisApi;
use crate::ServerError;

/// Server impl some user protocol such as redis protocol and a replication service.
pub struct Server {
    server_data: Arc<ServerData>,
    stop_txs: Vec<(&'static str, Sender<()>)>,
    join_handle: Vec<JoinHandle<()>>,
}

impl Server {
    pub fn new(sto: Storage, cluster: ClusterInfo, node_id: NodeId) -> Server {
        Server {
            server_data: Arc::new(ServerData::new(sto, cluster, node_id)),
            stop_txs: Vec::new(),
            join_handle: Vec::new(),
        }
    }

    /// Starts api server and repolication server
    ///
    /// # Arguments
    ///
    /// # Examples
    ///
    /// ```norun
    /// start();
    /// ```
    pub fn start(&mut self) {
        let (tx1, rx1) = tokio::sync::oneshot::channel::<()>();
        let (tx2, rx2) = tokio::sync::oneshot::channel::<()>();
        let (tx3, rx3) = tokio::sync::oneshot::channel::<()>();

        let fut = Server::_start_servers(self.server_data.clone(), rx1, rx2);
        let j = tokio::spawn(fut);
        self.join_handle.push(j);

        let fut = Server::_start_replica_exec(self.server_data.clone(), rx3);
        let j = tokio::spawn(fut);
        self.join_handle.push(j);

        self.stop_txs.push(("api", tx1));
        self.stop_txs.push(("replication", tx2));
        self.stop_txs.push(("exec", tx3));
    }

    async fn _start_replica_exec(sd: Arc<ServerData>, mut rx: Receiver<()>) {
        loop {
            let mut exec_count = 0;
            for r in sd.local_replicas.values() {
                match r.execute() {
                    Ok(iids) => {
                        println!(
                            "success to execute instances {:?} for {:?}",
                            iids, r.replica_id
                        );
                        exec_count += iids.len();
                    }
                    Err(e) => {
                        println!("{:?} while execute instances for {:?}", e, r.replica_id);
                        continue;
                    }
                }
            }

            if exec_count == 0 {
                tokio::time::delay_for(Duration::from_millis(10)).await;
            }

            match rx.try_recv() {
                Ok(_) => {
                    println!("exit replcia exec thread with recv stop signal");
                    break;
                }
                Err(e) => match e {
                    TryRecvError::Empty => {}
                    TryRecvError::Closed => {
                        println!("exit replcia exec thread with the sender had been dropped");
                        break;
                    }
                },
            }
        }
    }

    async fn _start_servers<F: Future + Send + 'static>(
        sd: Arc<ServerData>,
        sig_api: F,
        sig_repl: F,
    ) {
        let api_addr = sd.node.api_addr;
        let repl_addr = sd.node.replication;

        let redisapi = RedisApi {
            server_data: sd.clone(),
        };

        let j1 = tokio::spawn(async move {
            let rst = redisapi.serve_with_shutdown(api_addr, sig_api).await;
            println!("RedisApi rst={:?}", rst);
        });

        println!("serving: {}", api_addr);

        // TODO load cluster conf
        let qp = MyQPaxos::default();
        let s = tonic::transport::Server::builder().add_service(QPaxosServer::new(qp));

        let j2 = tokio::spawn(async move {
            println!("repl server spawned");
            let rst = s
                .serve_with_shutdown(repl_addr, async {
                    sig_repl.await;
                })
                .await;
            println!("replication server rst={:?}", rst);
        });

        println!("serving: {}", repl_addr);

        j1.await.unwrap();
        j2.await.unwrap();
    }

    pub fn stop(&mut self) -> Result<(), ServerError> {
        while let Some((name, tx)) = self.stop_txs.pop() {
            tx.send(()).or(Err(ServerError::RxClosed))?;
            println!("{} stop signal sent", name);
        }
        Ok(())
    }

    pub async fn join(&mut self) -> Result<(), ServerError> {
        let js = replace(&mut self.join_handle, Vec::new());
        for j in js {
            j.await.unwrap();
        }
        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
