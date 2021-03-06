pub use std::cmp::Ordering;
use std::ops::Index;
use std::ops::{Deref, DerefMut};

// to let user be able to call Phase::try_into() without use this trait
pub use std::convert::TryInto;

use derive_more;
use enum_utils;
use storage::ToKey;

include!(concat!(env!("OUT_DIR"), "/qpaxos.rs"));

#[macro_use]
pub mod macros;

// impl Display for qpaxos data types.
mod display;

pub type InstanceIdx = i64;
pub type ReplicaId = i64;

pub use q_paxos_client::*;
pub use q_paxos_server::*;

pub mod conflict;
pub mod errors;
pub mod quorums;

pub use conflict::*;
pub use errors::*;
pub use macros::*;
pub use quorums::*;

#[cfg(test)]
mod t;

#[cfg(test)]
mod test_display;

#[cfg(test)]
mod test_macros;

#[cfg(test)]
mod test_quorums;

#[cfg(test)]
mod test_command;

#[cfg(test)]
mod test_errors;

#[cfg(test)]
mod test_instance;

pub struct MakeRequest {}

#[derive(Debug, Eq, PartialEq, enum_utils::FromStr)]
pub enum Direction {
    Request,
    Reply,
}

impl From<(&str, &str, &str)> for InvalidRequest {
    fn from(t: (&str, &str, &str)) -> InvalidRequest {
        InvalidRequest {
            field: t.0.into(),
            problem: t.1.into(),
            ctx: t.2.into(),
        }
    }
}

impl From<(&str, &str)> for InvalidRequest {
    fn from(t: (&str, &str)) -> InvalidRequest {
        InvalidRequest {
            field: t.0.into(),
            problem: t.1.into(),
            ctx: "".into(),
        }
    }
}

impl Command {
    pub fn of(op: OpCode, key: &[u8], value: &[u8]) -> Command {
        Command {
            op: op as i32,
            key: key.to_vec(),
            value: value.to_vec(),
        }
    }
}

impl Conflict for Command {
    /// conflict checks if two commands conflict.
    /// Two commands conflict iff: the execution order exchange, the result might be differnt.
    fn conflict(&self, with: &Self) -> bool {
        if self.op == OpCode::NoOp as i32 {
            return false;
        }

        if with.op == OpCode::NoOp as i32 {
            return false;
        }

        if self.op == OpCode::Set as i32 || with.op == OpCode::Set as i32 {
            return self.key == with.key;
        }

        false
    }
}

impl From<(OpCode, &str, &str)> for Command {
    fn from(t: (OpCode, &str, &str)) -> Command {
        Command::of(t.0, &t.1.as_bytes().to_vec(), &t.2.as_bytes().to_vec())
    }
}

impl From<(&str, &str, &str)> for Command {
    fn from(t: (&str, &str, &str)) -> Command {
        Command::of(
            t.0.parse().unwrap(),
            &t.1.as_bytes().to_vec(),
            &t.2.as_bytes().to_vec(),
        )
    }
}

impl ToKey for InstanceId {
    fn to_key(&self) -> Vec<u8> {
        if self.idx < 0 {
            panic!("idx can not be less than 0:{}", self.idx);
        }
        format!("/instance/{:016x}/{:016x}", self.replica_id, self.idx).into_bytes()
    }
}

impl<A: Into<ReplicaId> + Copy, B: Into<i64> + Copy> From<(A, B)> for InstanceId {
    fn from(t: (A, B)) -> InstanceId {
        InstanceId {
            replica_id: t.0.into(),
            idx: t.1.into(),
        }
    }
}

impl<A: Into<ReplicaId> + Copy, B: Into<i64> + Copy> From<&(A, B)> for InstanceId {
    fn from(t: &(A, B)) -> InstanceId {
        InstanceId {
            replica_id: t.0.into(),
            idx: t.1.into(),
        }
    }
}

impl InstanceId {
    pub fn from_key(s: &str) -> Option<InstanceId> {
        let items: Vec<&str> = s.split("/").collect();
        if items[1] == "instance" && items.len() == 4 {
            let replica_id = match u64::from_str_radix(&items[2][..], 16) {
                Ok(v) => v as i64,
                Err(_) => return None,
            };

            let idx = match u64::from_str_radix(&items[3][..], 16) {
                Ok(v) => v as i64,
                Err(_) => return None,
            };

            return Some(InstanceId { replica_id, idx });
        }

        return None;
    }
}

/// Let user use method of Vec<InstanceId> directly.
impl Deref for InstanceIdVec {
    type Target = Vec<InstanceId>;
    fn deref(&self) -> &Self::Target {
        &self.ids
    }
}

/// Let user use method of Vec<InstanceId> directly.
impl DerefMut for InstanceIdVec {
    fn deref_mut(&mut self) -> &mut Vec<InstanceId> {
        &mut self.ids
    }
}

/// Let user use instance_id_vec[replic_id] to retreive an instance_id.
/// It panics if replica_id not found.
/// It returns the first match.
impl Index<ReplicaId> for InstanceIdVec {
    type Output = InstanceId;
    fn index(&self, rid: ReplicaId) -> &Self::Output {
        for inst in self.ids.iter() {
            if inst.replica_id == rid {
                return inst;
            }
        }
        panic!("NotFound instance_id with replica_id={}", rid);
    }
}

impl PartialEq<InstanceId> for InstanceIdVec {
    fn eq(&self, other: &InstanceId) -> bool {
        self.get(other.replica_id) == Some(*other)
    }
}

impl PartialOrd<InstanceId> for InstanceIdVec {
    fn partial_cmp(&self, other: &InstanceId) -> Option<Ordering> {
        let mine = self.get(other.replica_id);
        match mine {
            Some(iid) => Some(iid.cmp(other)),
            None => Some(Ordering::Less),
        }
    }
}

impl InstanceIdVec {
    /// get retreive an instance_id with specified replica_id.
    /// It returns the first match.
    pub fn get(&self, rid: ReplicaId) -> Option<InstanceId> {
        for iid in self.ids.iter() {
            if iid.replica_id == rid {
                return Some(*iid);
            }
        }
        None
    }

    /// set add an instanceId into it or overrides an existing one with the same replicaId.
    /// It returns the index the added InstanceId is, along with an Option of the replaced value.
    pub fn set(&mut self, inst_id: InstanceId) -> (usize, Option<InstanceId>) {
        let mut idx: i64 = -1;
        for (i, iid) in self.ids.iter().enumerate() {
            if iid.replica_id == inst_id.replica_id {
                idx = i as i64;
                break;
            }
        }

        if idx == -1 {
            let l = self.ids.len();
            self.ids.push(inst_id);
            (l, None)
        } else {
            let old = self.ids[idx as usize];
            self.ids[idx as usize] = inst_id;
            (idx as usize, Some(old))
        }
    }
}

impl From<&[InstanceId]> for InstanceIdVec {
    fn from(v: &[InstanceId]) -> InstanceIdVec {
        InstanceIdVec { ids: v.into() }
    }
}

impl<A: Into<ReplicaId> + Copy, B: Into<i64> + Copy> From<&[(A, B)]> for InstanceIdVec {
    fn from(v: &[(A, B)]) -> InstanceIdVec {
        v.iter()
            .map(|x| x.into())
            .collect::<Vec<InstanceId>>()
            .into()
    }
}

impl<A> From<&[A; 0]> for InstanceIdVec {
    fn from(_v: &[A; 0]) -> InstanceIdVec {
        InstanceIdVec { ids: vec![] }
    }
}

impl<A> From<[A; 0]> for InstanceIdVec {
    fn from(_v: [A; 0]) -> InstanceIdVec {
        InstanceIdVec { ids: vec![] }
    }
}

macro_rules! impl_instance_id_vec {
    ($n:expr) => {
        impl<A: Into<ReplicaId> + Copy, B: Into<i64> + Copy> From<&[(A, B); $n]> for InstanceIdVec {
            fn from(v: &[(A, B); $n]) -> InstanceIdVec {
                let q: &[_] = v;
                q.into()
            }
        }

        impl<A: Into<ReplicaId> + Copy, B: Into<i64> + Copy> From<[(A, B); $n]> for InstanceIdVec {
            fn from(v: [(A, B); $n]) -> InstanceIdVec {
                let q: &[_] = &v;
                q.into()
            }
        }
    };
}

impl_instance_id_vec!(1);
impl_instance_id_vec!(2);
impl_instance_id_vec!(3);
impl_instance_id_vec!(4);
impl_instance_id_vec!(5);
impl_instance_id_vec!(6);
impl_instance_id_vec!(7);
impl_instance_id_vec!(8);

impl ToKey for Instance {
    fn to_key(&self) -> Vec<u8> {
        self.instance_id.as_ref().unwrap().to_key()
    }
}

impl Conflict for Instance {
    /// conflict checks if two instances conflict.
    /// Two instances conflict iff: command a from self and another command b from with conflict.
    fn conflict(&self, with: &Self) -> bool {
        for a in self.cmds.iter() {
            for b in with.cmds.iter() {
                if a.conflict(b) {
                    return true;
                }
            }
        }

        false
    }
}

impl Instance {
    pub fn of(cmds: &[Command], ballot: BallotNum, deps: &[InstanceId]) -> Instance {
        Instance {
            cmds: cmds.into(),
            ballot: Some(ballot),
            initial_deps: Some(deps.into()),
            ..Default::default()
        }
    }

    pub fn after(&self, other: &Instance) -> bool {
        let mut great = false;
        // TODO unwrap
        for iid in other.final_deps.as_ref().unwrap().iter() {
            // TODO unwrap
            let myiid = self.final_deps.as_ref().unwrap().get(iid.replica_id);
            let myiid = match myiid {
                Some(v) => v,
                None => continue,
            };

            if myiid.idx < iid.idx {
                return false;
            }
            if myiid.idx > iid.idx {
                great = true;
            }
        }

        great
    }
}

macro_rules! make_req {
    ($to_replica_id:expr, $inst:expr, $phase:expr) => {
        ReplicateRequest {
            to_replica_id: $to_replica_id,
            ballot: $inst.ballot,
            instance_id: $inst.instance_id,
            phase: Some($phase.into()),
        }
    };
}

/// MakeRequest is a protobuf message for all kinds of replication request.
/// See: https://github.com/openacid/celeritasdb/wiki/replication-algo#messages;
///
/// ```ignore
/// // create a `prepare` request for an instance:
/// let req = MakeRequest::prepare(1, some_instance);
/// ```
impl MakeRequest {
    pub fn fast_accept(
        to_replica_id: i64,
        inst: &Instance,
        deps_committed: &[bool],
    ) -> ReplicateRequest {
        let p = FastAcceptRequest {
            cmds: inst.cmds.clone(),
            initial_deps: inst.initial_deps.clone(),
            deps_committed: deps_committed.into(),
        };
        make_req!(to_replica_id, inst, p)
    }

    pub fn accept(to_replica_id: i64, inst: &Instance) -> ReplicateRequest {
        let p = AcceptRequest {
            final_deps: inst.final_deps.clone(),
        };
        make_req!(to_replica_id, inst, p)
    }

    pub fn commit(to_replica_id: i64, inst: &Instance) -> ReplicateRequest {
        let p = CommitRequest {
            cmds: inst.cmds.clone(),
            final_deps: inst.final_deps.clone(),
        };
        make_req!(to_replica_id, inst, p)
    }

    pub fn prepare(to_replica_id: i64, inst: &Instance) -> ReplicateRequest {
        let p = PrepareRequest {};
        make_req!(to_replica_id, inst, p)
    }
}
