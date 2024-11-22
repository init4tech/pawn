use std::{convert::Infallible, marker::PhantomData};

use trevm::{
    revm::{
        db::{CacheDB, WrapDatabaseRef},
        primitives::{EVMError, ResultAndState},
        Database, DatabaseCommit, EvmBuilder,
    },
    EvmErrored, EvmNeedsBlock, EvmNeedsCfg, EvmNeedsTx, EvmReady, EvmTransacted, TrevmBuilder,
};

pub struct EvmParts<Ext, Db> {
    db: Db,
    ext: PhantomData<fn() -> Ext>,
}

impl<Ext, Db> EvmParts<Ext, Db>
where
    Ext: Default + Send + Sync + 'static,
    Db: DbConnect,
{
    pub fn new(db: Db) -> Self {
        Self {
            db,
            ext: PhantomData,
        }
    }
}

impl<Ext, Db> DbConnect for EvmParts<Ext, Db>
where
    Ext: Default + Send + Sync + 'static,
    Db: DbConnect,
{
    type Database = Db::Database;
    type Error = Db::Error;

    fn connect(&self) -> Result<Self::Database, Self::Error> {
        self.db.connect()
    }
}

impl<Ext, Db> EvmFactory for EvmParts<Ext, Db>
where
    Ext: Default + Send + Sync + 'static,
    Db: DbConnect,
{
    type Ext = Ext;

    fn create<'a>(&self) -> Result<EvmNeedsCfg<'a, Self::Ext, Self::Database>, Self::Error> {
        let db = self.db.connect()?;
        let evm = EvmBuilder::default()
            .with_external_context(Ext::default())
            .with_db(db)
            .build_trevm();
        Ok(evm)
    }
}

/// Trait for types that can be used to connect to a database.
pub trait DbConnect: Sync + 'static {
    /// The database type returned when connecting.
    type Database: Database + DatabaseCommit;

    /// The error type returned when connecting to the database.
    type Error: core::error::Error;

    /// Connect to the database.
    fn connect(&self) -> Result<Self::Database, Self::Error>;
}

/// Trait for types that can create EVM instances.
pub trait EvmFactory: DbConnect {
    type Ext: Sync + 'static;

    /// Create a new EVM instance with the given database connection and extension
    fn create<'a>(&self) -> Result<EvmNeedsCfg<'a, Self::Ext, Self::Database>, Self::Error>;

    fn create_with_cfg<'a, Cfg>(
        &'a self,
        cfg: &Cfg,
    ) -> Result<EvmNeedsBlock<'a, Self::Ext, Self::Database>, Self::Error>
    where
        Cfg: trevm::Cfg,
    {
        self.create().map(|evm| evm.fill_cfg(cfg))
    }

    fn create_with_block<'a, Cfg, Blk>(
        &'a self,
        cfg: &Cfg,
        block: &Blk,
    ) -> Result<EvmNeedsTx<'a, Self::Ext, Self::Database>, Self::Error>
    where
        Cfg: trevm::Cfg,
        Blk: trevm::Block,
    {
        self.create_with_cfg(cfg).map(|evm| evm.fill_block(block))
    }

    fn create_with_tx<'a, Cfg, Blk, Tx>(
        &'a self,
        cfg: &Cfg,
        block: &Blk,
        tx: &Tx,
    ) -> Result<EvmReady<'a, Self::Ext, Self::Database>, Self::Error>
    where
        Cfg: trevm::Cfg,
        Blk: trevm::Block,
        Tx: trevm::Tx,
    {
        self.create_with_block(cfg, block)
            .map(|evm| evm.fill_tx(tx))
    }

    fn transact<'a, Cfg, Blk, Tx>(
        &'a self,
        cfg: &Cfg,
        block: &Blk,
        tx: &Tx,
    ) -> Result<
        Result<
            EvmTransacted<'a, Self::Ext, Self::Database>,
            EvmErrored<'a, Self::Ext, Self::Database>,
        >,
        Self::Error,
    >
    where
        Cfg: trevm::Cfg,
        Blk: trevm::Block,
        Tx: trevm::Tx,
    {
        let evm = self.create_with_tx(cfg, block, tx)?;
        Ok(evm.run())
    }

    /// High level function to run the EVM with the given configuration, block,
    /// and transaction.
    fn run<Cfg, Blk, Tx>(
        &self,
        cfg: &Cfg,
        block: &Blk,
        tx: &Tx,
    ) -> Result<ResultAndState, EVMError<<Self::Database as Database>::Error>>
    where
        Cfg: trevm::Cfg,
        Blk: trevm::Block,
        Tx: trevm::Tx,
    {
        let trevm = self
            .transact(cfg, block, tx)
            .map_err(|e| EVMError::Custom(format!("{e}")))?;

        match trevm {
            Ok(t) => Ok(t.into_result_and_state()),
            Err(t) => Err(t.into_error()),
        }
    }
}

impl<'a, Ext, Db> DbConnect for trevm::revm::Evm<'a, Ext, Db>
where
    Ext: Sync,
    Db: Database + Sync,
{
    type Database = trevm::revm::db::CacheDB<WrapDatabaseRef<&Db>>;

    type Error = Infallible;

    fn connect(&self) -> Result<Self::Database, Self::Error> {
        todo!()
    }
}
