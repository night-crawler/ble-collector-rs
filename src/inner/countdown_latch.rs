use log::error;
use tokio::sync::Semaphore;

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

// pub(crate) struct CountDownLatch {
//     counter: AtomicI16,
//     sender: kanal::AsyncSender<()>,
//     receiver: kanal::AsyncReceiver<()>,
// }
//
// pub(crate) struct Token {
//     receiver: kanal::AsyncReceiver<()>,
// }
//
// impl Token {
//     pub(crate) async fn wait(&self) -> Result<(), ReceiveError> {
//         self.receiver.recv().await
//     }
// }
//
// impl CountDownLatch {
//     pub(crate) async fn countdown(&self) -> Result<(), SendError> {
//         let prev = self.counter.fetch_sub(1, Ordering::SeqCst);
//         if prev == 1 {
//             for _ in 0..self.sender.receiver_count() {
//                 self.sender.send(()).await?
//             }
//         }
//         Ok(())
//     }
//
//     pub(crate) async fn get_token(&self) -> Token {
//         Token {
//             receiver: self.receiver.clone(),
//         }
//     }
// }
