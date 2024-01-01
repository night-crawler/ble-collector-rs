use std::fmt::{Display, Formatter};

use metrics::{counter, gauge, KeyName, SharedString, Unit};
use serde::{Deserialize, Serialize};

pub(crate) mod measure_execution_time;

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

impl From<StaticMetric> for KeyName {
    fn from(value: StaticMetric) -> Self {
        KeyName::from(value.metric_name)
    }
}
