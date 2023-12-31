use metrics::{counter, gauge, histogram, KeyName, SharedString, Unit};
use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::future::Future;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) enum MetricType {
    Counter,
    Gauge,
    Histogram,
}

impl Display for MetricType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricType::Counter => write!(f, "Counter"),
            MetricType::Gauge => write!(f, "Gauge"),
            MetricType::Histogram => write!(f, "Histogram"),
        }
    }
}

pub(crate) struct StaticMetric {
    pub(crate) metric_name: &'static str,
    unit: Unit,
    description: &'static str,
    metric_type: MetricType,
}

impl StaticMetric {
    fn describe(&self) {
        let unit = self.unit;
        metrics::with_recorder(|recorder| {
            match self.metric_type {
                MetricType::Counter => recorder.describe_counter(
                    KeyName::from(self.metric_name),
                    Some(unit),
                    SharedString::from(self.description),
                ),
                MetricType::Gauge => recorder.describe_gauge(
                    KeyName::from(self.metric_name),
                    Some(unit),
                    SharedString::from(self.description),
                ),
                MetricType::Histogram => recorder.describe_histogram(
                    KeyName::from(self.metric_name),
                    Some(unit),
                    SharedString::from(self.description),
                ),
            };
        });
    }

    pub(crate) fn increment(&self) {
        match self.metric_type {
            MetricType::Counter => {
                counter!(self.metric_name).increment(1);
            }
            _ => panic!("Metric type mismatch"),
        }
    }

    pub(crate) fn gauge(&self, value: f64) {
        match self.metric_type {
            MetricType::Gauge => {
                gauge!(self.metric_name).set(value);
            }
            _ => panic!("Metric type mismatch"),
        }
    }
}

pub(crate) const PAYLOAD_PROCESSED_COUNT: StaticMetric = StaticMetric {
    metric_name: "collector.payload.processed.count",
    unit: Unit::Count,
    description: "The number of processed characteristic payloads",
    metric_type: MetricType::Counter,
};

pub(crate) const EVENT_COUNT: StaticMetric = StaticMetric {
    metric_name: "collector.event.count",
    unit: Unit::Count,
    description: "The number of received events",
    metric_type: MetricType::Counter,
};

pub(crate) const EVENT_THROTTLED_COUNT: StaticMetric = StaticMetric {
    metric_name: "collector.event.throttled.count",
    unit: Unit::Count,
    description: "The number of throttled events",
    metric_type: MetricType::Counter,
};

pub(crate) const CONNECTIONS_HANDLED: StaticMetric = StaticMetric {
    metric_name: "collector.connection.handled.count",
    unit: Unit::Count,
    description: "The number of handled connections",
    metric_type: MetricType::Counter,
};

pub(crate) const CONNECTIONS_DROPPED: StaticMetric = StaticMetric {
    metric_name: "collector.connection.dropped.count",
    unit: Unit::Count,
    description: "The number of dropped connections",
    metric_type: MetricType::Counter,
};

pub(crate) const CONNECTING_ERRORS: StaticMetric = StaticMetric {
    metric_name: "collector.connection.error.count",
    unit: Unit::Count,
    description: "The number of connection errors",
    metric_type: MetricType::Counter,
};

pub(crate) const CONNECTED_PERIPHERALS: StaticMetric = StaticMetric {
    metric_name: "collector.peripheral.connected.count",
    unit: Unit::Count,
    description: "The number of connected peripherals",
    metric_type: MetricType::Gauge,
};

pub(crate) const TOTAL_CONNECTING_DURATION: StaticMetric = StaticMetric {
    metric_name: "collector.peripheral.connecting.total.duration",
    unit: Unit::Milliseconds,
    description: "The total time spent connecting peripherals",
    metric_type: MetricType::Histogram,
};

pub(crate) const CONNECTING_DURATION: StaticMetric = StaticMetric {
    metric_name: "collector.peripheral.connecting.duration",
    unit: Unit::Milliseconds,
    description: "The time spent connecting peripheral",
    metric_type: MetricType::Histogram,
};

pub(crate) const CONNECTION_DURATION: StaticMetric = StaticMetric {
    metric_name: "collector.peripheral.connection.duration",
    unit: Unit::Milliseconds,
    description: "The time peripheral stays connected",
    metric_type: MetricType::Histogram,
};

pub(crate) const SERVICE_DISCOVERY_DURATION: StaticMetric = StaticMetric {
    metric_name: "collector.peripheral.discovery.duration",
    unit: Unit::Milliseconds,
    description: "The time spent discovering services",
    metric_type: MetricType::Histogram,
};

impl From<StaticMetric> for KeyName {
    fn from(value: StaticMetric) -> Self {
        KeyName::from(value.metric_name)
    }
}

pub(crate) fn describe_metrics() {
    PAYLOAD_PROCESSED_COUNT.describe();
    EVENT_THROTTLED_COUNT.describe();
    CONNECTIONS_HANDLED.describe();
    CONNECTIONS_DROPPED.describe();
    CONNECTING_ERRORS.describe();
    CONNECTED_PERIPHERALS.describe();
    CONNECTION_DURATION.describe();
    TOTAL_CONNECTING_DURATION.describe();
    CONNECTING_DURATION.describe();
    SERVICE_DISCOVERY_DURATION.describe();
    EVENT_COUNT.describe();
}

pub(crate) trait Measure: Sized {
    fn measure_execution_time<M>(self, metric: M) -> TimeInstrumented<Self>
    where
        M: Into<KeyName>,
    {
        let key_name = metric.into();
        TimeInstrumented {
            inner: ManuallyDrop::new(self),
            started_at: None,
            metric: key_name,
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
        metric: KeyName,
    }

    impl<T> PinnedDrop for TimeInstrumented<T>  {
        fn drop(this: Pin<&mut Self>) {
            let this = this.project();
            let started_at = this.started_at.get_or_insert_with(Instant::now);
            histogram!(this.metric.clone()).record(started_at.elapsed().as_millis() as f64);
            unsafe { ManuallyDrop::drop(this.inner.get_unchecked_mut()) }
        }
    }
}

impl<T: Future> Future for TimeInstrumented<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let inner = unsafe { this.inner.map_unchecked_mut(|v| &mut **v) };
        let started_at = this.started_at.get_or_insert_with(Instant::now);
        let res = inner.poll(cx);

        histogram!(this.metric.clone()).record(started_at.elapsed().as_millis() as f64);
        res
    }
}
