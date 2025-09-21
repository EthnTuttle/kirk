//! Health checking and system status monitoring

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use serde::{Deserialize, Serialize};
use crate::error::GameResult;

/// Overall system health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy {
        message: String,
        timestamp: u64,
        components: HashMap<String, ComponentHealth>,
    },
    Degraded {
        message: String,
        timestamp: u64,
        components: HashMap<String, ComponentHealth>,
        issues: Vec<String>,
    },
    Unhealthy {
        message: String,
        timestamp: u64,
        components: HashMap<String, ComponentHealth>,
        errors: Vec<String>,
    },
}

impl HealthStatus {
    /// Create a healthy status
    pub fn healthy(message: &str) -> Self {
        Self::Healthy {
            message: message.to_string(),
            timestamp: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs(),
            components: HashMap::new(),
        }
    }

    /// Create a degraded status
    pub fn degraded(message: &str, issues: Vec<String>) -> Self {
        Self::Degraded {
            message: message.to_string(),
            timestamp: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs(),
            components: HashMap::new(),
            issues,
        }
    }

    /// Create an unhealthy status
    pub fn unhealthy(message: &str, errors: Vec<String>) -> Self {
        Self::Unhealthy {
            message: message.to_string(),
            timestamp: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs(),
            components: HashMap::new(),
            errors,
        }
    }

    /// Check if the system is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy { .. })
    }

    /// Check if the system is degraded
    pub fn is_degraded(&self) -> bool {
        matches!(self, HealthStatus::Degraded { .. })
    }

    /// Check if the system is unhealthy
    pub fn is_unhealthy(&self) -> bool {
        matches!(self, HealthStatus::Unhealthy { .. })
    }

    /// Get the status message
    pub fn message(&self) -> &str {
        match self {
            HealthStatus::Healthy { message, .. } => message,
            HealthStatus::Degraded { message, .. } => message,
            HealthStatus::Unhealthy { message, .. } => message,
        }
    }

    /// Get the timestamp
    pub fn timestamp(&self) -> u64 {
        match self {
            HealthStatus::Healthy { timestamp, .. } => *timestamp,
            HealthStatus::Degraded { timestamp, .. } => *timestamp,
            HealthStatus::Unhealthy { timestamp, .. } => *timestamp,
        }
    }

    /// Get component health status
    pub fn components(&self) -> &HashMap<String, ComponentHealth> {
        match self {
            HealthStatus::Healthy { components, .. } => components,
            HealthStatus::Degraded { components, .. } => components,
            HealthStatus::Unhealthy { components, .. } => components,
        }
    }

    /// Add component health to status
    pub fn with_component(mut self, name: String, health: ComponentHealth) -> Self {
        match &mut self {
            HealthStatus::Healthy { components, .. } => {
                components.insert(name, health);
            }
            HealthStatus::Degraded { components, .. } => {
                components.insert(name, health);
            }
            HealthStatus::Unhealthy { components, .. } => {
                components.insert(name, health);
            }
        }
        self
    }
}

/// Health status of individual system components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Component name
    pub name: String,
    /// Whether the component is healthy
    pub healthy: bool,
    /// Optional status message
    pub message: Option<String>,
    /// Last check timestamp
    pub last_checked: u64,
    /// Response time in milliseconds
    pub response_time_ms: Option<u64>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl ComponentHealth {
    /// Create a healthy component
    pub fn healthy(name: &str) -> Self {
        Self {
            name: name.to_string(),
            healthy: true,
            message: None,
            last_checked: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs(),
            response_time_ms: None,
            metadata: HashMap::new(),
        }
    }

    /// Create an unhealthy component
    pub fn unhealthy(name: &str, message: &str) -> Self {
        Self {
            name: name.to_string(),
            healthy: false,
            message: Some(message.to_string()),
            last_checked: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs(),
            response_time_ms: None,
            metadata: HashMap::new(),
        }
    }

    /// Set response time
    pub fn with_response_time(mut self, response_time_ms: u64) -> Self {
        self.response_time_ms = Some(response_time_ms);
        self
    }

    /// Set message
    pub fn with_message(mut self, message: &str) -> Self {
        self.message = Some(message.to_string());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// Health check function type
pub type HealthCheckFn = Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ComponentHealth> + Send>> + Send + Sync>;

/// Main health checker that monitors system components
pub struct HealthChecker {
    enabled: bool,
    checks: HashMap<String, HealthCheckFn>,
    last_check_time: Option<SystemTime>,
    cached_status: Option<HealthStatus>,
}

impl std::fmt::Debug for HealthChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthChecker")
            .field("enabled", &self.enabled)
            .field("checks_count", &self.checks.len())
            .field("last_check_time", &self.last_check_time)
            .field("cached_status", &self.cached_status.as_ref().map(|s| s.message()))
            .finish()
    }
}

impl HealthChecker {
    /// Create new health checker
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            checks: HashMap::new(),
            last_check_time: None,
            cached_status: None,
        }
    }

    /// Initialize the health checker
    pub async fn initialize(&mut self) -> GameResult<()> {
        if !self.enabled {
            tracing::info!("Health checks disabled");
            return Ok(());
        }

        // Register default health checks
        self.register_default_checks();

        tracing::info!("Health checker initialized with {} checks", self.checks.len());
        Ok(())
    }

    /// Register a health check function
    pub fn register_check<F, Fut>(&mut self, name: &str, check_fn: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ComponentHealth> + Send + 'static,
    {
        let check_fn = Arc::new(move || -> std::pin::Pin<Box<dyn std::future::Future<Output = ComponentHealth> + Send>> {
            Box::pin(check_fn())
        });
        self.checks.insert(name.to_string(), check_fn);
    }

    /// Register default system health checks
    fn register_default_checks(&mut self) {
        // Memory usage check
        self.register_check("memory", || async {
            // Simple memory check - in a real implementation you'd check actual memory usage
            let available_memory = 1024 * 1024 * 1024; // 1GB placeholder
            if available_memory > 100 * 1024 * 1024 { // 100MB threshold
                ComponentHealth::healthy("memory")
                    .with_message("Memory usage within acceptable limits")
                    .with_metadata("available_mb", &format!("{}", available_memory / 1024 / 1024))
            } else {
                ComponentHealth::unhealthy("memory", "Low memory available")
                    .with_metadata("available_mb", &format!("{}", available_memory / 1024 / 1024))
            }
        });

        // Disk space check
        self.register_check("disk", || async {
            // Simple disk check - in a real implementation you'd check actual disk usage
            let available_disk = 10 * 1024 * 1024 * 1024; // 10GB placeholder
            if available_disk > 1024 * 1024 * 1024 { // 1GB threshold
                ComponentHealth::healthy("disk")
                    .with_message("Disk space sufficient")
                    .with_metadata("available_gb", &format!("{}", available_disk / 1024 / 1024 / 1024))
            } else {
                ComponentHealth::unhealthy("disk", "Low disk space")
                    .with_metadata("available_gb", &format!("{}", available_disk / 1024 / 1024 / 1024))
            }
        });

        // System load check
        self.register_check("system_load", || async {
            // Simple load check - in a real implementation you'd check actual system load
            let load_average = 0.5; // Placeholder
            if load_average < 5.0 {
                ComponentHealth::healthy("system_load")
                    .with_message("System load normal")
                    .with_metadata("load_average", &format!("{:.2}", load_average))
            } else {
                ComponentHealth::unhealthy("system_load", "High system load")
                    .with_metadata("load_average", &format!("{:.2}", load_average))
            }
        });
    }

    /// Check the health of all registered components
    pub async fn check_system_health(&self) -> HealthStatus {
        if !self.enabled {
            return HealthStatus::healthy("Health checks disabled");
        }

        let mut component_healths = HashMap::new();
        let mut issues = Vec::new();
        let mut errors = Vec::new();

        // Run all health checks
        for (name, check_fn) in &self.checks {
            let start_time = std::time::Instant::now();
            let health = check_fn().await;
            let response_time = start_time.elapsed().as_millis() as u64;

            let health = health.with_response_time(response_time);

            if !health.healthy {
                if let Some(message) = &health.message {
                    if message.contains("Low") || message.contains("High") {
                        issues.push(format!("{}: {}", name, message));
                    } else {
                        errors.push(format!("{}: {}", name, message));
                    }
                } else {
                    errors.push(format!("{}: unhealthy", name));
                }
            }

            component_healths.insert(name.clone(), health);
        }

        // Determine overall status
        let status = if !errors.is_empty() {
            HealthStatus::unhealthy("System has critical health issues", errors)
        } else if !issues.is_empty() {
            HealthStatus::degraded("System has performance issues", issues)
        } else {
            HealthStatus::healthy("All systems operational")
        };

        // Add component healths to status
        let mut status = status;
        for (name, health) in component_healths {
            status = status.with_component(name, health);
        }

        tracing::info!(
            healthy = status.is_healthy(),
            degraded = status.is_degraded(),
            components_checked = self.checks.len(),
            "Health check completed"
        );

        status
    }

    /// Get a quick readiness check (for load balancers)
    pub async fn check_readiness(&self) -> bool {
        if !self.enabled {
            return true;
        }

        // Simple readiness check - system is ready if no critical errors
        let health = self.check_system_health().await;
        !health.is_unhealthy()
    }

    /// Get a liveness check (for container orchestrators)
    pub async fn check_liveness(&self) -> bool {
        if !self.enabled {
            return true;
        }

        // Simple liveness check - system is alive if it can respond
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_status_creation() {
        let healthy = HealthStatus::healthy("All good");
        assert!(healthy.is_healthy());
        assert!(!healthy.is_degraded());
        assert!(!healthy.is_unhealthy());

        let degraded = HealthStatus::degraded("Some issues", vec!["Issue 1".to_string()]);
        assert!(!degraded.is_healthy());
        assert!(degraded.is_degraded());
        assert!(!degraded.is_unhealthy());

        let unhealthy = HealthStatus::unhealthy("Critical error", vec!["Error 1".to_string()]);
        assert!(!unhealthy.is_healthy());
        assert!(!unhealthy.is_degraded());
        assert!(unhealthy.is_unhealthy());
    }

    #[tokio::test]
    async fn test_component_health() {
        let healthy = ComponentHealth::healthy("test")
            .with_message("Working fine")
            .with_response_time(50)
            .with_metadata("version", "1.0");

        assert!(healthy.healthy);
        assert_eq!(healthy.name, "test");
        assert_eq!(healthy.message.unwrap(), "Working fine");
        assert_eq!(healthy.response_time_ms.unwrap(), 50);
        assert_eq!(healthy.metadata.get("version").unwrap(), "1.0");

        let unhealthy = ComponentHealth::unhealthy("test", "Not working");
        assert!(!unhealthy.healthy);
    }

    #[tokio::test]
    async fn test_health_checker() {
        let mut checker = HealthChecker::new(true);
        checker.initialize().await.unwrap();

        // Register a custom check
        checker.register_check("custom", || async {
            ComponentHealth::healthy("custom").with_message("Custom check passed")
        });

        let status = checker.check_system_health().await;
        assert!(status.is_healthy());
        assert!(!status.components().is_empty());
    }

    #[tokio::test]
    async fn test_readiness_and_liveness() {
        let checker = HealthChecker::new(true);

        let ready = checker.check_readiness().await;
        assert!(ready);

        let alive = checker.check_liveness().await;
        assert!(alive);
    }

    #[tokio::test]
    async fn test_disabled_health_checker() {
        let checker = HealthChecker::new(false);

        let status = checker.check_system_health().await;
        assert!(status.is_healthy());

        let ready = checker.check_readiness().await;
        assert!(ready);

        let alive = checker.check_liveness().await;
        assert!(alive);
    }
}