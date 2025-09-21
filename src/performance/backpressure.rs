//! Backpressure handling and load balancing for production resilience

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use crate::error::GameResult;

/// Configuration for backpressure handling
#[derive(Debug, Clone)]
pub struct BackpressureConfig {
    /// Maximum queue size before applying backpressure
    pub max_queue_size: usize,
    /// Maximum processing rate (requests per second)
    pub max_processing_rate: u32,
    /// Backpressure timeout in milliseconds
    pub backpressure_timeout_ms: u64,
    /// Circuit breaker failure threshold
    pub failure_threshold: u32,
    /// Circuit breaker recovery timeout
    pub recovery_timeout: Duration,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 1000,
            max_processing_rate: 100,
            backpressure_timeout_ms: 5000, // 5 seconds
            failure_threshold: 10,
            recovery_timeout: Duration::from_secs(60),
        }
    }
}

/// Request priority levels for load balancing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RequestPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Queued request with metadata
#[derive(Debug)]
pub struct QueuedRequest<T> {
    pub data: T,
    pub priority: RequestPriority,
    pub submitted_at: Instant,
    pub deadline: Option<Instant>,
}

impl<T> QueuedRequest<T> {
    pub fn new(data: T, priority: RequestPriority) -> Self {
        Self {
            data,
            priority,
            submitted_at: Instant::now(),
            deadline: None,
        }
    }

    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    pub fn is_expired(&self) -> bool {
        if let Some(deadline) = self.deadline {
            Instant::now() > deadline
        } else {
            false
        }
    }

    pub fn waiting_time(&self) -> Duration {
        self.submitted_at.elapsed()
    }
}

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    Closed,    // Normal operation
    Open,      // Failing, reject requests
    HalfOpen,  // Testing if service recovered
}

/// Circuit breaker for handling cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    last_failure_time: Option<Instant>,
    config: BackpressureConfig,
}

impl CircuitBreaker {
    pub fn new(config: BackpressureConfig) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            last_failure_time: None,
            config,
        }
    }

    pub fn call_permitted(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    if last_failure.elapsed() >= self.config.recovery_timeout {
                        self.state = CircuitState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.state = CircuitState::Closed;
        self.last_failure_time = None;
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(Instant::now());

        if self.failure_count >= self.config.failure_threshold {
            self.state = CircuitState::Open;
        }
    }

    pub fn state(&self) -> CircuitState {
        self.state
    }
}

/// Rate limiter using token bucket algorithm
#[derive(Debug)]
pub struct TokenBucket {
    capacity: u32,
    tokens: Arc<Mutex<u32>>,
    refill_rate: u32, // tokens per second
    last_refill: Arc<Mutex<Instant>>,
}

impl TokenBucket {
    pub fn new(capacity: u32, refill_rate: u32) -> Self {
        Self {
            capacity,
            tokens: Arc::new(Mutex::new(capacity)),
            refill_rate,
            last_refill: Arc::new(Mutex::new(Instant::now())),
        }
    }

    pub fn try_consume(&self, tokens_needed: u32) -> bool {
        self.refill_tokens();

        if let Ok(mut tokens) = self.tokens.lock() {
            if *tokens >= tokens_needed {
                *tokens -= tokens_needed;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    fn refill_tokens(&self) {
        let now = Instant::now();

        if let (Ok(mut tokens), Ok(mut last_refill)) =
            (self.tokens.lock(), self.last_refill.lock()) {

            let elapsed = now.duration_since(*last_refill);
            let new_tokens = ((elapsed.as_secs_f64() * self.refill_rate as f64) as u32)
                .min(self.capacity - *tokens);

            *tokens = (*tokens + new_tokens).min(self.capacity);
            *last_refill = now;
        }
    }

    pub fn available_tokens(&self) -> u32 {
        self.refill_tokens();
        self.tokens.lock().map(|t| *t).unwrap_or(0)
    }
}

/// Priority queue for handling requests with different priorities
#[derive(Debug)]
pub struct PriorityQueue<T> {
    queues: [VecDeque<QueuedRequest<T>>; 4], // One for each priority level
    total_size: usize,
}

impl<T> PriorityQueue<T> {
    pub fn new() -> Self {
        Self {
            queues: [
                VecDeque::new(), // Low
                VecDeque::new(), // Normal
                VecDeque::new(), // High
                VecDeque::new(), // Critical
            ],
            total_size: 0,
        }
    }

    pub fn push(&mut self, request: QueuedRequest<T>) {
        let priority_index = request.priority as usize;
        self.queues[priority_index].push_back(request);
        self.total_size += 1;
    }

    pub fn pop(&mut self) -> Option<QueuedRequest<T>> {
        // Process highest priority first
        for queue in self.queues.iter_mut().rev() {
            if let Some(request) = queue.pop_front() {
                self.total_size = self.total_size.saturating_sub(1);
                return Some(request);
            }
        }
        None
    }

    pub fn len(&self) -> usize {
        self.total_size
    }

    pub fn is_empty(&self) -> bool {
        self.total_size == 0
    }

    pub fn remove_expired(&mut self) -> usize {
        let mut removed = 0;

        for queue in &mut self.queues {
            let initial_len = queue.len();
            queue.retain(|req| !req.is_expired());
            removed += initial_len - queue.len();
        }

        self.total_size = self.total_size.saturating_sub(removed);
        removed
    }
}

/// Main backpressure handler
#[derive(Debug)]
pub struct BackpressureHandler {
    config: BackpressureConfig,
    request_queue: Arc<Mutex<PriorityQueue<Vec<u8>>>>, // Generic byte payload
    processing_semaphore: Semaphore,
    rate_limiter: TokenBucket,
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    statistics: Arc<Mutex<BackpressureStatistics>>,
}

#[derive(Debug, Clone)]
struct BackpressureStatistics {
    requests_queued: u64,
    requests_processed: u64,
    requests_dropped: u64,
    requests_timeout: u64,
    avg_queue_time_ms: f64,
    current_queue_size: usize,
}

impl Default for BackpressureStatistics {
    fn default() -> Self {
        Self {
            requests_queued: 0,
            requests_processed: 0,
            requests_dropped: 0,
            requests_timeout: 0,
            avg_queue_time_ms: 0.0,
            current_queue_size: 0,
        }
    }
}

impl BackpressureHandler {
    pub fn new(max_queue_size: usize, max_processing_rate: u32) -> Self {
        let config = BackpressureConfig {
            max_queue_size,
            max_processing_rate,
            ..Default::default()
        };

        Self {
            processing_semaphore: Semaphore::new(max_processing_rate as usize),
            rate_limiter: TokenBucket::new(max_processing_rate, max_processing_rate),
            circuit_breaker: Arc::new(Mutex::new(CircuitBreaker::new(config.clone()))),
            request_queue: Arc::new(Mutex::new(PriorityQueue::new())),
            statistics: Arc::new(Mutex::new(BackpressureStatistics::default())),
            config,
        }
    }

    pub async fn initialize(&mut self) -> GameResult<()> {
        println!("INFO: Backpressure handler initialized - Max queue: {}, Rate limit: {} req/s",
                 self.config.max_queue_size, self.config.max_processing_rate);
        Ok(())
    }

    pub fn is_overloaded(&self) -> bool {
        let queue_size = self.request_queue.lock()
            .map(|q| q.len())
            .unwrap_or(0);

        queue_size > self.config.max_queue_size / 2 || // 50% queue capacity
        self.rate_limiter.available_tokens() == 0
    }

    pub async fn apply_backpressure(&self) -> GameResult<bool> {
        // Check circuit breaker
        {
            let mut breaker = self.circuit_breaker.lock().unwrap();
            if !breaker.call_permitted() {
                return Err(crate::error::GameProtocolError::SystemError {
                    message: "Circuit breaker is open - rejecting requests".to_string(),
                    context: Some("backpressure_handler".to_string()),
                });
            }
        }

        // Apply rate limiting
        if !self.rate_limiter.try_consume(1) {
            if let Ok(mut stats) = self.statistics.lock() {
                stats.requests_dropped += 1;
            }

            return Err(crate::error::GameProtocolError::SystemError {
                message: "Rate limit exceeded - request dropped".to_string(),
                context: Some("rate_limiter".to_string()),
            });
        }

        // Check queue capacity
        let queue_size = self.request_queue.lock()
            .map(|q| q.len())
            .unwrap_or(0);

        if queue_size >= self.config.max_queue_size {
            if let Ok(mut stats) = self.statistics.lock() {
                stats.requests_dropped += 1;
            }

            return Err(crate::error::GameProtocolError::SystemError {
                message: format!("Queue full ({} requests) - dropping request", queue_size),
                context: Some("queue_overflow".to_string()),
            });
        }

        // Wait for processing slot
        let _permit = tokio::time::timeout(
            Duration::from_millis(self.config.backpressure_timeout_ms),
            self.processing_semaphore.acquire()
        ).await
        .map_err(|_| crate::error::GameProtocolError::SystemError {
            message: "Backpressure timeout - request rejected".to_string(),
            context: Some("semaphore_timeout".to_string()),
        })?
        .map_err(|_| crate::error::GameProtocolError::SystemError {
            message: "Failed to acquire processing semaphore".to_string(),
            context: Some("semaphore_error".to_string()),
        })?;

        Ok(true)
    }

    pub fn get_statistics(&self) -> super::BackpressureStats {
        if let Ok(mut stats) = self.statistics.lock() {
            let queue_size = self.request_queue.lock()
                .map(|q| q.len())
                .unwrap_or(0);

            stats.current_queue_size = queue_size;

            super::BackpressureStats {
                queue_size,
                processing_rate: self.config.max_processing_rate as f64,
                dropped_requests: stats.requests_dropped,
                backpressure_active: self.is_overloaded(),
            }
        } else {
            super::BackpressureStats {
                queue_size: 0,
                processing_rate: 0.0,
                dropped_requests: 0,
                backpressure_active: false,
            }
        }
    }

    pub fn record_success(&self) {
        if let Ok(mut breaker) = self.circuit_breaker.lock() {
            breaker.record_success();
        }

        if let Ok(mut stats) = self.statistics.lock() {
            stats.requests_processed += 1;
        }
    }

    pub fn record_failure(&self) {
        if let Ok(mut breaker) = self.circuit_breaker.lock() {
            breaker.record_failure();
        }
    }

    pub async fn cleanup(&mut self) -> GameResult<()> {
        // Clean up expired requests
        let expired_count = {
            let mut queue = self.request_queue.lock().unwrap();
            queue.remove_expired()
        };

        if expired_count > 0 {
            if let Ok(mut stats) = self.statistics.lock() {
                stats.requests_timeout += expired_count as u64;
            }
            println!("DEBUG: Cleaned up {} expired requests", expired_count);
        }

        Ok(())
    }
}

/// Load balancer for distributing requests across multiple processors
#[derive(Debug)]
pub struct LoadBalancer {
    processors: Vec<String>,
    current_index: Arc<Mutex<usize>>,
    health_status: Arc<Mutex<Vec<bool>>>,
}

impl LoadBalancer {
    pub fn new(processors: Vec<String>) -> Self {
        let processor_count = processors.len();
        Self {
            processors,
            current_index: Arc::new(Mutex::new(0)),
            health_status: Arc::new(Mutex::new(vec![true; processor_count])),
        }
    }

    pub fn next_processor(&self) -> Option<String> {
        if let (Ok(mut index), Ok(health)) =
            (self.current_index.lock(), self.health_status.lock()) {

            let start_index = *index;

            loop {
                if health[*index] {
                    let processor = self.processors[*index].clone();
                    *index = (*index + 1) % self.processors.len();
                    return Some(processor);
                }

                *index = (*index + 1) % self.processors.len();

                // If we've checked all processors, none are healthy
                if *index == start_index {
                    break;
                }
            }
        }

        None
    }

    pub fn mark_processor_health(&self, processor: &str, healthy: bool) {
        if let (Some(pos), Ok(mut health)) =
            (self.processors.iter().position(|p| p == processor), self.health_status.lock()) {
            health[pos] = healthy;
        }
    }

    pub fn healthy_processors(&self) -> usize {
        self.health_status.lock()
            .map(|health| health.iter().filter(|&&h| h).count())
            .unwrap_or(0)
    }
}

/// Rate-limited processor wrapper
pub struct RateLimitedProcessor<T> {
    processor: T,
    rate_limiter: TokenBucket,
}

impl<T> RateLimitedProcessor<T> {
    pub fn new(processor: T, rate_limit: u32) -> Self {
        Self {
            processor,
            rate_limiter: TokenBucket::new(rate_limit, rate_limit),
        }
    }

    pub async fn process<F, R>(&self, f: F) -> GameResult<R>
    where
        F: FnOnce(&T) -> R,
    {
        if !self.rate_limiter.try_consume(1) {
            tokio::time::sleep(Duration::from_millis(100)).await;

            if !self.rate_limiter.try_consume(1) {
                return Err(crate::error::GameProtocolError::SystemError {
                    message: "Rate limit exceeded for processor".to_string(),
                    context: Some("rate_limited_processor".to_string()),
                });
            }
        }

        Ok(f(&self.processor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker() {
        let config = BackpressureConfig::default();
        let mut breaker = CircuitBreaker::new(config);

        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(breaker.call_permitted());

        // Record failures to trip the breaker
        for _ in 0..10 {
            breaker.record_failure();
        }

        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.call_permitted());

        // Record success to reset
        breaker.record_success();
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_token_bucket() {
        let bucket = TokenBucket::new(10, 5); // 10 capacity, 5 tokens/sec

        // Should be able to consume initial tokens
        assert!(bucket.try_consume(5));
        assert!(bucket.try_consume(5));
        assert!(!bucket.try_consume(1)); // Should fail, bucket empty

        // Wait a bit for refill (in real usage)
        // For testing, we just verify the mechanism works
        assert!(bucket.available_tokens() <= 10);
    }

    #[test]
    fn test_priority_queue() {
        let mut queue = PriorityQueue::new();

        queue.push(QueuedRequest::new("low", RequestPriority::Low));
        queue.push(QueuedRequest::new("high", RequestPriority::High));
        queue.push(QueuedRequest::new("normal", RequestPriority::Normal));
        queue.push(QueuedRequest::new("critical", RequestPriority::Critical));

        // Should dequeue in priority order
        assert_eq!(queue.pop().unwrap().data, "critical");
        assert_eq!(queue.pop().unwrap().data, "high");
        assert_eq!(queue.pop().unwrap().data, "normal");
        assert_eq!(queue.pop().unwrap().data, "low");
        assert!(queue.is_empty());
    }

    #[tokio::test]
    async fn test_backpressure_handler() {
        let handler = BackpressureHandler::new(100, 10);

        assert!(!handler.is_overloaded());

        let stats = handler.get_statistics();
        assert_eq!(stats.queue_size, 0);
        assert!(!stats.backpressure_active);
    }

    #[test]
    fn test_load_balancer() {
        let processors = vec![
            "processor1".to_string(),
            "processor2".to_string(),
            "processor3".to_string(),
        ];

        let balancer = LoadBalancer::new(processors);
        assert_eq!(balancer.healthy_processors(), 3);

        // Should round-robin through processors
        assert_eq!(balancer.next_processor(), Some("processor1".to_string()));
        assert_eq!(balancer.next_processor(), Some("processor2".to_string()));
        assert_eq!(balancer.next_processor(), Some("processor3".to_string()));
        assert_eq!(balancer.next_processor(), Some("processor1".to_string()));

        // Mark one processor as unhealthy
        balancer.mark_processor_health("processor2", false);
        assert_eq!(balancer.healthy_processors(), 2);
    }
}