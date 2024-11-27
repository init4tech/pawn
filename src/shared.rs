use alloy::consensus::constants::KECCAK_EMPTY;
use alloy_primitives::{Address, Log, B256, U256};
use dashmap::{DashMap, Entry};
use std::{
    ops::Deref,
    sync::{Arc, Mutex},
};
use trevm::revm::{
    db::{AccountState, CacheDB, DatabaseCommit, DatabaseRef, DbAccount},
    primitives::{Account, AccountInfo, Bytecode},
    Database,
};

/// A cache that can be shared between threads and accessed concurrently.
pub struct ConcurrentCache {
    /// Account info where None means it is not existing. Not existing state is needed for Pre TANGERINE forks.
    /// `code` is always `None`, and bytecode can be found in `contracts`.
    pub accounts: DashMap<Address, DbAccount>,
    /// Tracks all contracts by their code hash.
    pub contracts: DashMap<B256, Bytecode>,
    /// All logs that were committed via [DatabaseCommit::commit].
    pub logs: Mutex<Vec<Log>>,
    /// All cached block hashes from the [DatabaseRef].
    pub block_hashes: DashMap<U256, B256>,
}

impl ConcurrentCache {
    /// Inserts the account's code into the cache.
    ///
    /// Accounts objects and code are stored separately in the cache, this will take the code from the account and instead map it to the code hash.
    ///
    /// Note: This will not insert into the underlying external database.
    pub fn insert_contract(&self, account: &mut AccountInfo) {
        if let Some(code) = &account.code {
            if !code.is_empty() {
                if account.code_hash == KECCAK_EMPTY {
                    account.code_hash = code.hash_slow();
                }
                self.contracts
                    .entry(account.code_hash)
                    .or_insert_with(|| code.clone());
            }
        }
        if account.code_hash.is_zero() {
            account.code_hash = KECCAK_EMPTY;
        }
    }

    /// Insert account info but not override storage
    pub fn insert_account_info(&mut self, address: Address, mut info: AccountInfo) {
        self.insert_contract(&mut info);
        self.accounts.entry(address).or_default().info = info;
    }
}

/// Inner for [`Root`].
pub struct RootInner<Db> {
    cache: ConcurrentCache,
    db: Db,
}

impl<Db> AsRef<ConcurrentCache> for RootInner<Db> {
    fn as_ref(&self) -> &ConcurrentCache {
        &self.cache
    }
}

impl<Db> AsMut<ConcurrentCache> for RootInner<Db> {
    fn as_mut(&mut self) -> &mut ConcurrentCache {
        &mut self.cache
    }
}

impl<Db: DatabaseRef> Database for RootInner<Db> {
    type Error = Db::Error;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        let info = match self.cache.accounts.entry(address) {
            Entry::Occupied(entry) => entry.get().info(),
            Entry::Vacant(entry) => entry
                .insert(
                    self.db
                        .basic_ref(address)?
                        .map(|info| DbAccount {
                            info,
                            ..Default::default()
                        })
                        .unwrap_or_else(DbAccount::new_not_existing),
                )
                .info(),
        };
        Ok(info)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        match self.cache.contracts.entry(code_hash) {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(entry) => {
                // if you return code bytes when basic fn is called this function is not needed.
                Ok(entry.insert(self.db.code_by_hash_ref(code_hash)?).clone())
            }
        }
    }

    /// Get the value in an account's storage slot.
    ///
    /// It is assumed that account is already loaded.
    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        match self.cache.accounts.entry(address) {
            Entry::Occupied(mut acc_entry) => {
                let acc_entry = acc_entry.get_mut();
                match acc_entry.storage.entry(index) {
                    std::collections::hash_map::Entry::Occupied(entry) => Ok(*entry.get()),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        if matches!(
                            acc_entry.account_state,
                            AccountState::StorageCleared | AccountState::NotExisting
                        ) {
                            Ok(U256::ZERO)
                        } else {
                            let slot = self.db.storage_ref(address, index)?;
                            entry.insert(slot);
                            Ok(slot)
                        }
                    }
                }
            }
            Entry::Vacant(acc_entry) => {
                // acc needs to be loaded for us to access slots.
                let info = self.db.basic_ref(address)?;
                let (account, value) = if info.is_some() {
                    let value = self.db.storage_ref(address, index)?;
                    let mut account: DbAccount = info.into();
                    account.storage.insert(index, value);
                    (account, value)
                } else {
                    (info.into(), U256::ZERO)
                };
                acc_entry.insert(account);
                Ok(value)
            }
        }
    }

    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        match self.cache.block_hashes.entry(U256::from(number)) {
            Entry::Occupied(entry) => Ok(*entry.get()),
            Entry::Vacant(entry) => {
                let hash = self.db.block_hash_ref(number)?;
                entry.insert(hash);
                Ok(hash)
            }
        }
    }
}

impl<Db: DatabaseRef> DatabaseRef for RootInner<Db> {
    type Error = Db::Error;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        match self.cache.accounts.get(&address) {
            Some(acc) => Ok(acc.info()),
            None => self.db.basic_ref(address),
        }
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        match self.cache.contracts.get(&code_hash) {
            Some(entry) => Ok(entry.clone()),
            None => self.db.code_by_hash_ref(code_hash),
        }
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        match self.cache.accounts.get(&address) {
            Some(acc_entry) => match acc_entry.storage.get(&index) {
                Some(entry) => Ok(*entry),
                None => {
                    if matches!(
                        acc_entry.account_state,
                        AccountState::StorageCleared | AccountState::NotExisting
                    ) {
                        Ok(U256::ZERO)
                    } else {
                        self.db.storage_ref(address, index)
                    }
                }
            },
            None => self.db.storage_ref(address, index),
        }
    }

    fn block_hash_ref(&self, number: u64) -> Result<B256, Self::Error> {
        match self.cache.block_hashes.get(&U256::from(number)) {
            Some(entry) => Ok(*entry),
            None => self.db.block_hash_ref(number),
        }
    }
}

impl<Db> DatabaseCommit for RootInner<Db> {
    fn commit(&mut self, changes: alloy_primitives::map::HashMap<Address, Account>) {
        for (address, mut account) in changes {
            if !account.is_touched() {
                continue;
            }
            if account.is_selfdestructed() {
                let mut db_account = self.cache.accounts.entry(address).or_default();
                db_account.storage.clear();
                db_account.account_state = AccountState::NotExisting;
                db_account.info = AccountInfo::default();
                continue;
            }
            let is_newly_created = account.is_created();
            self.cache.insert_contract(&mut account.info);

            let mut db_account = self.cache.accounts.entry(address).or_default();
            db_account.info = account.info;

            db_account.account_state = if is_newly_created {
                db_account.storage.clear();
                AccountState::StorageCleared
            } else if db_account.account_state.is_storage_cleared() {
                // Preserve old account state if it already exists
                AccountState::StorageCleared
            } else {
                AccountState::Touched
            };
            db_account.storage.extend(
                account
                    .storage
                    .into_iter()
                    .map(|(key, value)| (key, value.present_value())),
            );
        }
    }
}

/// A shared cache that can be concurrently accessed by several children.
pub struct Root<Db> {
    inner: RootInner<Db>,
}

impl<Db> Deref for Root<Db> {
    type Target = RootInner<Db>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Db> Root<Db> {
    /// Create a child, borrowing the inner DB.
    pub fn child(&self) -> Child<'_, Db> {
        Child {
            inner: CacheDB::new(&self.inner),
        }
    }
}

/// A child, with its own cache, that can accumulate changes and send them back
/// to the root.
pub struct Child<'a, Db> {
    inner: CacheDB<&'a RootInner<Db>>,
}

impl<Db: DatabaseRef> Database for Child<'_, Db> {
    type Error = <CacheDB<Arc<RootInner<Db>>> as Database>::Error;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.inner.basic(address)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.inner.code_by_hash(code_hash)
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.inner.storage(address, index)
    }

    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        self.inner.block_hash(number)
    }
}

impl<Db: DatabaseRef> DatabaseRef for Child<'_, Db> {
    type Error = <CacheDB<Arc<RootInner<Db>>> as DatabaseRef>::Error;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.inner.basic_ref(address)
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.inner.code_by_hash_ref(code_hash)
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.inner.storage_ref(address, index)
    }

    fn block_hash_ref(&self, number: u64) -> Result<B256, Self::Error> {
        self.inner.block_hash_ref(number)
    }
}

impl<Db> DatabaseCommit for Child<'_, Db> {
    fn commit(&mut self, changes: alloy_primitives::map::HashMap<Address, Account>) {
        self.inner.commit(changes)
    }
}

// Some code in this file has been reproduced from revm. The appropriate
// license notice is below:

// MIT License
//
// Copyright (c) 2021-2024 draganrakita
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
