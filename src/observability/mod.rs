//! Observability and monitoring infrastructure for Kirk gaming protocol

pub mod metrics;
pub mod health;
pub mod tracing;

pub use metrics::{MetricsRegistry, PerformanceMetrics, BusinessMetrics};
pub use health::{HealthChecker, HealthStatus, ComponentHealth};
pub use tracing::{CorrelationContext, RequestTracing};

use crate::error::GameResult;

/// Observability configuration
#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    /// Enable metrics collection
    pub enable_metrics: bool,
    /// Enable health checks
    pub enable_health_checks: bool,
    /// Enable detailed tracing
    pub enable_tracing: bool,
    /// Metrics export interval in seconds
    pub metrics_interval: u64,
    /// Health check interval in seconds
    pub health_check_interval: u64,
    /// Maximum number of correlation IDs to track
    pub max_correlation_ids: usize,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enable_metrics: true,
            enable_health_checks: true,
            enable_tracing: true,
            metrics_interval: 30,
            health_check_interval: 60,
            max_correlation_ids: 10000,
        }
    }
}

/// Main observability manager that coordinates all monitoring components
#[derive(Debug)]
pub struct ObservabilityManager {
    config: ObservabilityConfig,
    metrics: MetricsRegistry,
    health: HealthChecker,
    tracing: RequestTracing,
}

impl ObservabilityManager {
    /// Create new observability manager
    pub fn new(config: ObservabilityConfig) -> Self {
        let metrics = MetricsRegistry::new(config.enable_metrics);
        let health = HealthChecker::new(config.enable_health_checks);
        let tracing = RequestTracing::new(config.max_correlation_ids);

        Self {
            config,
            metrics,
            health,
            tracing,
        }
    }

    /// Initialize observability systems
    pub async fn initialize(&mut self) -> GameResult<()> {
        if self.config.enable_metrics {
            self.metrics.initialize().await?;
        }

        if self.config.enable_health_checks {
            self.health.initialize().await?;
        }

        if self.config.enable_tracing {
            self.tracing.initialize().await?;
        }

        println!("INFO: Observability manager initialized - metrics: {}, health: {}, tracing: {}",
                 self.config.enable_metrics, self.config.enable_health_checks, self.config.enable_tracing);

        Ok(())
    }

    /// Get metrics registry
    pub fn metrics(&self) -> &MetricsRegistry {
        &self.metrics
    }

    /// Get health checker
    pub fn health(&self) -> &HealthChecker {
        &self.health
    }

    /// Get request tracing
    pub fn tracing(&self) -> &RequestTracing {
        &self.tracing
    }

    /// Record an operation with timing and context
    pub async fn record_operation<F, R>(&self, operation: &str, f: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        let start = std::time::Instant::now();
        let result = f.await;
        let duration = start.elapsed();

        if self.config.enable_metrics {
            self.metrics.record_operation_duration(operation, duration);
        }

        println!("INFO: Operation '{}' completed in {}ms", operation, duration.as_millis());

        result
    }

    /// Get system health status
    pub async fn get_health_status(&self) -> HealthStatus {
        if self.config.enable_health_checks {
            self.health.check_system_health().await
        } else {
            HealthStatus::healthy("Health checks disabled")
        }
    }

    /// Export metrics in Prometheus format
    pub fn export_metrics(&self) -> String {
        if self.config.enable_metrics {
            self.metrics.export_prometheus()
        } else {
            String::new()
        }
    }

    /// Cleanup old data and rotate logs
    pub async fn cleanup(&mut self) -> GameResult<()> {
        if self.config.enable_tracing {
            self.tracing.cleanup_old_correlations().await?;
        }

        if self.config.enable_metrics {
            self.metrics.cleanup_old_data().await?;
        }

        println!("DEBUG: Observability cleanup completed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_observability_manager_creation() {
        let config = ObservabilityConfig::default();
        let manager = ObservabilityManager::new(config);

        assert!(manager.config.enable_metrics);
        assert!(manager.config.enable_health_checks);
        assert!(manager.config.enable_tracing);
    }

    #[tokio::test]
    async fn test_observability_initialization() {
        let config = ObservabilityConfig::default();
        let mut manager = ObservabilityManager::new(config);

        let result = manager.initialize().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_record_operation() {
        let config = ObservabilityConfig::default();
        let manager = ObservabilityManager::new(config);

        let result = manager.record_operation("test_op", async {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            "test_result"
        }).await;

        assert_eq!(result, "test_result");
    }

    #[tokio::test]
    async fn test_health_status() {
        let config = ObservabilityConfig::default();
        let manager = ObservabilityManager::new(config);

        let health = manager.get_health_status().await;
        assert!(health.is_healthy());
    }
}