use tokio::sync::Semaphore;
use tracing::error;

pub(crate) struct CountDownLatch {
    count: u32,
    semaphore: Semaphore,
}

impl CountDownLatch {
    pub(crate) fn new(count: usize) -> Self {
        Self {
            count: count as u32,
            semaphore: Semaphore::new(0),
        }
    }
    pub(crate) async fn wait(&self) {
        if let Err(err) = self.semaphore.acquire_many(self.count).await {
            error!("Failed to wait for semaphore: {err:?}");
        }
    }

    pub(crate) fn countdown(&self) {
        self.semaphore.add_permits(1);
    }
}
