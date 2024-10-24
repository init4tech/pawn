use crate::BlockExtractor;
use alloy_primitives::Bytes;
use trevm::revm::{Database, DatabaseCommit};

pub struct PawnHandle {
    sink: tokio::sync::mpsc::Sender<Bytes>,
}

impl PawnHandle {
    /// Run a block.
    pub fn run_block(&self, block: Bytes) {
        let _ = self.sink.blocking_send(block);
    }
}

/// Extract and execute transactions.
pub struct Pawn<Extractor> {
    extractor: Extractor,

    source: tokio::sync::mpsc::Receiver<Bytes>,
}

impl<Extractor> Pawn<Extractor> {
    /// Create a new pawn.
    pub fn new(extractor: Extractor) -> (Self, PawnHandle) {
        let (sink, source) = tokio::sync::mpsc::channel(100);

        let pawn = Pawn { extractor, source };

        let handle = PawnHandle { sink };

        (pawn, handle)
    }
}

impl<Extractor> Pawn<Extractor> {
    pub fn spawn<Ext, Db>(self, db: Db) -> std::thread::JoinHandle<eyre::Result<()>>
    where
        Db: Database + DatabaseCommit + Send + 'static,
        Extractor: BlockExtractor<Ext, Db>,
        Ext: 'static,
    {
        std::thread::spawn(move || self.run(db))
    }

    /// Run the pawn.
    pub fn run<Ext, Db>(mut self, db: Db) -> eyre::Result<()>
    where
        Db: Database + DatabaseCommit + 'static,
        Extractor: BlockExtractor<Ext, Db>,
        Ext: 'static,
    {
        let mut trevm = self.extractor.trevm(db);

        while let Some(notification) = self.source.blocking_recv() {
            let mut driver = self.extractor.extract(&notification);

            trevm = match trevm.drive_block(&mut driver) {
                Ok(t) => t,
                Err(e) => {
                    let err = e.into_error();
                    eyre::bail!(err.to_string());
                }
            };
        }

        Ok(())
    }
}
