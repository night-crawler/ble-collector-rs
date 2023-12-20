use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use tokio::sync::{AcquireError, Mutex, OwnedSemaphorePermit, Semaphore};
use tracing::{error, trace};

#[derive(Default, Debug)]
pub(crate) struct KeyLock<K> {
    store: Arc<Mutex<HashMap<K, Arc<Semaphore>>>>,
}

pub(crate) struct KeyLockGuard<'a, K>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
{
    key_lock: &'a KeyLock<K>,
    _permit: OwnedSemaphorePermit,
    key: Arc<K>,
}

impl<'a, K> Drop for KeyLockGuard<'a, K>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
{
    fn drop(&mut self) {
        let key = self.key.clone();
        let store = self.key_lock.store.clone();
        tokio::spawn(async move {
            let mut store = store.lock().await;
            let Some(value) = store.get(&key) else {
                error!("KeyLockGuard: key not found in store");
                return;
            };
            let used_count = Arc::strong_count(value);
            if used_count == 1 {
                trace!("KeyLockGuard: removing key from store");
                store.remove(&key);
            }
        });
    }
}

impl<K> KeyLock<K>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
{
    pub(crate) async fn lock_for(&self, key: K) -> Result<KeyLockGuard<K>, AcquireError> {
        let mut store = self.store.lock().await;
        let semaphore = store
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Semaphore::new(1)))
            .clone();
        drop(store);

        Ok(KeyLockGuard {
            key_lock: self,
            _permit: semaphore.acquire_owned().await?,
            key: key.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::task::JoinSet;

    #[tokio::test]
    async fn sanity() {
        let l = Arc::new(KeyLock::<usize>::default());
        let l1 = l.clone();
        let l2 = l.clone();

        let mut join_set = JoinSet::new();
        join_set.spawn(async move {
            let guard = l1.lock_for(1).await.unwrap();
            drop(guard);
        });

        join_set.spawn(async move {
            let guard = l2.lock_for(1).await.unwrap();
            drop(guard);
        });

        while let Some(result) = join_set.join_next().await {
            result.unwrap();
        }

        assert!(l.store.lock().await.is_empty());
    }
}
