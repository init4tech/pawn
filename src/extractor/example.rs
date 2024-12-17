use crate::BlockExtractor;
use alloy::consensus::TxEnvelope;
use trevm::{
    revm::{primitives::EVMError, Database, DatabaseCommit},
    BlockDriver, NoopBlock, NoopCfg, TrevmBuilder,
};

pub struct Error<Db: Database>(EVMError<Db::Error>);

impl<Db> From<EVMError<Db::Error>> for Error<Db>
where
    Db: Database,
{
    fn from(e: EVMError<Db::Error>) -> Self {
        Self(e)
    }
}

impl<Db: Database> core::error::Error for Error<Db> {}

impl<Db: Database> core::fmt::Debug for Error<Db> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "Error")
    }
}

impl<Db: Database> core::fmt::Display for Error<Db> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "Error")
    }
}

pub struct ExampleExtractor {}

impl<Db> BlockExtractor<(), Db> for ExampleExtractor
where
    Db: Database + DatabaseCommit + Send + 'static,
{
    type Driver = Block;

    fn trevm(&self, db: Db) -> trevm::EvmNeedsBlock<'static, (), Db> {
        trevm::revm::EvmBuilder::default()
            .with_db(db)
            .build_trevm()
            .fill_cfg(&NoopCfg)
    }

    fn extract(&mut self, bytes: &[u8]) -> Self::Driver {
        #[allow(clippy::useless_asref)]
        let txs: Vec<TxEnvelope> =
            alloy_rlp::Decodable::decode(&mut bytes.as_ref()).unwrap_or_default();
        Block(txs, NoopBlock)
    }
}

pub struct Block(Vec<TxEnvelope>, NoopBlock);

impl<Ext> BlockDriver<Ext> for Block {
    type Block = NoopBlock;
    type Error<Db: Database> = Error<Db>;

    fn block(&self) -> &Self::Block {
        &NoopBlock
    }

    fn run_txns<'a, Db: Database + DatabaseCommit>(
        &mut self,
        mut trevm: trevm::EvmNeedsTx<'a, Ext, Db>,
    ) -> trevm::RunTxResult<'a, Ext, Db, Self> {
        for tx in self.0.iter() {
            if tx.recover_signer().is_ok() {
                let t = match trevm.run_tx(tx) {
                    Ok(t) => t,
                    Err(e) => {
                        if e.is_transaction_error() {
                            return Ok(e.discard_error());
                        } else {
                            return Err(e.err_into());
                        }
                    }
                };
                (_, trevm) = t.accept();
            }
        }
        Ok(trevm)
    }

    fn post_block<Db: Database + DatabaseCommit>(
        &mut self,
        _trevm: &trevm::EvmNeedsBlock<'_, Ext, Db>,
    ) -> Result<(), Self::Error<Db>> {
        Ok(())
    }
}
