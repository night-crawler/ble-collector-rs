use std::sync::Arc;

use dashmap::DashMap;
use metrics::{counter, gauge, histogram, KeyName, SharedString};
use tracing::warn;
use crate::inner::conf::dto::publish::PublishMetricConfigDto;

use crate::inner::metrics::MetricType;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::process::PublishPayload;

pub(crate) struct MetricPublisher {
    registered_metrics: DashMap<Arc<String>, ()>,
}

impl MetricPublisher {
    pub(crate) fn new() -> MetricPublisher {
        Self {
            registered_metrics: Default::default(),
        }
    }

    fn register_metric(&self, metric_conf: &PublishMetricConfigDto) {
        self.registered_metrics
            .entry(metric_conf.name.clone())
            .or_insert_with(move || {
                let recorder = metrics::try_recorder().unwrap();
                let description = metric_conf
                    .description
                    .as_ref()
                    .map(|d| SharedString::from(d.to_string()))
                    .unwrap_or(SharedString::from(""));

                let description = SharedString::from(description);

                let name = KeyName::from(metric_conf.name.to_string());

                match metric_conf.metric_type {
                    MetricType::Counter => recorder.describe_counter(name, None, description),
                    MetricType::Gauge => recorder.describe_gauge(name, None, description),
                    MetricType::Histogram => recorder.describe_histogram(name, None, description),
                };
            });
    }
}

impl PublishPayload for MetricPublisher {
    fn publish(&self, payload: Arc<CharacteristicPayload>) {
        let conf = payload.conf.as_ref();
        let Some(metric_conf) = conf.publish_metrics() else {
            return;
        };

        if !payload.value.is_numeric() {
            warn!(
                "Non-numeric value received for metric {}: {} ({})",
                metric_conf.metric_type, payload.value, payload.fqcn
            );
            return;
        }

        self.register_metric(metric_conf);
        let mut labels = metric_conf.labels();
        labels.extend([
            payload.fqcn.peripheral_label(),
            payload.fqcn.service_label(),
            payload.fqcn.characteristic_label(),
        ]);

        let name = metric_conf.name.to_string();

        match metric_conf.metric_type {
            MetricType::Counter => {
                counter!(name, payload.value.as_u64().unwrap(), labels);
            }
            MetricType::Gauge => {
                gauge!(name, payload.value.as_f64().unwrap(), labels);
            }
            MetricType::Histogram => {
                histogram!(name, payload.value.as_f64().unwrap(), labels);
            }
        }
    }
}
