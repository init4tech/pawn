use alloy_primitives::U256;
use std::{
    future::Future,
    sync::{Arc, Weak},
};
use tokio::{sync::Semaphore, task::JoinSet};
use trevm::{revm::primitives::ResultAndState, Block, Cfg, EvmFactory, Tx};

/// A trait for extracting transactions from a block.
#[derive(Debug, Clone)]
pub struct EvmCtxInner<Ef, C, B> {
    evm_factory: Ef,
    cfg: C,
    block: B,
}

#[derive(Debug, Clone)]
pub struct EvmCtx<Ef, C, B>(Arc<EvmCtxInner<Ef, C, B>>);

pub struct EvmPool<Ef, C, B> {
    evm: EvmCtx<Ef, C, B>,
}

impl<Ef, C, B> EvmPool<Ef, C, B>
where
    Ef: for<'a> EvmFactory<'a> + Send + 'static,
    C: Cfg + 'static,
    B: Block + 'static,
{
    fn weak_evm(&self) -> Weak<EvmCtxInner<Ef, C, B>> {
        Arc::downgrade(&self.evm.0)
    }

    fn spawn_eval<T, F>(
        &self,
        tx: Weak<T>,
        evaluator: F,
    ) -> tokio::task::JoinHandle<Option<Best<T>>>
    where
        T: Tx + 'static,
        F: Fn(&ResultAndState) -> U256 + Send + Sync + 'static,
    {
        let evm = self.weak_evm();
        tokio::task::spawn_blocking(|| eval_fn(evm, tx, evaluator))
    }
}

fn eval_fn<Ef, C, B, T, F>(
    evm: Weak<EvmCtxInner<Ef, C, B>>,
    tx: Weak<T>,
    evaluator: F,
) -> Option<Best<T>>
where
    Ef: for<'a> EvmFactory<'a> + Send + 'static,
    C: Cfg + 'static,
    B: Block + 'static,
    T: Tx + 'static,
    F: Fn(&ResultAndState) -> U256 + Send + Sync + 'static,
{
    // If none, then simulation is over.
    let evm = evm.upgrade()?;
    // If none, tx can be skipped
    let tx = tx.upgrade()?;

    // If none, then tx errored, and can be skipped.
    let result = evm
        .evm_factory
        .run(&evm.cfg, &evm.block, tx.as_ref())
        .ok()?;

    let score = evaluator(&result);
    Some(Best { tx, result, score })
}

pub struct Best<T, Score: PartialOrd + Ord = U256> {
    pub tx: Arc<T>,
    pub result: ResultAndState,
    pub score: Score,
}

impl<Ef, C, B> EvmPool<Ef, C, B>
where
    Ef: for<'a> EvmFactory<'a> + Send + 'static,
    C: Cfg + 'static,
    B: Block + 'static,
{
    pub fn spawn<T, F>(
        self,
        mut rx: tokio::sync::mpsc::Receiver<Arc<T>>,
        evaluator: F,
        deadline: tokio::time::Instant,
    ) -> tokio::task::JoinHandle<Option<Best<T>>>
    where
        T: Tx + 'static,
        F: Fn(&ResultAndState) -> U256 + Send + Sync + 'static + Clone,
    {
        tokio::spawn(async move {
            let mut futs = JoinSet::new();
            let sleep = tokio::time::sleep_until(deadline);
            tokio::pin!(sleep);

            let mut best: Option<Best<T>> = None;

            loop {
                tokio::select! {
                    biased;
                    _ = &mut sleep => break,
                    tx = rx.recv() => {
                        let tx = match tx {
                            Some(tx) => tx,
                            None => break,
                        };

                        let weak_tx = Arc::downgrade(&tx);
                        let evm = self.weak_evm();
                        let eval = evaluator.clone();
                        futs.spawn_blocking(|| eval_fn(evm, weak_tx, eval));
                    }
                    Some(Ok(Some(candidate))) = futs.join_next() => {
                        if candidate.score > best.as_ref().map(|b| b.score).unwrap_or_default() {
                            best = Some(candidate);
                        }
                    }
                }
            }
            best
        })
    }
}
