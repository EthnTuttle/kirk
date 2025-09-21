//! Performance optimization and scalability components

pub mod connection_pool;
pub mod data_structures;
pub mod memory_management;
pub mod backpressure;

pub use connection_pool::{ConnectionManager, ConnectionPool, PooledConnection};
pub use data_structures::{IndexedSequenceStore, EventIndex, FastLookupTable};
pub use memory_management::{MemoryManager, MemoryLimits, ResourceTracker};
pub use backpressure::{BackpressureHandler, LoadBalancer, RateLimitedProcessor};

use crate::error::GameResult;
//

/// Performance configuration for production deployments
#[derive(Debug, Clone)]
pub struct PerformanceConfig {
    /// Maximum memory usage in bytes
    pub max_memory_usage: u64,
    /// Connection pool sizes
    pub nostr_pool_size: usize,
    pub cashu_pool_size: usize,
    /// Backpressure thresholds
    pub queue_size_limit: usize,
    pub processing_rate_limit: u32,
    /// Caching configuration
    pub enable_caching: bool,
    pub cache_size_limit: usize,
    pub cache_ttl_seconds: u64,
    /// Streaming and pagination
    pub max_batch_size: usize,
    pub stream_buffer_size: usize,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_memory_usage: 1024 * 1024 * 1024, // 1GB
            nostr_pool_size: 10,
            cashu_pool_size: 5,
            queue_size_limit: 1000,
            processing_rate_limit: 100, // requests per second
            enable_caching: true,
            cache_size_limit: 1000,
            cache_ttl_seconds: 300, // 5 minutes
            max_batch_size: 100,
            stream_buffer_size: 8192,
        }
    }
}

/// Main performance manager that coordinates all optimization components
#[derive(Debug)]
pub struct PerformanceManager {
    config: PerformanceConfig,
    connection_manager: ConnectionManager,
    memory_manager: MemoryManager,
    backpressure_handler: BackpressureHandler,
}

impl PerformanceManager {
    /// Create new performance manager
    pub fn new(config: PerformanceConfig) -> Self {
        let connection_manager = ConnectionManager::new(
            config.nostr_pool_size,
            config.cashu_pool_size,
        );

        let memory_manager = MemoryManager::new(MemoryLimits {
            max_total_memory: config.max_memory_usage,
            max_cache_memory: config.max_memory_usage / 4, // 25% for cache
            max_buffer_memory: config.max_memory_usage / 8, // 12.5% for buffers
        });

        let backpressure_handler = BackpressureHandler::new(
            config.queue_size_limit,
            config.processing_rate_limit,
        );

        Self {
            config,
            connection_manager,
            memory_manager,
            backpressure_handler,
        }
    }

    /// Initialize the performance system
    pub async fn initialize(&mut self) -> GameResult<()> {
        self.connection_manager.initialize().await?;
        self.memory_manager.initialize().await?;
        self.backpressure_handler.initialize().await?;

        println!("INFO: Performance manager initialized with {} MB max memory",
                 self.config.max_memory_usage / 1024 / 1024);
        Ok(())
    }

    /// Get connection manager
    pub fn connections(&self) -> &ConnectionManager {
        &self.connection_manager
    }

    /// Get memory manager
    pub fn memory(&self) -> &MemoryManager {
        &self.memory_manager
    }

    /// Get backpressure handler
    pub fn backpressure(&self) -> &BackpressureHandler {
        &self.backpressure_handler
    }

    /// Check if system is under load
    pub fn is_under_load(&self) -> bool {
        self.backpressure_handler.is_overloaded() ||
        self.memory_manager.is_memory_constrained()
    }

    /// Apply backpressure if necessary
    pub async fn apply_backpressure(&self) -> GameResult<bool> {
        if self.is_under_load() {
            self.backpressure_handler.apply_backpressure().await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get performance statistics
    pub fn get_statistics(&self) -> PerformanceStatistics {
        PerformanceStatistics {
            memory_usage: self.memory_manager.get_current_usage(),
            connection_pool_stats: self.connection_manager.get_statistics(),
            backpressure_stats: self.backpressure_handler.get_statistics(),
            cache_hit_rate: self.memory_manager.get_cache_hit_rate(),
        }
    }

    /// Cleanup and optimize resources
    pub async fn cleanup(&mut self) -> GameResult<()> {
        self.memory_manager.cleanup().await?;
        self.connection_manager.cleanup().await?;
        self.backpressure_handler.cleanup().await?;

        println!("DEBUG: Performance cleanup completed");
        Ok(())
    }
}

/// Performance statistics for monitoring
#[derive(Debug, Clone)]
pub struct PerformanceStatistics {
    pub memory_usage: MemoryUsage,
    pub connection_pool_stats: ConnectionPoolStats,
    pub backpressure_stats: BackpressureStats,
    pub cache_hit_rate: f64,
}

#[derive(Debug, Clone)]
pub struct MemoryUsage {
    pub total_allocated: u64,
    pub cache_memory: u64,
    pub buffer_memory: u64,
    pub utilization_percent: f64,
}

#[derive(Debug, Clone)]
pub struct ConnectionPoolStats {
    pub active_connections: usize,
    pub idle_connections: usize,
    pub total_connections: usize,
    pub connection_errors: u64,
}

#[derive(Debug, Clone)]
pub struct BackpressureStats {
    pub queue_size: usize,
    pub processing_rate: f64,
    pub dropped_requests: u64,
    pub backpressure_active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_performance_manager_creation() {
        let config = PerformanceConfig::default();
        let manager = PerformanceManager::new(config);

        assert_eq!(manager.config.max_memory_usage, 1024 * 1024 * 1024);
        assert_eq!(manager.config.nostr_pool_size, 10);
        assert!(!manager.is_under_load()); // Should start unloaded
    }

    #[tokio::test]
    async fn test_performance_initialization() {
        let config = PerformanceConfig::default();
        let mut manager = PerformanceManager::new(config);

        let result = manager.initialize().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_performance_statistics() {
        let config = PerformanceConfig::default();
        let manager = PerformanceManager::new(config);

        let stats = manager.get_statistics();
        assert!(stats.cache_hit_rate >= 0.0);
        assert!(stats.cache_hit_rate <= 1.0);
    }

    #[tokio::test]
    async fn test_backpressure_application() {
        let config = PerformanceConfig::default();
        let manager = PerformanceManager::new(config);

        let applied = manager.apply_backpressure().await.unwrap();
        assert!(!applied); // Should not apply backpressure when not under load
    }
}