use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::Instant;

pub(crate) struct DebounceLimiter<K> {
    store: RwLock<HashMap<K, Instant>>,
    default_duration: Duration,
}

impl<K> DebounceLimiter<K>
    where
        K: Hash + Eq + PartialEq + Clone,
{
    pub(crate) fn new(_sample_size: usize, _threshold: f64, default_duration: Duration) -> Self {
        // todo: implement sample size and threshold purge logic
        Self {
            store: Default::default(),
            default_duration,
        }
    }

    pub(crate) async fn throttle(&self, event: K) -> bool {
        if rand::random::<f64>() < 0.1 {
            self.purge().await;
        }
        let store = self.store.read().await;
        if let Some(created_at) = store.get(&event) {
            let elapsed = created_at.elapsed();
            if elapsed < self.default_duration {
                return true;
            }
        }
        drop(store);

        let mut store = self.store.write().await;
        match store.entry(event) {
            Entry::Occupied(mut entry) => {
                let elapsed = entry.get().elapsed();
                if elapsed < self.default_duration {
                    true
                } else {
                    entry.insert(Instant::now());
                    false
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(Instant::now());
                false
            }
        }
    }

    async fn purge(&self) {
        self.store.write().await.retain(|_, v| v.elapsed() < self.default_duration);
    }
}
