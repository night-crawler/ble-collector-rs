use metrics::{counter, gauge, KeyName, SharedString, Unit};

pub(crate) enum MetricType {
    Counter,
    Gauge,
    Histogram,
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
        let recorder = metrics::try_recorder().unwrap();
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
        }
    }

    pub(crate) fn increment(&self, value: u64, labels: &'static [(&'static str, &'static str)]) {
        match self.metric_type {
            MetricType::Counter => {
                counter!(self.metric_name, value, labels);
            }
            _ => panic!("Metric type mismatch"),
        }
    }

    pub(crate) fn gauge(&self, value: f64, labels: &'static [(&'static str, &'static str)]) {
        match self.metric_type {
            MetricType::Gauge => {
                gauge!(self.metric_name, value, labels);
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

pub(crate) const PAYLOAD_THROTTLED_COUNT: StaticMetric = StaticMetric {
    metric_name: "collector.payload.throttled.count",
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

pub(crate) const CONNECTED_PERPHERALS: StaticMetric = StaticMetric {
    metric_name: "collector.peripheral.connected.count",
    unit: Unit::Count,
    description: "The number of connected peripherals",
    metric_type: MetricType::Gauge,
};

pub(crate) fn describe_metrics() {
    PAYLOAD_PROCESSED_COUNT.describe();
    PAYLOAD_THROTTLED_COUNT.describe();
    CONNECTIONS_HANDLED.describe();
    CONNECTIONS_DROPPED.describe();
    CONNECTING_ERRORS.describe();
    CONNECTED_PERPHERALS.describe();
}
