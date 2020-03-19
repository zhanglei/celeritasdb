#[allow(unused_macros)]
macro_rules! cmds {
    [$(($op:expr, $key:expr, $val:expr)),*] => {
        vec![$(Command::from(($op, $key, $val))),*]
    }
}

#[allow(unused_macros)]
macro_rules! instid {
    ($replica_id:expr, $idx:expr) => {
        InstanceId::from(($replica_id, $idx))
    };
}

#[allow(unused_macros)]
macro_rules! instids {
    [$(($replica_id:expr, $idx:expr)),*] => {
        vec![$(InstanceId::from(($replica_id, $idx))),*]
    }
}

#[allow(unused_macros)]
macro_rules! ballot {
    ($epoch:expr, $num:expr, $replica_id:expr) => {
        BallotNum {
            epoch: $epoch as i32,
            num: $num as i32,
            replica_id: $replica_id as i64,
        }
    };
}

/// Create an instance with:
/// instance_id: (replica_id, idx),
/// ballot: (epoch, num, _). the `_` is a place holder indicating to use replica_is from instance_id.
/// cmds: [("Set", "x", "y")...]
/// initial_deps: [(replica_id, idx)...]
/// deps: [(replica_id, idx)...]; `deps` can be "withdeps" as a flag: clone initial_deps as its value.
///
/// Supported pattern:
/// inst!(instance_id, last_ballot, ballot, cmds, initial_deps, deps, final_deps, committed, executed)
/// inst!(instance_id, ballot, cmds, initial_deps, deps)
/// inst!(instance_id, ballot, cmds, initial_deps, "withdeps")
/// inst!(instance_id, ballot, cmds, initial_deps)
/// inst!(instance_id, ballot, cmds)
#[allow(unused_macros)]
macro_rules! inst {
    // instance_id, ballot, cmds, initial_deps=None
    (($replica_id:expr, $idx:expr),
     ($epoch:expr, $num:expr, _),
     [$( ($op:expr, $key:expr, $val:expr)),*]
     $(,)*
     ) => {
        Instance {
            initial_deps: None,
            ..inst!(($replica_id, $idx), ($epoch, $num, _),
                    [$(($op, $key, $val)),*],
                    [],
            )
        }
    };

    // instance_id, ballot, cmds, initial_deps
    (($replica_id:expr, $idx:expr),
     ($epoch:expr, $num:expr, _),
     [$( ($op:expr, $key:expr, $val:expr)),*],
     [$( ($idep_rid:expr, $idep_idx:expr)),*]
     $(,)*
     ) => {
        Instance {
            instance_id: Some(($replica_id, $idx).into()),
            ballot: Some(($epoch, $num, $replica_id).into()),
            cmds: cmds![$( ($op, $key, $val)),*].into(),
            initial_deps: Some(
                instids![$( ($idep_rid, $idep_idx)),*].into()
            ),
            ..Default::default()
        }
    };

    // instance_id, ballot, cmds, initial_deps, deps=initial_deps
    (($replica_id:expr, $idx:expr),
     ($epoch:expr, $num:expr, _),
     [$( ($op:expr, $key:expr, $val:expr)),*],
     [$( ($idep_rid:expr, $idep_idx:expr)),*],
     "withdeps"
     $(,)*
     ) => {
        Instance {
            instance_id: Some(($replica_id, $idx).into()),
            ballot: Some(($epoch, $num, $replica_id).into()),
            cmds: cmds![$( ($op, $key, $val)),*].into(),
            initial_deps: Some(
                instids![$( ($idep_rid, $idep_idx)),*].into()
            ),
            deps: Some(
                instids![$( ($idep_rid, $idep_idx)),*].into()
            ),
            ..Default::default()
        }
    };

    // instance_id, ballot, cmds, initial_deps, specified deps
    (($replica_id:expr, $idx:expr),
     ($epoch:expr, $num:expr, _),
     [$( ($op:expr, $key:expr, $val:expr)),*],
     [$( ($idep_rid:expr, $idep_idx:expr)),*],
     [$( ($dep_rid:expr, $dep_idx:expr)),*]
     $(,)*
     ) => {
        Instance {
            instance_id: Some(($replica_id, $idx).into()),
            ballot: Some(($epoch, $num, $replica_id).into()),
            cmds: cmds![$( ($op, $key, $val)),*].into(),
            initial_deps: Some(
                instids![$( ($idep_rid, $idep_idx)),*].into()
            ),
            deps: Some(
                instids![$( ($dep_rid, $dep_idx)),*].into()
            ),
            ..Default::default()
        }
    };

    // all arg
    (($replica_id:expr, $idx:expr),
     ($lepoch:expr, $lnum:expr, $lbrid:expr),
     ($epoch:expr, $num:expr, $brid:expr),
     [$( ($op:expr, $key:expr, $val:expr)),*],
     [$( ($idep_rid:expr, $idep_idx:expr)),*],
     [$( ($dep_rid:expr, $dep_idx:expr)),*],
     [$( ($fdep_rid:expr, $fdep_idx:expr)),*],
     $committed:expr,
     $executed:expr
     $(,)*
     ) => {
        Instance {
            instance_id: Some(($replica_id, $idx).into()),
            last_ballot: Some(($lepoch, $lnum, $lbrid).into()),
            ballot: Some(($epoch, $num, $brid).into()),
            cmds: cmds![$( ($op, $key, $val)),*].into(),
            initial_deps: Some(
                instids![$( ($idep_rid, $idep_idx)),*].into()
            ),
            deps: Some(
                instids![$( ($dep_rid, $dep_idx)),*].into()
            ),
            final_deps: Some(
                instids![$( ($fdep_rid, $fdep_idx)),*].into()
            ),
            committed:$committed,
            executed:$executed,
            ..Default::default()
        }
    };
}