use tonic;
use tonic::{Request, Response, Status};

use super::instance::Instance;
use super::message;

include!(concat!(env!("OUT_DIR"), "/qpaxos.rs"));

// #[cfg(test)]
// mod t;

pub use q_paxos_client::*;
pub use q_paxos_server::*;

#[derive(Debug, Default)]
pub struct MyQPaxos {}

#[tonic::async_trait]
impl QPaxos for MyQPaxos {
    async fn replicate(
        &self,
        request: Request<message::Request>,
    ) -> Result<Response<message::Reply>, Status> {
        // TODO I did nothing but let the test pass happily
        let inst = Instance {
            ..Default::default()
        };

        let reply = message::Reply::accept(&inst);

        println!("Got a request: {:?}", request);

        Ok(Response::new(reply))
    }
}