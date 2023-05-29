//! Time out logic for futures.

use std::future::Future;
use std::ops::ControlFlow;

use thiserror::Error;

/// Timeout related errors.
#[derive(Error, Debug)]
pub enum Error {
    /// A future timed out.
    #[error("The future timed out")]
    Elapsed,
}

/// A sleep strategy to be applied to fallible runs of arbitrary tasks.
#[derive(Debug, Clone)]
pub enum SleepStrategy {
    /// Constant sleep.
    Constant(Duration),
    /// Linear backoff sleep.
    LinearBackoff {
        /// The amount of time added to each consecutive run.
        delta: Duration,
    },
}

impl SleepStrategy {
    /// Sleep and update the `backoff` timeout, if necessary.
    async fn sleep_update(&self, backoff: &mut Duration) {
        match self {
            Self::Constant(sleep_duration) => {
                _ = Delay::new(*sleep_duration).await;
            }
            Self::LinearBackoff { delta } => {
                *backoff += *delta;
                _ = Delay::new(*backoff).await;
            }
        }
    }

    /// Execute a fallible task.
    ///
    /// Different retries will result in a sleep operation,
    /// with the current [`SleepStrategy`].
    pub async fn run<T, F, G>(&self, mut future_gen: G) -> T
    where
        G: FnMut() -> F,
        F: Future<Output = ControlFlow<T>>,
    {
        let mut backoff = Duration::from_secs(0);
        loop {
            let fut = future_gen();
            match fut.await {
                ControlFlow::Continue(()) => {
                    self.sleep_update(&mut backoff).await;
                }
                ControlFlow::Break(ret) => break ret,
            }
        }
    }

    /// Run a time constrained task until the given deadline.
    ///
    /// Different retries will result in a sleep operation,
    /// with the current [`SleepStrategy`].
    #[inline]
    pub async fn timeout<T, F, G>(
        &self,
        deadline: Instant,
        future_gen: G,
    ) -> Result<T, Error>
    where
        G: FnMut() -> F,
        F: Future<Output = ControlFlow<T>>,
    {
        timeout_at(deadline, async move { self.run(future_gen).await })
            .await
            .map_err(|_| Error::Elapsed)
    }
}

#[cfg(target_family = "wasm")]
mod internal {
    use std::future::Future;
    pub use std::time::Duration;

    pub use wasm_timer::Instant;
    use wasm_timer::TryFutureExt;

    /// Timeout a future.
    ///
    /// If a timeout occurs, return [`Err`].
    #[inline]
    pub(super) async fn timeout_at<F: Future>(
        deadline: Instant,
        future: F,
    ) -> Result<F::Output, ()> {
        let run_future = async move {
            let value = future.await;
            Result::<_, std::io::Error>::Ok(value)
        };
        run_future.timeout_at(deadline).await.map_err(|_| ())
    }
}

#[cfg(not(target_family = "wasm"))]
mod internal {
    use std::future::Future;

    use tokio::time::timeout_at as tokio_timeout_at;
    pub use tokio::time::{Duration, Instant};

    /// Timeout a future.
    ///
    /// If a timeout occurs, return [`Err`].
    #[inline]
    pub(super) async fn timeout_at<F: Future>(
        deadline: Instant,
        future: F,
    ) -> Result<F::Output, ()> {
        tokio_timeout_at(deadline, future).await.map_err(|_| ())
    }
}

pub use internal::*;
