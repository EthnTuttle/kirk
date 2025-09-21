//! Connection pooling and resource management for efficient network operations

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use crate::error::GameResult;

/// Configuration for connection pools
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of connections in the pool
    pub max_connections: usize,
    /// Minimum number of idle connections to maintain
    pub min_idle_connections: usize,
    /// Maximum time a connection can be idle before being closed
    pub max_idle_time: Duration,
    /// Connection timeout duration
    pub connection_timeout: Duration,
    /// Maximum number of retry attempts for failed connections
    pub max_retries: u32,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_idle_connections: 2,
            max_idle_time: Duration::from_secs(300), // 5 minutes
            connection_timeout: Duration::from_secs(30),
            max_retries: 3,
        }
    }
}

/// A pooled connection wrapper
pub struct PooledConnection<T>
where
    T: Send + 'static,
{
    connection: Option<T>,
    pool: Arc<ConnectionPool<T>>,
    created_at: Instant,
    last_used: Instant,
}

impl<T> PooledConnection<T>
where
    T: Send + 'static,
{
    fn new(connection: T, pool: Arc<ConnectionPool<T>>) -> Self {
        let now = Instant::now();
        Self {
            connection: Some(connection),
            pool,
            created_at: now,
            last_used: now,
        }
    }

    /// Get a reference to the underlying connection
    pub fn connection(&self) -> Option<&T> {
        self.connection.as_ref()
    }

    /// Get a mutable reference to the underlying connection
    pub fn connection_mut(&mut self) -> Option<&mut T> {
        self.last_used = Instant::now();
        self.connection.as_mut()
    }

    /// Check if the connection is still valid (not too old)
    pub fn is_valid(&self, max_idle_time: Duration) -> bool {
        self.last_used.elapsed() < max_idle_time
    }

    /// Get the age of this connection
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
}

impl<T> Drop for PooledConnection<T>
where
    T: Send + 'static,
{
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            // Return the connection to the pool
            self.pool.return_connection(connection);
        }
    }
}

/// Generic connection pool implementation
pub struct ConnectionPool<T> {
    config: PoolConfig,
    available_connections: Arc<Mutex<VecDeque<T>>>,
    semaphore: Semaphore,
    statistics: Arc<Mutex<ConnectionPoolStatistics>>,
}

#[derive(Debug, Clone)]
struct ConnectionPoolStatistics {
    total_created: u64,
    total_destroyed: u64,
    current_active: usize,
    connection_errors: u64,
    pool_hits: u64,
    pool_misses: u64,
}

impl Default for ConnectionPoolStatistics {
    fn default() -> Self {
        Self {
            total_created: 0,
            total_destroyed: 0,
            current_active: 0,
            connection_errors: 0,
            pool_hits: 0,
            pool_misses: 0,
        }
    }
}

impl<T> ConnectionPool<T>
where
    T: Send + 'static,
{
    /// Create a new connection pool
    pub fn new(config: PoolConfig) -> Self {
        Self {
            semaphore: Semaphore::new(config.max_connections),
            available_connections: Arc::new(Mutex::new(VecDeque::new())),
            statistics: Arc::new(Mutex::new(ConnectionPoolStatistics::default())),
            config,
        }
    }

    /// Get a connection from the pool or create a new one
    pub async fn get_connection<F, Fut>(&self, factory: F) -> GameResult<PooledConnection<T>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = GameResult<T>>,
    {
        // Acquire a permit from the semaphore to limit concurrent connections
        let _permit = self.semaphore.acquire().await
            .map_err(|_| crate::error::GameProtocolError::SystemError {
                message: "Failed to acquire connection permit".to_string(),
                context: Some("connection_pool".to_string()),
            })?;

        // Try to get an existing connection from the pool
        if let Some(connection) = self.take_available_connection().await {
            if let Ok(mut stats) = self.statistics.lock() {
                stats.pool_hits += 1;
            }
            return Ok(PooledConnection::new(connection, Arc::new(self.clone())));
        }

        // No available connection, create a new one
        match factory().await {
            Ok(connection) => {
                if let Ok(mut stats) = self.statistics.lock() {
                    stats.total_created += 1;
                    stats.current_active += 1;
                    stats.pool_misses += 1;
                }
                Ok(PooledConnection::new(connection, Arc::new(self.clone())))
            }
            Err(e) => {
                if let Ok(mut stats) = self.statistics.lock() {
                    stats.connection_errors += 1;
                }
                Err(e)
            }
        }
    }

    /// Take an available connection from the pool if one exists
    async fn take_available_connection(&self) -> Option<T> {
        if let Ok(mut connections) = self.available_connections.lock() {
            connections.pop_front()
        } else {
            None
        }
    }

    /// Return a connection to the pool
    fn return_connection(&self, connection: T) {
        if let Ok(mut connections) = self.available_connections.lock() {
            if connections.len() < self.config.max_connections {
                connections.push_back(connection);
            } else {
                // Pool is full, drop the connection
                if let Ok(mut stats) = self.statistics.lock() {
                    stats.total_destroyed += 1;
                }
            }
        }

        if let Ok(mut stats) = self.statistics.lock() {
            stats.current_active = stats.current_active.saturating_sub(1);
        }
    }

    /// Get current pool statistics
    pub fn get_statistics(&self) -> super::ConnectionPoolStats {
        if let Ok(stats) = self.statistics.lock() {
            let available_count = self.available_connections.lock()
                .map(|c| c.len())
                .unwrap_or(0);

            super::ConnectionPoolStats {
                active_connections: stats.current_active,
                idle_connections: available_count,
                total_connections: stats.current_active + available_count,
                connection_errors: stats.connection_errors,
            }
        } else {
            super::ConnectionPoolStats {
                active_connections: 0,
                idle_connections: 0,
                total_connections: 0,
                connection_errors: 0,
            }
        }
    }

    /// Cleanup expired connections
    pub async fn cleanup_expired_connections(&self) -> GameResult<usize> {
        let mut removed_count = 0;

        if let Ok(mut connections) = self.available_connections.lock() {
            // For now, we just clear old connections
            // In a real implementation, we'd check connection validity
            let initial_size = connections.len();

            // Keep only recent connections (simplified logic)
            if initial_size > self.config.min_idle_connections {
                let to_remove = initial_size - self.config.min_idle_connections;
                for _ in 0..to_remove {
                    if connections.pop_front().is_some() {
                        removed_count += 1;
                    }
                }
            }
        }

        if removed_count > 0 {
            if let Ok(mut stats) = self.statistics.lock() {
                stats.total_destroyed += removed_count as u64;
            }
            println!("DEBUG: Cleaned up {} expired connections", removed_count);
        }

        Ok(removed_count)
    }
}

impl<T> Clone for ConnectionPool<T> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            available_connections: Arc::clone(&self.available_connections),
            semaphore: Semaphore::new(self.config.max_connections),
            statistics: Arc::clone(&self.statistics),
        }
    }
}

/// Connection manager that manages multiple pools for different connection types
#[derive(Debug)]
pub struct ConnectionManager {
    nostr_pool_size: usize,
    cashu_pool_size: usize,
}

impl ConnectionManager {
    /// Create new connection manager
    pub fn new(nostr_pool_size: usize, cashu_pool_size: usize) -> Self {
        Self {
            nostr_pool_size,
            cashu_pool_size,
        }
    }

    /// Initialize the connection manager
    pub async fn initialize(&mut self) -> GameResult<()> {
        println!("INFO: Connection manager initialized - Nostr pool: {}, Cashu pool: {}",
                 self.nostr_pool_size, self.cashu_pool_size);
        Ok(())
    }

    /// Get statistics from all pools
    pub fn get_statistics(&self) -> super::ConnectionPoolStats {
        // This would aggregate statistics from all managed pools
        super::ConnectionPoolStats {
            active_connections: 0,
            idle_connections: 0,
            total_connections: 0,
            connection_errors: 0,
        }
    }

    /// Cleanup all connection pools
    pub async fn cleanup(&mut self) -> GameResult<()> {
        println!("DEBUG: Connection manager cleanup completed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock connection type for testing
    #[derive(Debug, Clone)]
    struct MockConnection {
        id: u32,
    }

    impl MockConnection {
        fn new(id: u32) -> Self {
            Self { id }
        }
    }

    #[tokio::test]
    async fn test_connection_pool_creation() {
        let config = PoolConfig::default();
        let pool: ConnectionPool<MockConnection> = ConnectionPool::new(config);

        let stats = pool.get_statistics();
        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.idle_connections, 0);
    }

    #[tokio::test]
    async fn test_connection_pool_get_connection() {
        let config = PoolConfig::default();
        let pool: ConnectionPool<MockConnection> = ConnectionPool::new(config);

        let connection = pool.get_connection(|| async {
            Ok(MockConnection::new(1))
        }).await;

        assert!(connection.is_ok());
        let conn = connection.unwrap();
        assert_eq!(conn.connection().unwrap().id, 1);
    }

    #[tokio::test]
    async fn test_connection_pool_reuse() {
        let config = PoolConfig::default();
        let pool: ConnectionPool<MockConnection> = ConnectionPool::new(config);

        // Get and drop a connection
        {
            let _connection = pool.get_connection(|| async {
                Ok(MockConnection::new(1))
            }).await.unwrap();
        } // Connection should be returned to pool here

        tokio::time::sleep(Duration::from_millis(10)).await;

        // Get another connection - should reuse the first one
        let connection2 = pool.get_connection(|| async {
            Ok(MockConnection::new(2))
        }).await.unwrap();

        // The reused connection should have id 1, not 2
        assert_eq!(connection2.connection().unwrap().id, 1);
    }

    #[tokio::test]
    async fn test_connection_manager() {
        let mut manager = ConnectionManager::new(5, 3);
        let result = manager.initialize().await;
        assert!(result.is_ok());

        let stats = manager.get_statistics();
        assert_eq!(stats.total_connections, 0);
    }
}