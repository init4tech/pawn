use std::marker::PhantomData;
use tokio::sync::{mpsc, oneshot};

/// TODO: what is the exact type of the work result
pub type WorkResult = Result<(), ()>;

/// Struct that captures the net work done by several work requests on a worker.
/// This should include AT LEAST:
/// - per-tx gas usage
/// - events
/// - any other necessary information
/// - bundle state
pub struct TotalWork {}

/// Request for work. Worker should simulate the bundle, and send back
/// a result indicating validity and outcome.
pub struct WorkRequest<T> {
    /// the work to be done.
    work: T,
    /// The result of doing work.
    // TODO: what is the exact type of the response
    rx: oneshot::Receiver<WorkResult>,
}

/// Handle to a worker. Used to issue work (bundles) to a worker
/// and manage their lifecycle.
pub struct WorkerHandle<T> {
    /// Sender fo
    tx: mpsc::Sender<WorkRequest<T>>,

    /// Used to shutdown the worker. Either drop to discard, or send to
    /// get the `TotalWork`
    shutdown: oneshot::Sender<()>,

    /// Used to receive the when accepting
    outcome: oneshot::Receiver<TotalWork>,
}

impl<T> WorkerHandle<T> {
    /// Apply a bundle to the worker's inner state.
    async fn do_work(&self, work: WorkRequest<T>) -> WorkResult {
        todo!()
    }

    /// Accept the accumulated work.
    async fn accept(self) -> TotalWork {
        todo!()
    }

    /// Reject and discard the accumulated work.
    fn reject(self) {
        drop(self);
    }
}

/// Worker contains the following:
/// - receiver for work requests
/// - shutdown channel
///   - if dropped, indicates discard
///   - if triggered, indicates acceptance
/// - outcome channel to send execution results
pub struct Worker<T> {
    rx: mpsc::Receiver<WorkRequest<T>>,
    shutdown: oneshot::Receiver<()>,

    outcome: oneshot::Sender<TotalWork>,
}

/// Spawns workers, by wrapping the root DB in cache DB.
pub struct Factory<Ext, Db> {
    db: Db,
    _pd: PhantomData<fn() -> (Ext, Db)>,
}

impl<Ext, Db> Factory<Ext, Db> {
    /// Instantiate a worker with an empty cache
    fn worker<T>(&self) -> WorkerHandle<T> {
        todo!()
    }

    /// Instantiate a worker with a base state
    fn worker_with<T>(&self, based_on: &TotalWork) -> WorkerHandle<T> {
        todo!()
    }
}
