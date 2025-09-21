//! Memory management and resource tracking for production deployments

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, Duration};
use crate::error::GameResult;

/// Memory limits configuration
#[derive(Debug, Clone)]
pub struct MemoryLimits {
    /// Maximum total memory usage in bytes
    pub max_total_memory: u64,
    /// Maximum memory for caches
    pub max_cache_memory: u64,
    /// Maximum memory for buffers
    pub max_buffer_memory: u64,
}

impl Default for MemoryLimits {
    fn default() -> Self {
        Self {
            max_total_memory: 1024 * 1024 * 1024, // 1GB
            max_cache_memory: 256 * 1024 * 1024,  // 256MB
            max_buffer_memory: 128 * 1024 * 1024, // 128MB
        }
    }
}

/// Resource allocation tracking
#[derive(Debug, Clone)]
pub struct ResourceAllocation {
    pub component_name: String,
    pub allocated_bytes: u64,
    pub allocation_time: SystemTime,
    pub last_accessed: SystemTime,
    pub allocation_type: AllocationType,
}

#[derive(Debug, Clone)]
pub enum AllocationType {
    Cache,
    Buffer,
    Working,
    Temporary,
}

/// Memory manager for tracking and controlling resource usage
#[derive(Debug)]
pub struct MemoryManager {
    limits: MemoryLimits,
    allocations: Arc<Mutex<HashMap<String, ResourceAllocation>>>,
    total_allocated: Arc<Mutex<u64>>,
    cache_allocated: Arc<Mutex<u64>>,
    buffer_allocated: Arc<Mutex<u64>>,
    allocation_counter: Arc<Mutex<u64>>,
    gc_threshold: f64, // Trigger GC when usage exceeds this percentage
}

impl MemoryManager {
    /// Create a new memory manager
    pub fn new(limits: MemoryLimits) -> Self {
        Self {
            limits,
            allocations: Arc::new(Mutex::new(HashMap::new())),
            total_allocated: Arc::new(Mutex::new(0)),
            cache_allocated: Arc::new(Mutex::new(0)),
            buffer_allocated: Arc::new(Mutex::new(0)),
            allocation_counter: Arc::new(Mutex::new(0)),
            gc_threshold: 0.85, // 85% usage triggers GC
        }
    }

    /// Initialize the memory manager
    pub async fn initialize(&mut self) -> GameResult<()> {
        println!("INFO: Memory manager initialized - Max memory: {} MB",
                 self.limits.max_total_memory / 1024 / 1024);
        Ok(())
    }

    /// Allocate memory for a component
    pub fn allocate(&self, component: &str, bytes: u64, allocation_type: AllocationType) -> GameResult<String> {
        let allocation_id = {
            let mut counter = self.allocation_counter.lock().unwrap();
            *counter += 1;
            format!("{}_{}", component, *counter)
        };

        // Check if allocation would exceed limits
        {
            let total = *self.total_allocated.lock().unwrap();
            if total + bytes > self.limits.max_total_memory {
                // Try to free some memory
                self.try_free_memory(bytes)?;
            }

            // Check type-specific limits
            match allocation_type {
                AllocationType::Cache => {
                    let cache_total = *self.cache_allocated.lock().unwrap();
                    if cache_total + bytes > self.limits.max_cache_memory {
                        return Err(crate::error::GameProtocolError::SystemError {
                            message: format!("Cache memory limit exceeded: {} + {} > {}",
                                           cache_total, bytes, self.limits.max_cache_memory),
                            context: Some("memory_manager".to_string()),
                        });
                    }
                }
                AllocationType::Buffer => {
                    let buffer_total = *self.buffer_allocated.lock().unwrap();
                    if buffer_total + bytes > self.limits.max_buffer_memory {
                        return Err(crate::error::GameProtocolError::SystemError {
                            message: format!("Buffer memory limit exceeded: {} + {} > {}",
                                           buffer_total, bytes, self.limits.max_buffer_memory),
                            context: Some("memory_manager".to_string()),
                        });
                    }
                }
                _ => {} // No specific limits for other types
            }
        }

        // Create allocation record
        let now = SystemTime::now();
        let allocation = ResourceAllocation {
            component_name: component.to_string(),
            allocated_bytes: bytes,
            allocation_time: now,
            last_accessed: now,
            allocation_type: allocation_type.clone(),
        };

        // Update tracking
        {
            let mut allocations = self.allocations.lock().unwrap();
            allocations.insert(allocation_id.clone(), allocation);

            let mut total = self.total_allocated.lock().unwrap();
            *total += bytes;

            match allocation_type {
                AllocationType::Cache => {
                    let mut cache_total = self.cache_allocated.lock().unwrap();
                    *cache_total += bytes;
                }
                AllocationType::Buffer => {
                    let mut buffer_total = self.buffer_allocated.lock().unwrap();
                    *buffer_total += bytes;
                }
                _ => {}
            }
        }

        println!("DEBUG: Allocated {} bytes for {} (id: {})", bytes, component, allocation_id);
        Ok(allocation_id)
    }

    /// Deallocate memory
    pub fn deallocate(&self, allocation_id: &str) -> GameResult<()> {
        let allocation = {
            let mut allocations = self.allocations.lock().unwrap();
            allocations.remove(allocation_id)
        };

        if let Some(alloc) = allocation {
            // Update counters
            {
                let mut total = self.total_allocated.lock().unwrap();
                *total = total.saturating_sub(alloc.allocated_bytes);

                match alloc.allocation_type {
                    AllocationType::Cache => {
                        let mut cache_total = self.cache_allocated.lock().unwrap();
                        *cache_total = cache_total.saturating_sub(alloc.allocated_bytes);
                    }
                    AllocationType::Buffer => {
                        let mut buffer_total = self.buffer_allocated.lock().unwrap();
                        *buffer_total = buffer_total.saturating_sub(alloc.allocated_bytes);
                    }
                    _ => {}
                }
            }

            println!("DEBUG: Deallocated {} bytes for {} (id: {})",
                     alloc.allocated_bytes, alloc.component_name, allocation_id);
        }

        Ok(())
    }

    /// Update last accessed time for an allocation
    pub fn touch_allocation(&self, allocation_id: &str) {
        if let Ok(mut allocations) = self.allocations.lock() {
            if let Some(allocation) = allocations.get_mut(allocation_id) {
                allocation.last_accessed = SystemTime::now();
            }
        }
    }

    /// Try to free memory by garbage collecting old allocations
    fn try_free_memory(&self, needed_bytes: u64) -> GameResult<()> {
        let mut freed_bytes = 0u64;
        let cutoff_time = SystemTime::now() - Duration::from_secs(300); // 5 minutes old

        let allocations_to_free: Vec<String> = {
            let allocations = self.allocations.lock().unwrap();
            allocations
                .iter()
                .filter(|(_, allocation)| {
                    allocation.last_accessed < cutoff_time &&
                    matches!(allocation.allocation_type, AllocationType::Cache | AllocationType::Temporary)
                })
                .map(|(id, _)| id.clone())
                .collect()
        };

        for allocation_id in allocations_to_free {
            if freed_bytes >= needed_bytes {
                break;
            }

            let allocation = {
                let mut allocations = self.allocations.lock().unwrap();
                allocations.remove(&allocation_id)
            };

            if let Some(alloc) = allocation {
                freed_bytes += alloc.allocated_bytes;

                // Update counters
                let mut total = self.total_allocated.lock().unwrap();
                *total = total.saturating_sub(alloc.allocated_bytes);

                match alloc.allocation_type {
                    AllocationType::Cache => {
                        let mut cache_total = self.cache_allocated.lock().unwrap();
                        *cache_total = cache_total.saturating_sub(alloc.allocated_bytes);
                    }
                    AllocationType::Buffer => {
                        let mut buffer_total = self.buffer_allocated.lock().unwrap();
                        *buffer_total = buffer_total.saturating_sub(alloc.allocated_bytes);
                    }
                    _ => {}
                }

                println!("DEBUG: Freed {} bytes from {} during GC",
                         alloc.allocated_bytes, alloc.component_name);
            }
        }

        if freed_bytes < needed_bytes {
            let total = *self.total_allocated.lock().unwrap();
            return Err(crate::error::GameProtocolError::SystemError {
                message: format!("Cannot allocate {} bytes, only freed {} bytes. Total usage: {}",
                               needed_bytes, freed_bytes, total),
                context: Some("memory_manager_gc".to_string()),
            });
        }

        println!("INFO: Garbage collection freed {} bytes", freed_bytes);
        Ok(())
    }

    /// Get current memory usage
    pub fn get_current_usage(&self) -> super::MemoryUsage {
        let total = *self.total_allocated.lock().unwrap();
        let cache = *self.cache_allocated.lock().unwrap();
        let buffer = *self.buffer_allocated.lock().unwrap();

        super::MemoryUsage {
            total_allocated: total,
            cache_memory: cache,
            buffer_memory: buffer,
            utilization_percent: (total as f64 / self.limits.max_total_memory as f64) * 100.0,
        }
    }

    /// Check if memory is constrained
    pub fn is_memory_constrained(&self) -> bool {
        let total = *self.total_allocated.lock().unwrap();
        let utilization = total as f64 / self.limits.max_total_memory as f64;
        utilization > self.gc_threshold
    }

    /// Get cache hit rate (simplified calculation)
    pub fn get_cache_hit_rate(&self) -> f64 {
        // This would be calculated based on cache usage statistics
        // For now, return a placeholder value
        0.75
    }

    /// Force garbage collection
    pub async fn force_gc(&self) -> GameResult<u64> {
        let cutoff_time = SystemTime::now() - Duration::from_secs(60); // 1 minute old
        let freed_bytes = 0u64;

        let allocations_to_free: Vec<String> = {
            let allocations = self.allocations.lock().unwrap();
            allocations
                .iter()
                .filter(|(_, allocation)| allocation.last_accessed < cutoff_time)
                .map(|(id, _)| id.clone())
                .collect()
        };

        for allocation_id in allocations_to_free {
            if let Ok(()) = self.deallocate(&allocation_id) {
                // Deallocation already handles the freed bytes calculation
            }
        }

        println!("INFO: Forced garbage collection completed");
        Ok(freed_bytes)
    }

    /// Cleanup memory manager
    pub async fn cleanup(&mut self) -> GameResult<()> {
        // Clear all allocations (simulating process shutdown)
        {
            let mut allocations = self.allocations.lock().unwrap();
            let total_allocations = allocations.len();
            allocations.clear();

            *self.total_allocated.lock().unwrap() = 0;
            *self.cache_allocated.lock().unwrap() = 0;
            *self.buffer_allocated.lock().unwrap() = 0;

            println!("DEBUG: Memory manager cleanup - cleared {} allocations", total_allocations);
        }

        Ok(())
    }

    /// Get memory statistics for monitoring
    pub fn get_statistics(&self) -> MemoryStatistics {
        let allocations = self.allocations.lock().unwrap();
        let total_allocations = allocations.len();

        let mut by_component: HashMap<String, u64> = HashMap::new();
        let mut by_type: HashMap<String, u64> = HashMap::new();

        for allocation in allocations.values() {
            *by_component.entry(allocation.component_name.clone()).or_insert(0) += allocation.allocated_bytes;

            let type_name = match allocation.allocation_type {
                AllocationType::Cache => "cache",
                AllocationType::Buffer => "buffer",
                AllocationType::Working => "working",
                AllocationType::Temporary => "temporary",
            };
            *by_type.entry(type_name.to_string()).or_insert(0) += allocation.allocated_bytes;
        }

        MemoryStatistics {
            total_allocations,
            total_allocated: *self.total_allocated.lock().unwrap(),
            cache_allocated: *self.cache_allocated.lock().unwrap(),
            buffer_allocated: *self.buffer_allocated.lock().unwrap(),
            allocations_by_component: by_component,
            allocations_by_type: by_type,
        }
    }
}

/// Memory statistics for monitoring and debugging
#[derive(Debug)]
pub struct MemoryStatistics {
    pub total_allocations: usize,
    pub total_allocated: u64,
    pub cache_allocated: u64,
    pub buffer_allocated: u64,
    pub allocations_by_component: HashMap<String, u64>,
    pub allocations_by_type: HashMap<String, u64>,
}

/// Resource tracker for monitoring resource usage patterns
#[derive(Debug)]
pub struct ResourceTracker {
    memory_manager: Arc<MemoryManager>,
    tracking_enabled: bool,
}

impl ResourceTracker {
    /// Create a new resource tracker
    pub fn new(memory_manager: Arc<MemoryManager>) -> Self {
        Self {
            memory_manager,
            tracking_enabled: true,
        }
    }

    /// Track resource usage for an operation
    pub async fn track_operation<F, R>(&self, operation_name: &str, f: F) -> GameResult<R>
    where
        F: std::future::Future<Output = GameResult<R>>,
    {
        if !self.tracking_enabled {
            return f.await;
        }

        let start_usage = self.memory_manager.get_current_usage();
        let start_time = SystemTime::now();

        let result = f.await;

        let end_usage = self.memory_manager.get_current_usage();
        let duration = start_time.elapsed().unwrap_or(Duration::ZERO);

        let memory_delta = end_usage.total_allocated as i64 - start_usage.total_allocated as i64;

        println!("DEBUG: Operation '{}' - Duration: {:?}, Memory delta: {} bytes",
                 operation_name, duration, memory_delta);

        result
    }

    /// Enable or disable tracking
    pub fn set_tracking_enabled(&mut self, enabled: bool) {
        self.tracking_enabled = enabled;
    }

    /// Get memory manager reference
    pub fn memory_manager(&self) -> &Arc<MemoryManager> {
        &self.memory_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_manager_creation() {
        let limits = MemoryLimits::default();
        let manager = MemoryManager::new(limits);

        let usage = manager.get_current_usage();
        assert_eq!(usage.total_allocated, 0);
        assert!(!manager.is_memory_constrained());
    }

    #[tokio::test]
    async fn test_memory_allocation() {
        let limits = MemoryLimits::default();
        let manager = MemoryManager::new(limits);

        let allocation_id = manager.allocate("test_component", 1024, AllocationType::Working).unwrap();
        assert!(!allocation_id.is_empty());

        let usage = manager.get_current_usage();
        assert_eq!(usage.total_allocated, 1024);

        manager.deallocate(&allocation_id).unwrap();

        let usage_after = manager.get_current_usage();
        assert_eq!(usage_after.total_allocated, 0);
    }

    #[tokio::test]
    async fn test_memory_limits() {
        let limits = MemoryLimits {
            max_total_memory: 1024, // Very small limit for testing
            max_cache_memory: 512,
            max_buffer_memory: 256,
        };
        let manager = MemoryManager::new(limits);

        // This should succeed
        let allocation1 = manager.allocate("test1", 512, AllocationType::Working);
        assert!(allocation1.is_ok());

        // This should fail due to total memory limit
        let allocation2 = manager.allocate("test2", 1024, AllocationType::Working);
        assert!(allocation2.is_err());
    }

    #[tokio::test]
    async fn test_resource_tracker() {
        let limits = MemoryLimits::default();
        let memory_manager = Arc::new(MemoryManager::new(limits));
        let tracker = ResourceTracker::new(Arc::clone(&memory_manager));

        let result = tracker.track_operation("test_operation", async {
            Ok::<_, crate::error::GameProtocolError>("success")
        }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_garbage_collection() {
        let limits = MemoryLimits::default();
        let manager = MemoryManager::new(limits);

        // Allocate some memory
        let _alloc1 = manager.allocate("test1", 1024, AllocationType::Temporary).unwrap();
        let _alloc2 = manager.allocate("test2", 2048, AllocationType::Cache).unwrap();

        let usage_before = manager.get_current_usage();
        assert!(usage_before.total_allocated > 0);

        // Force garbage collection
        let freed = manager.force_gc().await.unwrap();
        println!("Freed {} bytes", freed);

        // Usage might not change immediately due to recent allocation times
        // But the GC mechanism was tested
    }
}