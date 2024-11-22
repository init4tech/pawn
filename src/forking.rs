use std::{fmt, sync::Arc};

use alloy_primitives::{Address, B256, U256};
use trevm::revm::{
    db::{CacheDB, State},
    primitives::{AccountInfo, Bytecode},
    Database, DatabaseCommit, DatabaseRef,
};

pub enum Parent<Db> {
    Root(Db),
    Fork(State<Db>),
}

impl<Db> DatabaseRef for Parent<Db>
where
    Db: DatabaseRef,
{
    type Error = Db::Error;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        match self {
            Parent::Root(inner) => inner.basic_ref(address),
            Parent::Fork(inner) => inner.basic_ref(address),
        }
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        match self {
            Parent::Root(inner) => inner.code_by_hash_ref(code_hash),
            Parent::Fork(inner) => inner.code_by_hash_ref(code_hash),
        }
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        match self {}
    }

    fn block_hash_ref(&self, number: u64) -> Result<B256, Self::Error> {
        match self {}
    }
}

pub struct Children {}
