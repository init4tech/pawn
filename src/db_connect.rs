use trevm::revm::{Database, DatabaseCommit};

/// Trait for types that can be used to connect to a database.
pub trait DbConnect: Send + Sync + 'static {
    type Database: Database + DatabaseCommit;
    type Error: core::error::Error;

    fn connect(&self) -> impl std::future::Future<Output = Result<Self::Database, Self::Error>>;
}
