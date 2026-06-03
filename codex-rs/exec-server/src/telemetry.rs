use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::Ordering;
use std::time::Duration;

use codex_otel::MetricsClient;
use tracing::warn;

const CONNECTIONS_ACTIVE_METRIC: &str = "exec_server_connections_active";
const CONNECTIONS_ACTIVE_DESCRIPTION: &str = "Number of active exec-server connections.";
const CONNECTIONS_TOTAL_METRIC: &str = "exec_server_connections_total";
const CONNECTIONS_TOTAL_DESCRIPTION: &str = "Total number of accepted exec-server connections.";
const REMOTE_REGISTRATION_TOTAL_METRIC: &str = "exec_server_remote_registration_total";
const REMOTE_REGISTRATION_TOTAL_DESCRIPTION: &str =
    "Total number of remote exec-server registration attempts.";
const REMOTE_REGISTRATION_DURATION_METRIC: &str =
    "exec_server_remote_registration_duration_seconds";
const REMOTE_REGISTRATION_DURATION_DESCRIPTION: &str =
    "Duration of remote exec-server registration attempts in seconds.";
const REMOTE_WEBSOCKET_ACTIVE_METRIC: &str = "exec_server_remote_websocket_active";
const REMOTE_WEBSOCKET_ACTIVE_DESCRIPTION: &str =
    "Number of active remote exec-server WebSocket connections.";
const REMOTE_WEBSOCKET_CONNECT_TOTAL_METRIC: &str = "exec_server_remote_websocket_connect_total";
const REMOTE_WEBSOCKET_CONNECT_TOTAL_DESCRIPTION: &str =
    "Total number of remote exec-server WebSocket connection attempts.";
const REMOTE_WEBSOCKET_CONNECT_DURATION_METRIC: &str =
    "exec_server_remote_websocket_connect_duration_seconds";
const REMOTE_WEBSOCKET_CONNECT_DURATION_DESCRIPTION: &str =
    "Duration of remote exec-server WebSocket connection attempts in seconds.";
const REMOTE_WEBSOCKET_RECONNECTS_METRIC: &str = "exec_server_remote_websocket_reconnects_total";
const REMOTE_WEBSOCKET_RECONNECTS_DESCRIPTION: &str =
    "Total number of remote exec-server WebSocket reconnects.";
const REQUESTS_TOTAL_METRIC: &str = "exec_server_requests_total";
const REQUESTS_TOTAL_DESCRIPTION: &str = "Total number of exec-server requests.";
const REQUEST_DURATION_METRIC: &str = "exec_server_request_duration_seconds";
const REQUEST_DURATION_DESCRIPTION: &str = "Duration of exec-server requests in seconds.";
const PROCESSES_ACTIVE_METRIC: &str = "exec_server_processes_active";
const PROCESSES_ACTIVE_DESCRIPTION: &str = "Number of active exec-server processes.";
const PROCESSES_FINISHED_TOTAL_METRIC: &str = "exec_server_processes_finished_total";
const PROCESSES_FINISHED_TOTAL_DESCRIPTION: &str =
    "Total number of finished exec-server processes.";
const PROCESS_DURATION_METRIC: &str = "exec_server_process_duration_seconds";
const PROCESS_DURATION_DESCRIPTION: &str = "Duration of exec-server processes in seconds.";

pub fn runtime_span() -> tracing::Span {
    tracing::info_span!("codex.exec_server", otel.kind = "internal")
}

#[derive(Clone, Copy)]
pub(crate) enum ConnectionTransport {
    Relay,
    Stdio,
    WebSocket,
}

impl ConnectionTransport {
    fn metric_tag(self) -> &'static str {
        match self {
            Self::Relay => "relay",
            Self::Stdio => "stdio",
            Self::WebSocket => "websocket",
        }
    }
}

#[derive(Clone, Default)]
pub struct ExecServerTelemetry {
    inner: Option<Arc<ExecServerTelemetryInner>>,
}

struct ExecServerTelemetryInner {
    metrics: MetricsClient,
    relay_connections: AtomicI64,
    stdio_connections: AtomicI64,
    websocket_connections: AtomicI64,
    remote_websockets: AtomicI64,
    active_processes: AtomicI64,
}

pub(crate) struct ConnectionMetricGuard {
    telemetry: ExecServerTelemetry,
    transport: ConnectionTransport,
}

pub(crate) struct RemoteWebSocketMetricGuard {
    telemetry: ExecServerTelemetry,
}

impl ExecServerTelemetry {
    pub fn new(metrics: Option<MetricsClient>) -> Self {
        Self {
            inner: metrics.map(|metrics| {
                Arc::new(ExecServerTelemetryInner {
                    metrics,
                    relay_connections: AtomicI64::new(0),
                    stdio_connections: AtomicI64::new(0),
                    websocket_connections: AtomicI64::new(0),
                    remote_websockets: AtomicI64::new(0),
                    active_processes: AtomicI64::new(0),
                })
            }),
        }
    }

    pub(crate) fn connection_started(
        &self,
        transport: ConnectionTransport,
    ) -> ConnectionMetricGuard {
        self.with_inner(|inner| {
            let active = inner
                .connection_counter(transport)
                .fetch_add(1, Ordering::AcqRel)
                + 1;
            inner.gauge(
                CONNECTIONS_ACTIVE_METRIC,
                CONNECTIONS_ACTIVE_DESCRIPTION,
                active,
                &[("transport", transport.metric_tag())],
            );
            inner.counter(
                CONNECTIONS_TOTAL_METRIC,
                CONNECTIONS_TOTAL_DESCRIPTION,
                &[
                    ("transport", transport.metric_tag()),
                    ("result", "accepted"),
                ],
            );
        });
        ConnectionMetricGuard {
            telemetry: self.clone(),
            transport,
        }
    }

    pub(crate) fn remote_registration_completed(&self, result: &'static str, duration: Duration) {
        self.with_inner(|inner| {
            let tags = [("result", result)];
            inner.counter(
                REMOTE_REGISTRATION_TOTAL_METRIC,
                REMOTE_REGISTRATION_TOTAL_DESCRIPTION,
                &tags,
            );
            inner.duration(
                REMOTE_REGISTRATION_DURATION_METRIC,
                REMOTE_REGISTRATION_DURATION_DESCRIPTION,
                duration,
                &tags,
            );
        });
    }

    pub(crate) fn remote_websocket_connected(&self) -> RemoteWebSocketMetricGuard {
        self.with_inner(|inner| {
            let active = inner.remote_websockets.fetch_add(1, Ordering::AcqRel) + 1;
            inner.gauge(
                REMOTE_WEBSOCKET_ACTIVE_METRIC,
                REMOTE_WEBSOCKET_ACTIVE_DESCRIPTION,
                active,
                &[],
            );
        });
        RemoteWebSocketMetricGuard {
            telemetry: self.clone(),
        }
    }

    pub(crate) fn remote_websocket_connect_completed(
        &self,
        result: &'static str,
        duration: Duration,
    ) {
        self.with_inner(|inner| {
            let tags = [("result", result)];
            inner.counter(
                REMOTE_WEBSOCKET_CONNECT_TOTAL_METRIC,
                REMOTE_WEBSOCKET_CONNECT_TOTAL_DESCRIPTION,
                &tags,
            );
            inner.duration(
                REMOTE_WEBSOCKET_CONNECT_DURATION_METRIC,
                REMOTE_WEBSOCKET_CONNECT_DURATION_DESCRIPTION,
                duration,
                &tags,
            );
        });
    }

    pub(crate) fn request_completed(
        &self,
        method: &'static str,
        result: &'static str,
        duration: Duration,
    ) {
        self.with_inner(|inner| {
            let tags = [("method", method), ("result", result)];
            inner.counter(REQUESTS_TOTAL_METRIC, REQUESTS_TOTAL_DESCRIPTION, &tags);
            inner.duration(
                REQUEST_DURATION_METRIC,
                REQUEST_DURATION_DESCRIPTION,
                duration,
                &tags,
            );
        });
    }

    pub(crate) fn process_started(&self) {
        self.with_inner(|inner| {
            let active = inner.active_processes.fetch_add(1, Ordering::AcqRel) + 1;
            inner.gauge(
                PROCESSES_ACTIVE_METRIC,
                PROCESSES_ACTIVE_DESCRIPTION,
                active,
                &[],
            );
        });
    }

    pub(crate) fn process_finished(&self, result: &'static str, duration: Duration) {
        self.with_inner(|inner| {
            let active = inner.active_processes.fetch_sub(1, Ordering::AcqRel) - 1;
            inner.gauge(
                PROCESSES_ACTIVE_METRIC,
                PROCESSES_ACTIVE_DESCRIPTION,
                active,
                &[],
            );
            inner.counter(
                PROCESSES_FINISHED_TOTAL_METRIC,
                PROCESSES_FINISHED_TOTAL_DESCRIPTION,
                &[("result", result)],
            );
            inner.duration(
                PROCESS_DURATION_METRIC,
                PROCESS_DURATION_DESCRIPTION,
                duration,
                &[("result", result)],
            );
        });
    }

    pub(crate) fn remote_websocket_reconnect(&self, reason: &'static str) {
        self.with_inner(|inner| {
            inner.counter(
                REMOTE_WEBSOCKET_RECONNECTS_METRIC,
                REMOTE_WEBSOCKET_RECONNECTS_DESCRIPTION,
                &[("reason", reason)],
            );
        });
    }

    fn connection_finished(&self, transport: ConnectionTransport) {
        self.with_inner(|inner| {
            let active = inner
                .connection_counter(transport)
                .fetch_sub(1, Ordering::AcqRel)
                - 1;
            inner.gauge(
                CONNECTIONS_ACTIVE_METRIC,
                CONNECTIONS_ACTIVE_DESCRIPTION,
                active,
                &[("transport", transport.metric_tag())],
            );
        });
    }

    fn remote_websocket_disconnected(&self) {
        self.with_inner(|inner| {
            let active = inner.remote_websockets.fetch_sub(1, Ordering::AcqRel) - 1;
            inner.gauge(
                REMOTE_WEBSOCKET_ACTIVE_METRIC,
                REMOTE_WEBSOCKET_ACTIVE_DESCRIPTION,
                active,
                &[],
            );
        });
    }

    fn with_inner(&self, emit: impl FnOnce(&ExecServerTelemetryInner)) {
        if let Some(inner) = &self.inner {
            emit(inner);
        }
    }
}

impl Drop for ConnectionMetricGuard {
    fn drop(&mut self) {
        self.telemetry.connection_finished(self.transport);
    }
}

impl Drop for RemoteWebSocketMetricGuard {
    fn drop(&mut self) {
        self.telemetry.remote_websocket_disconnected();
    }
}

impl ExecServerTelemetryInner {
    fn connection_counter(&self, transport: ConnectionTransport) -> &AtomicI64 {
        match transport {
            ConnectionTransport::Relay => &self.relay_connections,
            ConnectionTransport::Stdio => &self.stdio_connections,
            ConnectionTransport::WebSocket => &self.websocket_connections,
        }
    }

    fn counter(&self, name: &str, description: &str, tags: &[(&str, &str)]) {
        if self
            .metrics
            .counter_with_description(name, description, /*inc*/ 1, tags)
            .is_err()
        {
            warn!(metric = name, "failed to emit exec-server counter");
        }
    }

    fn duration(&self, name: &str, description: &str, duration: Duration, tags: &[(&str, &str)]) {
        if self
            .metrics
            .record_duration_seconds_with_description(name, description, duration, tags)
            .is_err()
        {
            warn!(metric = name, "failed to emit exec-server duration");
        }
    }

    fn gauge(&self, name: &str, description: &str, value: i64, tags: &[(&str, &str)]) {
        if self
            .metrics
            .gauge_with_description(name, description, value, tags)
            .is_err()
        {
            warn!(metric = name, "failed to emit exec-server gauge");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use codex_otel::MetricsConfig;
    use opentelemetry::KeyValue;
    use opentelemetry_sdk::metrics::InMemoryMetricExporter;
    use opentelemetry_sdk::metrics::data::AggregatedMetrics;
    use opentelemetry_sdk::metrics::data::Metric;
    use opentelemetry_sdk::metrics::data::MetricData;
    use opentelemetry_sdk::metrics::data::ResourceMetrics;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn emits_bounded_exec_server_metrics() {
        let exporter = InMemoryMetricExporter::default();
        let metrics = codex_otel::MetricsClient::new(MetricsConfig::in_memory(
            "test",
            "codex-exec-server",
            env!("CARGO_PKG_VERSION"),
            exporter.clone(),
        ))
        .expect("metrics");
        let telemetry = ExecServerTelemetry::new(Some(metrics.clone()));

        let connection = telemetry.connection_started(ConnectionTransport::WebSocket);
        telemetry.remote_registration_completed("success", Duration::from_millis(5));
        let remote_websocket = telemetry.remote_websocket_connected();
        telemetry.remote_websocket_connect_completed("success", Duration::from_millis(7));
        telemetry.request_completed("process/start", "success", Duration::from_millis(12));
        telemetry.process_started();
        telemetry.process_finished("success", Duration::from_millis(34));
        telemetry.remote_websocket_reconnect("connect_failed");
        drop(remote_websocket);
        drop(connection);
        metrics.shutdown().expect("shutdown metrics");

        let metrics = latest_metrics(&exporter);
        assert_eq!(
            metric_points(&metrics, "exec_server_connections_total"),
            vec![(
                1.0,
                BTreeMap::from([
                    ("result".to_string(), "accepted".to_string()),
                    ("transport".to_string(), "websocket".to_string()),
                ]),
            )]
        );
        assert_eq!(
            metric_points(&metrics, "exec_server_connections_active"),
            vec![(
                0.0,
                BTreeMap::from([("transport".to_string(), "websocket".to_string())]),
            )]
        );
        assert_eq!(
            metric_points(&metrics, "exec_server_remote_registration_total"),
            vec![(
                1.0,
                BTreeMap::from([("result".to_string(), "success".to_string())]),
            )]
        );
        assert_eq!(
            metric_points(&metrics, "exec_server_remote_websocket_connect_total"),
            vec![(
                1.0,
                BTreeMap::from([("result".to_string(), "success".to_string())]),
            )]
        );
        assert_eq!(
            metric_points(&metrics, "exec_server_remote_websocket_active"),
            vec![(0.0, BTreeMap::new())]
        );
        assert_eq!(
            metric_points(&metrics, "exec_server_requests_total"),
            vec![(
                1.0,
                BTreeMap::from([
                    ("method".to_string(), "process/start".to_string()),
                    ("result".to_string(), "success".to_string()),
                ]),
            )]
        );
        assert_eq!(
            metric_points(&metrics, "exec_server_processes_active"),
            vec![(0.0, BTreeMap::new())]
        );
        assert_eq!(
            metric_points(&metrics, "exec_server_processes_finished_total"),
            vec![(
                1.0,
                BTreeMap::from([("result".to_string(), "success".to_string())]),
            )]
        );
        assert_eq!(
            metric_points(&metrics, "exec_server_remote_websocket_reconnects_total"),
            vec![(
                1.0,
                BTreeMap::from([("reason".to_string(), "connect_failed".to_string())]),
            )]
        );
        assert_eq!(
            histogram_count(&metrics, "exec_server_remote_registration_duration_seconds"),
            1
        );
        assert_eq!(
            histogram_count(
                &metrics,
                "exec_server_remote_websocket_connect_duration_seconds"
            ),
            1
        );
        assert_eq!(
            histogram_count(&metrics, "exec_server_request_duration_seconds"),
            1
        );
        assert_eq!(
            histogram_count(&metrics, "exec_server_process_duration_seconds"),
            1
        );
        assert_eq!(
            histogram_sum(&metrics, "exec_server_request_duration_seconds"),
            0.012
        );
        for (name, description, unit) in [
            (
                "exec_server_connections_active",
                CONNECTIONS_ACTIVE_DESCRIPTION,
                "",
            ),
            (
                "exec_server_connections_total",
                CONNECTIONS_TOTAL_DESCRIPTION,
                "",
            ),
            (
                "exec_server_remote_registration_total",
                REMOTE_REGISTRATION_TOTAL_DESCRIPTION,
                "",
            ),
            (
                "exec_server_remote_registration_duration_seconds",
                REMOTE_REGISTRATION_DURATION_DESCRIPTION,
                "s",
            ),
            (
                "exec_server_remote_websocket_active",
                REMOTE_WEBSOCKET_ACTIVE_DESCRIPTION,
                "",
            ),
            (
                "exec_server_remote_websocket_connect_total",
                REMOTE_WEBSOCKET_CONNECT_TOTAL_DESCRIPTION,
                "",
            ),
            (
                "exec_server_remote_websocket_connect_duration_seconds",
                REMOTE_WEBSOCKET_CONNECT_DURATION_DESCRIPTION,
                "s",
            ),
            (
                "exec_server_remote_websocket_reconnects_total",
                REMOTE_WEBSOCKET_RECONNECTS_DESCRIPTION,
                "",
            ),
            ("exec_server_requests_total", REQUESTS_TOTAL_DESCRIPTION, ""),
            (
                "exec_server_request_duration_seconds",
                REQUEST_DURATION_DESCRIPTION,
                "s",
            ),
            (
                "exec_server_processes_active",
                PROCESSES_ACTIVE_DESCRIPTION,
                "",
            ),
            (
                "exec_server_processes_finished_total",
                PROCESSES_FINISHED_TOTAL_DESCRIPTION,
                "",
            ),
            (
                "exec_server_process_duration_seconds",
                PROCESS_DURATION_DESCRIPTION,
                "s",
            ),
        ] {
            assert_metric_metadata(&metrics, name, description, unit);
        }
    }

    fn latest_metrics(exporter: &InMemoryMetricExporter) -> ResourceMetrics {
        exporter
            .get_finished_metrics()
            .expect("finished metrics")
            .into_iter()
            .last()
            .expect("metrics export")
    }

    fn find_metric<'a>(resource_metrics: &'a ResourceMetrics, name: &str) -> &'a Metric {
        resource_metrics
            .scope_metrics()
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
            .find(|metric| metric.name() == name)
            .unwrap_or_else(|| panic!("metric {name} missing"))
    }

    fn metric_points(
        resource_metrics: &ResourceMetrics,
        name: &str,
    ) -> Vec<(f64, BTreeMap<String, String>)> {
        match find_metric(resource_metrics, name).data() {
            AggregatedMetrics::I64(MetricData::Gauge(gauge)) => gauge
                .data_points()
                .map(|point| (point.value() as f64, attributes_to_map(point.attributes())))
                .collect(),
            AggregatedMetrics::U64(MetricData::Sum(sum)) => sum
                .data_points()
                .map(|point| (point.value() as f64, attributes_to_map(point.attributes())))
                .collect(),
            _ => panic!("unexpected metric data for {name}"),
        }
    }

    fn histogram_count(resource_metrics: &ResourceMetrics, name: &str) -> u64 {
        match find_metric(resource_metrics, name).data() {
            AggregatedMetrics::F64(MetricData::Histogram(histogram)) => histogram
                .data_points()
                .map(opentelemetry_sdk::metrics::data::HistogramDataPoint::count)
                .sum(),
            _ => panic!("unexpected histogram data for {name}"),
        }
    }

    fn histogram_sum(resource_metrics: &ResourceMetrics, name: &str) -> f64 {
        match find_metric(resource_metrics, name).data() {
            AggregatedMetrics::F64(MetricData::Histogram(histogram)) => histogram
                .data_points()
                .map(opentelemetry_sdk::metrics::data::HistogramDataPoint::sum)
                .sum(),
            _ => panic!("unexpected histogram data for {name}"),
        }
    }

    fn assert_metric_metadata(
        resource_metrics: &ResourceMetrics,
        name: &str,
        description: &str,
        unit: &str,
    ) {
        let metric = find_metric(resource_metrics, name);
        assert_eq!(metric.description(), description);
        assert_eq!(metric.unit(), unit);
    }

    fn attributes_to_map<'a>(
        attributes: impl Iterator<Item = &'a KeyValue>,
    ) -> BTreeMap<String, String> {
        attributes
            .map(|attribute| {
                (
                    attribute.key.as_str().to_string(),
                    attribute.value.as_str().to_string(),
                )
            })
            .collect()
    }
}
