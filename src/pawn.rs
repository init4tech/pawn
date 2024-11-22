use crate::{BlockExtractor, DbConnect};
use alloy_primitives::Bytes;

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
pub struct Pawn<Extractor, Connect> {
    extractor: Extractor,
    connect: Connect,

    source: tokio::sync::mpsc::Receiver<Bytes>,
}

impl<Extractor, Connect> Pawn<Extractor, Connect> {
    /// Create a new pawn.
    pub fn new(extractor: Extractor, connect: Connect) -> (Self, PawnHandle) {
        let (sink, source) = tokio::sync::mpsc::channel(100);

        let pawn = Pawn {
            extractor,
            connect,
            source,
        };

        let handle = PawnHandle { sink };

        (pawn, handle)
    }
}

impl<Extractor, Connect> Pawn<Extractor, Connect> {
    /// THIS FUNCTION BLOCKS INDEFINITELY
    pub fn run_until_panic<Ext>(mut self)
    where
        Connect: DbConnect,
        Extractor: BlockExtractor<Ext, <Connect as DbConnect>::Database>,
        Ext: 'static,
    {
        std::thread::scope(|s| {
            s.spawn(|| {
                let rt = tokio::runtime::Builder::new_current_thread().build()?;
                rt.block_on(self.run());
            })
        });
    }

    /// Run the pawn.
    pub async fn run<Ext>(&mut self) -> eyre::Result<()>
    where
        Connect: DbConnect,
        Extractor: BlockExtractor<Ext, <Connect as DbConnect>::Database>,
        Ext: 'static,
    {
        let db = self.connect.connect().map_err(|e| eyre::eyre!("{}", e))?;

        let mut trevm = self.extractor.trevm(db);

        while let Some(notification) = self.source.recv().await {
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
