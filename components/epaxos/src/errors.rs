use crate::qpaxos::{InvalidRequest, QError, StorageFailure};

quick_error! {
    /// Errors occur when set/get with snapshot
    #[derive(Debug, PartialEq)]
    pub enum ReplicationError {
        Timeout{msec: i64} {
            display("timeout after:{}", msec)
        }

        NotEnoughFastQuorum{
            want: i64,
            got: i64, 
        } {
            display("fast-quorum wants >= {} replies but only {}", want, got)
        }

        NotEnoughQuorum{
            want: i64,
            got: i64, 
        } {
            display("quorum wants >= {} replies but only {}", want, got)
        }
    }
}
