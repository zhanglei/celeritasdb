use crate::qpaxos::*;
use crate::replica::*;
use crate::snapshot::MemEngine;

/// Create an instance with command "set x=y".
/// Use this when only deps are concerned.
/// The initial_deps and deps are all set to the second arg.
macro_rules! foo_inst {
    (($rid:expr, $idx: expr),
     [$(($dep_rid:expr, $dep_idx:expr)),* $(,)*]
    ) => {
        inst!(($rid, $idx), (0, 0, _),
              [("Set", "x", "y")],
              [$(($dep_rid, $dep_idx)),*],
              "withdeps"
        )
    };

    (None,
     [$(($dep_rid:expr, $dep_idx:expr)),* $(,)*]
    ) => {
        Instance {
            instance_id: None,
            ..inst!((0, 0), (0, 0, _),
                      [("Set", "x", "y")],
                      [$(($dep_rid, $dep_idx)),*],
                      "withdeps"
                     )
        }
    };

    (($rid:expr, $idx: expr)
    ) => {
        inst!(($rid, $idx), (0, 0, _),
              [("Set", "x", "y")],
        )
    };
}

fn new_foo_inst(leader_id: i64) -> Instance {
    let mut ii = inst!(
        (leader_id, 1),
        (2, 2, _),
        [("NoOp", "k1", "v1"), ("Get", "k2", "v2")],
        [(1, 10), (2, 20), (3, 30)],
        [(2, 20)]
    );
    ii.last_ballot = Some((1, 2, leader_id).into());
    ii.final_deps = Some(instids![(3, 30)].into());

    ii
}

/// Create a stupid replica with some instances stored.
fn new_foo_replica(replica_id: i64, insts: &[((i64, i64), &Instance)]) -> Replica {
    let mut r = Replica {
        replica_id,
        group_replica_ids: vec![0, 1, 2],
        status: ReplicaStatus::Running,
        peers: vec![],
        conf: ReplicaConf {
            ..Default::default()
        },
        inst_idx: 0,
        latest_cp: (1, 1).into(),
        storage: Box::new(MemEngine::new().unwrap()),
        problem_inst_ids: vec![],
    };

    for (iid, inst) in insts.iter() {
        r.storage.set_obj((*iid).into(), inst).unwrap();
    }

    r
}

macro_rules! test_invalid_req {
    ($replica:expr, $req_t:ident, $handle:path, $cases: expr) => {
        for (cmn, etuple) in $cases.clone() {
            let req = $req_t {
                cmn,
                ..Default::default()
            };
            let repl = $handle($replica, &req);
            let err = repl.err.unwrap();
            assert_eq!(
                QError {
                    req: Some(etuple.into()),
                    ..Default::default()
                },
                err
            );
        }
    };
}

#[test]
fn test_handle_xxx_request_invalid() {
    let replica_id = 2;
    let mut replica = new_foo_replica(replica_id, &vec![]);

    let cases: Vec<(Option<RequestCommon>, (&str, &str, &str))> = vec![
        (None, ("cmn", "LackOf", "")),
        (
            Some(RequestCommon {
                to_replica_id: 0,
                ballot: None,
                instance_id: None,
            }),
            ("cmn.to_replica_id", "NotFound", "0; my replica_id: 2"),
        ),
        (
            Some(RequestCommon {
                to_replica_id: replica_id,
                ballot: None,
                instance_id: None,
            }),
            ("cmn.ballot", "LackOf", ""),
        ),
        (
            Some(RequestCommon {
                to_replica_id: replica_id,
                ballot: Some((0, 0, 1).into()),
                instance_id: None,
            }),
            ("cmn.instance_id", "LackOf", ""),
        ),
    ];

    test_invalid_req!(&mut replica, AcceptRequest, Replica::handle_accept, cases);
    test_invalid_req!(&mut replica, CommitRequest, Replica::handle_commit, cases);
}

#[test]
#[should_panic(expected = "local_inst.deps is unexpected to be None")]
fn test_handle_fast_accept_request_panic_local_deps_none() {
    let inst = foo_inst!((0, 0));
    let req_inst = foo_inst!((1, 0), [(0, 0)]);

    _handle_fast_accept_request((0, 0), inst, req_inst);
}

#[test]
#[should_panic(expected = "local_inst.instance_id is unexpected to be None")]
fn test_handle_fast_accept_request_panic_local_instance_id_none() {
    let inst = foo_inst!(None, [(2, 0)]);
    let req_inst = foo_inst!((1, 0), [(0, 0)]);

    _handle_fast_accept_request((0, 0), inst, req_inst);
}

fn _handle_fast_accept_request(iid: (i64, i64), inst: Instance, req_inst: Instance) {
    let mut replica = new_foo_replica(1, &[(iid, &inst)]);

    let req = MakeRequest::fast_accept(1, &req_inst, &vec![false]);
    replica.handle_fast_accept(&req);
}

#[test]
fn test_handle_fast_accept_request() {
    let replica_id = 1;
    let mut replica = new_foo_replica(replica_id, &vec![]);

    {
        let mut inst = new_foo_inst(replica_id);
        let iid = inst.instance_id.unwrap();
        let blt = inst.ballot;

        let none = replica.storage.get_instance(iid).unwrap();
        assert_eq!(None, none);

        let deps_committed = vec![false, false, false];
        let req = MakeRequest::fast_accept(replica_id, &inst, &deps_committed);
        let repl = replica.handle_fast_accept(&req);

        inst.deps = inst.initial_deps.clone();

        assert_eq!(None, repl.err);
        assert_eq!(deps_committed, repl.deps_committed);
        assert_eq!(inst.deps, repl.deps);

        _test_repl_cmn_ok(&repl.cmn.unwrap(), iid, blt);

        // get the written instance.
        _test_get_inst(&replica, iid, blt, blt, inst.cmds, None);
    }

    {
        // instance space layout, then a is replicated to R1
        //               .c
        //             /  |
        // d          d   |
        // |          |\ /
        // a          a-b            c
        // x y z      x y z      x y z
        // -----      -----      -----
        // R0         R1         R2

        // below code that new instance is encapsulated in a func
        // instx
        let x_iid = (0, 0).into();
        let cmd1 = ("Set", "key_x", "val_x").into();
        let cmds = vec![cmd1];
        let ballot = (0, 0, 0).into();
        let initial_deps = vec![];

        let mut instx = Instance::of(&cmds[..], ballot, &initial_deps[..]);
        instx.deps = Some([(0, 0), (0, 0), (0, 0)].into());
        instx.instance_id = Some(x_iid);
        replica.storage.set_instance(&instx).unwrap();

        // insty
        let y_iid = (1, 0).into();
        let cmd1 = ("Get", "key_y", "val_y").into();
        let cmds = vec![cmd1];
        let ballot = (0, 0, 1).into();
        let initial_deps = vec![];

        let mut insty = Instance::of(&cmds[..], ballot, &initial_deps[..]);
        insty.deps = Some([(0, 0), (0, 0), (0, 0)].into());
        insty.instance_id = Some(y_iid);
        replica.storage.set_instance(&insty).unwrap();

        // instz
        let z_iid = (2, 0).into();
        let cmd1 = ("Get", "key_z", "val_z").into();
        let cmds = vec![cmd1];
        let ballot = (0, 0, 2).into();
        let initial_deps = vec![];

        let mut instz = Instance::of(&cmds[..], ballot, &initial_deps[..]);
        instz.deps = Some([(0, 0), (0, 0), (0, 0)].into());
        instz.instance_id = Some(z_iid);
        replica.storage.set_instance(&instz).unwrap();

        // instb
        let b_iid = (1, 1).into();
        let cmd1 = ("Get", "key_b", "val_b").into();
        let cmds = vec![cmd1];
        let ballot = (0, 0, 1).into();
        let initial_deps = vec![];

        let mut instb = Instance::of(&cmds[..], ballot, &initial_deps[..]);
        instb.deps = Some(vec![x_iid, y_iid, z_iid].into());
        instb.instance_id = Some(b_iid);
        instb.committed = true;
        replica.storage.set_instance(&instb).unwrap();

        // insta
        let a_iid = (0, 1).into();
        let cmd1 = ("Get", "key_a", "val_a").into();
        let cmds = vec![cmd1];
        let ballot = (0, 0, 0).into();
        let initial_deps = vec![x_iid, y_iid, z_iid];

        let mut insta = Instance::of(&cmds[..], ballot, &initial_deps[..]);
        insta.deps = Some(vec![x_iid, y_iid, z_iid].into());
        insta.instance_id = Some(a_iid);

        // instd
        let d_iid = (0, 2).into();
        let cmd1 = ("Get", "key_d", "val_d").into();
        let cmds = vec![cmd1];
        let ballot = (0, 0, 0).into();
        let initial_deps = vec![];

        let mut instd = Instance::of(&cmds[..], ballot, &initial_deps[..]);
        instd.deps = Some(vec![a_iid, b_iid, z_iid].into());
        instd.instance_id = Some(d_iid);
        replica.storage.set_instance(&instd).unwrap();

        // instc
        let c_iid = (2, 3).into();
        let cmd1 = ("Get", "key_z", "val_z").into();
        let cmds = vec![cmd1];
        let ballot = (0, 0, 2).into();
        let initial_deps = vec![];

        let mut instc = Instance::of(&cmds[..], ballot, &initial_deps[..]);
        instc.deps = Some(vec![d_iid, b_iid, z_iid].into());
        instc.instance_id = Some(c_iid);
        replica.storage.set_instance(&instc).unwrap();

        let deps_committed = vec![false, true, false];
        let req = MakeRequest::fast_accept(replica_id, &insta, &deps_committed);
        let repl = replica.handle_fast_accept(&req);

        insta.deps = Some(vec![x_iid, b_iid, z_iid].into());

        assert_eq!(None, repl.err);
        assert_eq!(deps_committed, repl.deps_committed);
        assert_eq!(insta.deps, repl.deps);

        _test_repl_cmn_ok(&repl.cmn.unwrap(), insta.instance_id.unwrap(), insta.ballot);

        // get the written instance.
        _test_get_inst(
            &replica,
            insta.instance_id.unwrap(),
            insta.ballot,
            insta.ballot,
            insta.cmds,
            None,
        );
    }
}

#[test]
fn test_handle_accept_request() {
    let replica_id = 2;
    let inst = new_foo_inst(replica_id);
    let iid = inst.instance_id.unwrap();
    let blt = inst.ballot;
    let fdeps = inst.final_deps.clone();

    let mut replica = new_foo_replica(replica_id, &vec![]);
    let none = replica.storage.get_instance(iid).unwrap();
    assert_eq!(None, none);

    {
        // ok reply with none instance.
        let req = MakeRequest::accept(replica_id, &inst);
        let repl = replica.handle_accept(&req);
        assert_eq!(None, repl.err);
        _test_repl_cmn_ok(&repl.cmn.unwrap(), iid, None);

        // get the written instance.
        _test_get_inst(&replica, iid, blt, None, vec![], fdeps.clone());
    }

    {
        // ok reply when replacing instance. same ballot.
        let req = MakeRequest::accept(replica_id, &inst);
        assert_eq!(
            req.cmn.clone().unwrap().ballot,
            replica.storage.get_instance(iid).unwrap().unwrap().ballot
        );

        let repl = replica.handle_accept(&req);
        assert_eq!(None, repl.err);
        _test_repl_cmn_ok(&repl.cmn.unwrap(), iid, blt);

        // get the accepted instance.
        _test_get_inst(&replica, iid, blt, blt, vec![], fdeps.clone());
    }

    {
        // ok reply but not written because of a higher ballot.
        let req = MakeRequest::accept(replica_id, &inst);

        // make an instance with bigger ballot.
        let mut curr = replica.storage.get_instance(iid).unwrap().unwrap();
        let mut bigger = blt.unwrap();
        bigger.num += 1;
        let bigger = Some(bigger);

        curr.ballot = bigger;
        curr.final_deps = Some(vec![].into());
        replica.storage.set_instance(&curr).unwrap();

        let curr = replica.storage.get_instance(iid).unwrap().unwrap();
        assert!(curr.ballot > blt);

        // accept wont update this instance.
        let repl = replica.handle_accept(&req);
        assert_eq!(None, repl.err);
        _test_repl_cmn_ok(&repl.cmn.unwrap(), iid, bigger);

        // get the intact instance.
        _test_get_inst(&replica, iid, bigger, blt, vec![], Some(vec![].into()));
    }

    // TODO test storage error
}

#[test]
fn test_handle_commit_request() {
    let replica_id = 2;
    let inst = new_foo_inst(replica_id);
    let iid = inst.instance_id.unwrap();
    let blt = inst.ballot;
    let cmds = inst.cmds.clone();
    let fdeps = inst.final_deps.clone();

    let mut replica = new_foo_replica(replica_id, &vec![]);
    let none = replica.storage.get_instance(iid).unwrap();
    assert_eq!(None, none);

    let req = MakeRequest::commit(replica_id, &inst);

    {
        // ok reply with none instance.
        let repl = replica.handle_commit(&req);
        assert_eq!(None, repl.err);
        _test_repl_cmn_ok(&repl.cmn.unwrap(), iid, None);

        // get the committed instance.
        _test_get_inst(&replica, iid, blt, None, cmds.clone(), fdeps.clone());
    }

    {
        // ok reply when replacing instance.
        let repl = replica.handle_commit(&req);
        assert_eq!(None, repl.err);
        _test_repl_cmn_ok(&repl.cmn.unwrap(), iid, blt);

        // get the committed instance.
        _test_get_inst(&replica, iid, blt, blt, cmds.clone(), fdeps.clone());
    }

    // TODO test storage error
}

fn _test_repl_cmn_ok(cmn: &ReplyCommon, iid: InstanceId, last: Option<BallotNum>) {
    assert_eq!(iid, cmn.instance_id.unwrap());
    assert_eq!(last, cmn.last_ballot);
}

fn _test_get_inst(
    replica: &Replica,
    iid: InstanceId,
    blt: Option<BallotNum>,
    last: Option<BallotNum>,
    cmds: Vec<Command>,
    final_deps: Option<InstanceIdVec>,
) {
    let got = replica.storage.get_instance(iid).unwrap().unwrap();
    assert_eq!(iid, got.instance_id.unwrap());
    assert_eq!(blt, got.ballot);
    assert_eq!(last, got.last_ballot);
    assert_eq!(cmds, got.cmds);
    assert_eq!(final_deps, got.final_deps);
}
