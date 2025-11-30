//! Blocking adapter for async providers.

#[cfg(feature = "async")]
use super::{AsyncProvider, Provider, ProviderResult, ProviderValue};
#[cfg(feature = "async")]
use std::collections::HashMap;

/// Adapter that wraps an [`AsyncProvider`] to implement sync [`Provider`].
///
/// This uses a tokio runtime handle to block on async operations.
///
/// # Example
///
/// ```rust,ignore
/// use procenv::provider::{BlockingAdapter, AsyncProvider};
/// use tokio::runtime::Handle;
///
/// let async_provider = VaultProvider::new();
/// let sync_provider = BlockingAdapter::new(async_provider, Handle::current());
/// ```
#[cfg(feature = "async")]
pub struct BlockingAdapter<P: AsyncProvider> {
    provider: P,
    runtime: tokio::runtime::Handle,
}

#[cfg(feature = "async")]
impl<P: AsyncProvider> BlockingAdapter<P> {
    /// Creates a new blocking adapter.
    ///
    /// # Arguments
    ///
    /// * `provider` - The async provider to wrap
    /// * `runtime` - A handle to a tokio runtime for blocking
    pub fn new(provider: P, runtime: tokio::runtime::Handle) -> Self {
        Self { provider, runtime }
    }

    /// Creates a blocking adapter using the current runtime.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a tokio runtime context.
    pub fn from_current(provider: P) -> Self {
        Self {
            provider,
            runtime: tokio::runtime::Handle::current(),
        }
    }
}

#[cfg(feature = "async")]
impl<P: AsyncProvider> Provider for BlockingAdapter<P> {
    fn name(&self) -> &str {
        self.provider.name()
    }

    fn get(&self, key: &str) -> ProviderResult<ProviderValue> {
        self.runtime.block_on(self.provider.get(key))
    }

    fn get_many(&self, keys: &[&str]) -> HashMap<String, ProviderResult<ProviderValue>> {
        self.runtime.block_on(self.provider.get_many(keys))
    }

    fn is_available(&self) -> bool {
        self.runtime.block_on(self.provider.is_available())
    }

    fn priority(&self) -> u32 {
        self.provider.priority()
    }
}
