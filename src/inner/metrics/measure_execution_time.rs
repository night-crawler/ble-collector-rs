use std::future::Future;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use metrics::{histogram, KeyName};
use pin_project_lite::pin_project;
use tracing::Span;

pub(crate) trait Measure: Sized {
    fn measure_execution_time<M>(self, metric: M, span: Span) -> TimeInstrumented<Self>
    where
        M: Into<KeyName>,
    {
        let key_name = metric.into();
        TimeInstrumented {
            inner: ManuallyDrop::new(self),
            started_at: None,
            key_name,
            span,
        }
    }
}

impl<T: Sized> Measure for T {}

pin_project! {
    #[derive(Debug, Clone)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct TimeInstrumented<T> {
        #[pin]
        inner: ManuallyDrop<T>,
        started_at: Option<Instant>,
        key_name: KeyName,
        span: Span,
    }

    impl<T> PinnedDrop for TimeInstrumented<T>  {
        fn drop(this: Pin<&mut Self>) {
            let this = this.project();
            let _enter = this.span.enter();
            let started_at = this.started_at.get_or_insert_with(Instant::now);
            histogram!(this.key_name.clone()).record(started_at.elapsed().as_millis() as f64);
            unsafe { ManuallyDrop::drop(this.inner.get_unchecked_mut()) }
        }
    }
}

impl<T: Future> Future for TimeInstrumented<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let _enter = this.span.enter();

        let inner = unsafe { this.inner.map_unchecked_mut(|v| &mut **v) };
        let started_at = this.started_at.get_or_insert_with(Instant::now);
        let res = inner.poll(cx);

        histogram!(this.key_name.clone()).record(started_at.elapsed().as_millis() as f64);
        res
    }
}
