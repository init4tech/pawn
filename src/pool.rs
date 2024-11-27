use crate::db_connect::EvmFactory;
use alloy_primitives::U256;
use rayon::{prelude::*, ThreadPool};
use trevm::{revm::primitives::ResultAndState, Block, Cfg, Tx};

pub struct EvmPool<EF, C, B> {
    thread_pool: Option<ThreadPool>,
    evm_factory: EF,
    cfg: C,
    block: B,
}

pub struct Best<'a, T, Score: PartialOrd + Ord = U256> {
    pub tx: &'a T,
    pub result: ResultAndState,
    pub score: Score,
}

impl<EF, C, B> EvmPool<EF, C, B>
where
    EF: EvmFactory,
    C: Cfg,
    B: Block,
{
    /// Find the best candidate from a list of candidates.
    pub fn find_best<'a, T, F>(&self, candidates: &'a [T], evaluator: F) -> Best<'a, T, U256>
    where
        C: Cfg + Sync,
        B: Block + Sync,
        T: Tx + Sync,
        F: Fn(&ResultAndState) -> U256 + Send + Sync,
    {
        let op = || {
            candidates
                .par_iter()
                .take(rayon::current_num_threads())
                .filter_map(|tx| {
                    let result = self.evm_factory.run(&self.cfg, &self.block, tx).ok()?;
                    let score = evaluator(&result);
                    Some(Best { tx, result, score })
                })
                .max_by_key(|s| s.score)
        };

        // Run the operation on the locally configured thread pool if any.
        // Run it on the rayon global thread pool otherwise.
        if let Some(ref pool) = self.thread_pool {
            pool.install(op)
        } else {
            op()
        }
        .expect("empty candidate array")
    }
}