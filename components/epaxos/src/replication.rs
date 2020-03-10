use crate::qpaxos::*;
use crate::errors::ReplicationError as RErr;

pub struct Replication {
    pub leader_id: ReplicaID,
    pub cluster: &ClusterInfo,
    pub instacne: &Instance, 

    pub n: i64, 
    pub quorum: i64, 
    pub fast_quorum: i64, 
}

impl Replication {

    pub fn replicate(&self, inst: &Instance) ->Result<(), ()> {

        self.write_local_replica(inst);

        for replica_id in self.other_replica_ids() {
            let req = MakeRequest:fast_accept(inst);
            self.send_fast_accept(replica_id, req);
        }

        let rst = self.wait_for_fast_accept();
        match rst {
            Ok(_) => {
                self.async_commit();
                return;
            },
            Err(e) => {
                match e {
                    RErr::Timeout{_} => { },
                    RErr::NotEnoughFastQuorum{_} => { }, 
                    RErr::NotEnoughQuorum{_} => {
                        return Err(e);
                    }, 
                };
            }
        };

        let inst = self.choose_instance_for_accept();

        for replica_id in self.other_replica_ids() {
            let req = MakeRequest:accept(inst);
            self.send_accept(req);
        }

        let rst = self.wait_for_accept();
        match rst {
            Ok(_) => {
                self.async_commit();
                return Ok(());
            },
            Err(e) => {
                return Err(());
            }
        };
    }
}
