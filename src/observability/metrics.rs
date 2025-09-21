//! Metrics collection and export for performance monitoring

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use crate::error::GameResult;

/// Performance metrics for system operations
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Total number of requests processed
    pub requests_total: u64,
    /// Number of requests currently being processed
    pub requests_active: u64,
    /// Average request duration in milliseconds
    pub request_duration_avg: f64,
    /// 95th percentile request duration
    pub request_duration_p95: f64,
    /// Number of errors encountered
    pub errors_total: u64,
    /// System uptime in seconds
    pub uptime_seconds: u64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            requests_total: 0,
            requests_active: 0,
            request_duration_avg: 0.0,
            request_duration_p95: 0.0,
            errors_total: 0,
            uptime_seconds: 0,
        }
    }
}

/// Business-specific metrics for gaming protocol
#[derive(Debug, Clone)]
pub struct BusinessMetrics {
    /// Number of active game sequences
    pub active_sequences: u64,
    /// Total number of completed games
    pub completed_games: u64,
    /// Number of failed/forfeited games
    pub failed_games: u64,
    /// Average game duration in seconds
    pub game_duration_avg: f64,
    /// Total value of tokens processed
    pub tokens_processed: u64,
    /// Number of fraud attempts detected
    pub fraud_attempts: u64,
}

impl Default for BusinessMetrics {
    fn default() -> Self {
        Self {
            active_sequences: 0,
            completed_games: 0,
            failed_games: 0,
            game_duration_avg: 0.0,
            tokens_processed: 0,
            fraud_attempts: 0,
        }
    }
}

/// Counter metric for tracking cumulative values
#[derive(Debug)]
struct Counter {
    value: Arc<Mutex<u64>>,
}

impl Counter {
    fn new() -> Self {
        Self {
            value: Arc::new(Mutex::new(0)),
        }
    }

    fn increment(&self) {
        if let Ok(mut value) = self.value.lock() {
            *value += 1;
        }
    }

    fn add(&self, amount: u64) {
        if let Ok(mut value) = self.value.lock() {
            *value += amount;
        }
    }

    fn get(&self) -> u64 {
        self.value.lock().map(|v| *v).unwrap_or(0)
    }
}

/// Histogram metric for tracking distributions
#[derive(Debug)]
struct Histogram {
    buckets: Arc<Mutex<Vec<f64>>>,
    samples: Arc<Mutex<Vec<f64>>>,
}

impl Histogram {
    fn new() -> Self {
        Self {
            buckets: Arc::new(Mutex::new(vec![
                0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
            ])),
            samples: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn observe(&self, value: f64) {
        if let Ok(mut samples) = self.samples.lock() {
            samples.push(value);
            // Keep only last 1000 samples to prevent memory growth
            if samples.len() > 1000 {
                samples.remove(0);
            }
        }
    }

    fn percentile(&self, p: f64) -> f64 {
        if let Ok(mut samples) = self.samples.lock() {
            if samples.is_empty() {
                return 0.0;
            }
            samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let index = ((p / 100.0) * samples.len() as f64) as usize;
            samples.get(index.min(samples.len() - 1)).copied().unwrap_or(0.0)
        } else {
            0.0
        }
    }

    fn average(&self) -> f64 {
        if let Ok(samples) = self.samples.lock() {
            if samples.is_empty() {
                0.0
            } else {
                samples.iter().sum::<f64>() / samples.len() as f64
            }
        } else {
            0.0
        }
    }
}

/// Main metrics registry for collecting and exporting metrics
#[derive(Debug)]
pub struct MetricsRegistry {
    enabled: bool,
    start_time: SystemTime,
    counters: HashMap<String, Counter>,
    histograms: HashMap<String, Histogram>,
    performance: Arc<Mutex<PerformanceMetrics>>,
    business: Arc<Mutex<BusinessMetrics>>,
}

impl MetricsRegistry {
    /// Create new metrics registry
    pub fn new(enabled: bool) -> Self {
        let mut counters = HashMap::new();
        let mut histograms = HashMap::new();

        // Initialize standard metrics
        counters.insert("requests_total".to_string(), Counter::new());
        counters.insert("errors_total".to_string(), Counter::new());
        counters.insert("sequences_completed".to_string(), Counter::new());
        counters.insert("sequences_failed".to_string(), Counter::new());
        counters.insert("fraud_attempts".to_string(), Counter::new());

        histograms.insert("request_duration_seconds".to_string(), Histogram::new());
        histograms.insert("game_duration_seconds".to_string(), Histogram::new());

        Self {
            enabled,
            start_time: SystemTime::now(),
            counters,
            histograms,
            performance: Arc::new(Mutex::new(PerformanceMetrics::default())),
            business: Arc::new(Mutex::new(BusinessMetrics::default())),
        }
    }

    /// Initialize the metrics system
    pub async fn initialize(&mut self) -> GameResult<()> {
        if !self.enabled {
            tracing::info!("Metrics collection disabled");
            return Ok(());
        }

        tracing::info!("Metrics registry initialized with {} counters and {} histograms",
                      self.counters.len(), self.histograms.len());
        Ok(())
    }

    /// Record operation duration
    pub fn record_operation_duration(&self, operation: &str, duration: Duration) {
        if !self.enabled {
            return;
        }

        let duration_secs = duration.as_secs_f64();

        // Record in histogram
        if let Some(histogram) = self.histograms.get("request_duration_seconds") {
            histogram.observe(duration_secs);
        }

        // Update performance metrics
        if let Ok(mut perf) = self.performance.lock() {
            perf.requests_total += 1;
            perf.request_duration_avg = self.histograms
                .get("request_duration_seconds")
                .map(|h| h.average())
                .unwrap_or(0.0);
            perf.request_duration_p95 = self.histograms
                .get("request_duration_seconds")
                .map(|h| h.percentile(95.0))
                .unwrap_or(0.0);
        }

        tracing::debug!(
            operation = operation,
            duration_ms = duration.as_millis(),
            "Recorded operation duration"
        );
    }

    /// Increment a counter metric
    pub fn increment_counter(&self, name: &str) {
        if !self.enabled {
            return;
        }

        if let Some(counter) = self.counters.get(name) {
            counter.increment();
        }
    }

    /// Add to a counter metric
    pub fn add_to_counter(&self, name: &str, amount: u64) {
        if !self.enabled {
            return;
        }

        if let Some(counter) = self.counters.get(name) {
            counter.add(amount);
        }
    }

    /// Record a histogram observation
    pub fn record_histogram(&self, name: &str, value: f64) {
        if !self.enabled {
            return;
        }

        if let Some(histogram) = self.histograms.get(name) {
            histogram.observe(value);
        }
    }

    /// Get current performance metrics
    pub fn get_performance_metrics(&self) -> PerformanceMetrics {
        if let Ok(mut perf) = self.performance.lock() {
            perf.uptime_seconds = self.start_time
                .elapsed()
                .unwrap_or(Duration::ZERO)
                .as_secs();
            perf.errors_total = self.counters
                .get("errors_total")
                .map(|c| c.get())
                .unwrap_or(0);
            perf.clone()
        } else {
            PerformanceMetrics::default()
        }
    }

    /// Get current business metrics
    pub fn get_business_metrics(&self) -> BusinessMetrics {
        if let Ok(mut business) = self.business.lock() {
            business.completed_games = self.counters
                .get("sequences_completed")
                .map(|c| c.get())
                .unwrap_or(0);
            business.failed_games = self.counters
                .get("sequences_failed")
                .map(|c| c.get())
                .unwrap_or(0);
            business.fraud_attempts = self.counters
                .get("fraud_attempts")
                .map(|c| c.get())
                .unwrap_or(0);
            business.game_duration_avg = self.histograms
                .get("game_duration_seconds")
                .map(|h| h.average())
                .unwrap_or(0.0);
            business.clone()
        } else {
            BusinessMetrics::default()
        }
    }

    /// Update business metrics
    pub fn update_business_metrics<F>(&self, updater: F)
    where
        F: FnOnce(&mut BusinessMetrics),
    {
        if !self.enabled {
            return;
        }

        if let Ok(mut business) = self.business.lock() {
            updater(&mut business);
        }
    }

    /// Export metrics in Prometheus format
    pub fn export_prometheus(&self) -> String {
        if !self.enabled {
            return String::new();
        }

        let mut output = String::new();

        // Export counters
        for (name, counter) in &self.counters {
            output.push_str(&format!("# HELP {} Total count of {}\n", name, name));
            output.push_str(&format!("# TYPE {} counter\n", name));
            output.push_str(&format!("{} {}\n", name, counter.get()));
        }

        // Export histograms
        for (name, histogram) in &self.histograms {
            let avg = histogram.average();
            let p95 = histogram.percentile(95.0);

            output.push_str(&format!("# HELP {}_seconds Duration of {}\n", name, name));
            output.push_str(&format!("# TYPE {}_seconds histogram\n", name));
            output.push_str(&format!("{}_seconds_sum {:.6}\n", name, avg));
            output.push_str(&format!("{}_seconds_count 1\n", name));
            output.push_str(&format!("{}_seconds{{quantile=\"0.95\"}} {:.6}\n", name, p95));
        }

        // Export performance metrics
        let perf = self.get_performance_metrics();
        output.push_str(&format!("# HELP system_uptime_seconds System uptime\n"));
        output.push_str(&format!("# TYPE system_uptime_seconds gauge\n"));
        output.push_str(&format!("system_uptime_seconds {}\n", perf.uptime_seconds));

        output.push_str(&format!("# HELP requests_active Currently active requests\n"));
        output.push_str(&format!("# TYPE requests_active gauge\n"));
        output.push_str(&format!("requests_active {}\n", perf.requests_active));

        output
    }

    /// Cleanup old metric data
    pub async fn cleanup_old_data(&mut self) -> GameResult<()> {
        if !self.enabled {
            return Ok(());
        }

        // For histograms, the samples are already limited to 1000 entries
        // Counters accumulate indefinitely as expected
        tracing::debug!("Metrics cleanup completed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_metrics_registry_creation() {
        let registry = MetricsRegistry::new(true);
        assert!(registry.enabled);
        assert!(!registry.counters.is_empty());
        assert!(!registry.histograms.is_empty());
    }

    #[tokio::test]
    async fn test_counter_operations() {
        let registry = MetricsRegistry::new(true);

        registry.increment_counter("requests_total");
        registry.add_to_counter("requests_total", 5);

        let counter = registry.counters.get("requests_total").unwrap();
        assert_eq!(counter.get(), 6);
    }

    #[tokio::test]
    async fn test_histogram_operations() {
        let registry = MetricsRegistry::new(true);

        registry.record_histogram("request_duration_seconds", 0.1);
        registry.record_histogram("request_duration_seconds", 0.2);
        registry.record_histogram("request_duration_seconds", 0.3);

        let histogram = registry.histograms.get("request_duration_seconds").unwrap();
        let avg = histogram.average();
        assert!((avg - 0.2).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_operation_duration_recording() {
        let registry = MetricsRegistry::new(true);
        let duration = Duration::from_millis(100);

        registry.record_operation_duration("test_op", duration);

        let metrics = registry.get_performance_metrics();
        assert_eq!(metrics.requests_total, 1);
        assert!(metrics.request_duration_avg > 0.0);
    }

    #[tokio::test]
    async fn test_prometheus_export() {
        let registry = MetricsRegistry::new(true);
        registry.increment_counter("requests_total");

        let output = registry.export_prometheus();
        assert!(output.contains("requests_total"));
        assert!(output.contains("counter"));
    }

    #[tokio::test]
    async fn test_disabled_metrics() {
        let registry = MetricsRegistry::new(false);
        registry.increment_counter("requests_total");

        let output = registry.export_prometheus();
        assert!(output.is_empty());
    }
}