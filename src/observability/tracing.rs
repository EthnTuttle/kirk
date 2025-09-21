//! Request tracing and correlation ID management

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use crate::error::GameResult;

/// Correlation context for tracking requests across service boundaries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationContext {
    /// Unique correlation ID for the request
    pub correlation_id: String,
    /// Parent correlation ID if this is a sub-request
    pub parent_id: Option<String>,
    /// User ID associated with the request
    pub user_id: Option<String>,
    /// Session ID associated with the request
    pub session_id: Option<String>,
    /// Operation being performed
    pub operation: String,
    /// Request start timestamp
    pub start_time: u64,
    /// Additional context metadata
    pub metadata: HashMap<String, String>,
}

impl CorrelationContext {
    /// Create a new correlation context
    pub fn new(operation: &str) -> Self {
        Self {
            correlation_id: Uuid::new_v4().to_string(),
            parent_id: None,
            user_id: None,
            session_id: None,
            operation: operation.to_string(),
            start_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs(),
            metadata: HashMap::new(),
        }
    }

    /// Create a child correlation context
    pub fn child(&self, operation: &str) -> Self {
        Self {
            correlation_id: Uuid::new_v4().to_string(),
            parent_id: Some(self.correlation_id.clone()),
            user_id: self.user_id.clone(),
            session_id: self.session_id.clone(),
            operation: operation.to_string(),
            start_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs(),
            metadata: self.metadata.clone(),
        }
    }

    /// Set user ID
    pub fn with_user_id(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self
    }

    /// Set session ID
    pub fn with_session_id(mut self, session_id: &str) -> Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// Get elapsed time since start
    pub fn elapsed(&self) -> Duration {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        Duration::from_secs(now.saturating_sub(self.start_time))
    }
}

/// Span information for tracing operations
#[derive(Debug, Clone)]
pub struct TraceSpan {
    /// Span ID
    pub span_id: String,
    /// Correlation context
    pub context: CorrelationContext,
    /// Start time
    pub start_time: SystemTime,
    /// End time (if completed)
    pub end_time: Option<SystemTime>,
    /// Operation result (success/failure)
    pub success: Option<bool>,
    /// Error message if failed
    pub error: Option<String>,
    /// Child span IDs
    pub children: Vec<String>,
}

impl TraceSpan {
    /// Create a new trace span
    pub fn new(context: CorrelationContext) -> Self {
        Self {
            span_id: Uuid::new_v4().to_string(),
            context,
            start_time: SystemTime::now(),
            end_time: None,
            success: None,
            error: None,
            children: Vec::new(),
        }
    }

    /// Complete the span successfully
    pub fn complete_success(mut self) -> Self {
        self.end_time = Some(SystemTime::now());
        self.success = Some(true);
        self
    }

    /// Complete the span with failure
    pub fn complete_failure(mut self, error: &str) -> Self {
        self.end_time = Some(SystemTime::now());
        self.success = Some(false);
        self.error = Some(error.to_string());
        self
    }

    /// Add child span
    pub fn add_child(&mut self, child_span_id: &str) {
        self.children.push(child_span_id.to_string());
    }

    /// Get duration of the span
    pub fn duration(&self) -> Option<Duration> {
        self.end_time.map(|end| {
            end.duration_since(self.start_time)
                .unwrap_or(Duration::ZERO)
        })
    }

    /// Check if span is completed
    pub fn is_completed(&self) -> bool {
        self.end_time.is_some()
    }
}

/// Main request tracing system
#[derive(Debug)]
pub struct RequestTracing {
    /// Active correlation contexts
    active_contexts: Arc<Mutex<HashMap<String, CorrelationContext>>>,
    /// Active trace spans
    active_spans: Arc<Mutex<HashMap<String, TraceSpan>>>,
    /// Completed spans (for audit and debugging)
    completed_spans: Arc<Mutex<HashMap<String, TraceSpan>>>,
    /// Maximum number of correlation IDs to track
    max_correlations: usize,
}

impl RequestTracing {
    /// Create new request tracing system
    pub fn new(max_correlations: usize) -> Self {
        Self {
            active_contexts: Arc::new(Mutex::new(HashMap::new())),
            active_spans: Arc::new(Mutex::new(HashMap::new())),
            completed_spans: Arc::new(Mutex::new(HashMap::new())),
            max_correlations,
        }
    }

    /// Initialize the tracing system
    pub async fn initialize(&mut self) -> GameResult<()> {
        tracing::info!(
            max_correlations = self.max_correlations,
            "Request tracing initialized"
        );
        Ok(())
    }

    /// Start a new traced operation
    pub fn start_operation(&self, operation: &str) -> CorrelationContext {
        let context = CorrelationContext::new(operation);

        if let Ok(mut contexts) = self.active_contexts.lock() {
            // Clean up old contexts if we're at the limit
            if contexts.len() >= self.max_correlations {
                self.cleanup_old_contexts(&mut contexts);
            }

            contexts.insert(context.correlation_id.clone(), context.clone());
        }

        tracing::info!(
            correlation_id = %context.correlation_id,
            operation = operation,
            "Started traced operation"
        );

        context
    }

    /// Start a child operation
    pub fn start_child_operation(&self, parent_context: &CorrelationContext, operation: &str) -> CorrelationContext {
        let child_context = parent_context.child(operation);

        if let Ok(mut contexts) = self.active_contexts.lock() {
            contexts.insert(child_context.correlation_id.clone(), child_context.clone());
        }

        tracing::info!(
            correlation_id = %child_context.correlation_id,
            parent_id = %parent_context.correlation_id,
            operation = operation,
            "Started child traced operation"
        );

        child_context
    }

    /// Begin a trace span
    pub fn begin_span(&self, context: &CorrelationContext) -> String {
        let span = TraceSpan::new(context.clone());
        let span_id = span.span_id.clone();

        if let Ok(mut spans) = self.active_spans.lock() {
            spans.insert(span_id.clone(), span);
        }

        tracing::debug!(
            span_id = %span_id,
            correlation_id = %context.correlation_id,
            operation = %context.operation,
            "Began trace span"
        );

        span_id
    }

    /// End a trace span successfully
    pub fn end_span_success(&self, span_id: &str) {
        if let Ok(mut active_spans) = self.active_spans.lock() {
            if let Some(span) = active_spans.remove(span_id) {
                let completed_span = span.complete_success();
                let duration = completed_span.duration();

                if let Ok(mut completed_spans) = self.completed_spans.lock() {
                    completed_spans.insert(span_id.to_string(), completed_span);
                }

                tracing::debug!(
                    span_id = %span_id,
                    duration_ms = duration.map(|d| d.as_millis()).unwrap_or(0),
                    "Completed trace span successfully"
                );
            }
        }
    }

    /// End a trace span with failure
    pub fn end_span_failure(&self, span_id: &str, error: &str) {
        if let Ok(mut active_spans) = self.active_spans.lock() {
            if let Some(span) = active_spans.remove(span_id) {
                let completed_span = span.complete_failure(error);
                let duration = completed_span.duration();

                if let Ok(mut completed_spans) = self.completed_spans.lock() {
                    completed_spans.insert(span_id.to_string(), completed_span);
                }

                tracing::warn!(
                    span_id = %span_id,
                    error = error,
                    duration_ms = duration.map(|d| d.as_millis()).unwrap_or(0),
                    "Completed trace span with failure"
                );
            }
        }
    }

    /// Complete an operation
    pub fn complete_operation(&self, context: &CorrelationContext, success: bool, error: Option<&str>) {
        let duration = context.elapsed();

        if let Ok(mut contexts) = self.active_contexts.lock() {
            contexts.remove(&context.correlation_id);
        }

        if success {
            tracing::info!(
                correlation_id = %context.correlation_id,
                operation = %context.operation,
                duration_ms = duration.as_millis(),
                user_id = context.user_id.as_deref(),
                session_id = context.session_id.as_deref(),
                "Completed traced operation successfully"
            );
        } else {
            tracing::error!(
                correlation_id = %context.correlation_id,
                operation = %context.operation,
                duration_ms = duration.as_millis(),
                error = error.unwrap_or("Unknown error"),
                user_id = context.user_id.as_deref(),
                session_id = context.session_id.as_deref(),
                "Completed traced operation with failure"
            );
        }
    }

    /// Get active correlation context by ID
    pub fn get_context(&self, correlation_id: &str) -> Option<CorrelationContext> {
        self.active_contexts
            .lock()
            .ok()?
            .get(correlation_id)
            .cloned()
    }

    /// Get trace span by ID
    pub fn get_span(&self, span_id: &str) -> Option<TraceSpan> {
        // Check active spans first
        if let Ok(active_spans) = self.active_spans.lock() {
            if let Some(span) = active_spans.get(span_id) {
                return Some(span.clone());
            }
        }

        // Check completed spans
        self.completed_spans
            .lock()
            .ok()?
            .get(span_id)
            .cloned()
    }

    /// Get statistics about tracing
    pub fn get_statistics(&self) -> TracingStatistics {
        let active_contexts = self.active_contexts.lock().map(|c| c.len()).unwrap_or(0);
        let active_spans = self.active_spans.lock().map(|s| s.len()).unwrap_or(0);
        let completed_spans = self.completed_spans.lock().map(|s| s.len()).unwrap_or(0);

        TracingStatistics {
            active_contexts,
            active_spans,
            completed_spans,
            max_correlations: self.max_correlations,
        }
    }

    /// Cleanup old correlation contexts
    fn cleanup_old_contexts(&self, contexts: &mut HashMap<String, CorrelationContext>) {
        let cutoff_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs() - 3600; // Remove contexts older than 1 hour

        let old_keys: Vec<String> = contexts
            .iter()
            .filter(|(_, context)| context.start_time < cutoff_time)
            .map(|(key, _)| key.clone())
            .collect();

        for key in old_keys {
            contexts.remove(&key);
        }
    }

    /// Cleanup old correlations and spans
    pub async fn cleanup_old_correlations(&mut self) -> GameResult<()> {
        let cutoff_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs() - 3600; // 1 hour

        // Cleanup old contexts
        if let Ok(mut contexts) = self.active_contexts.lock() {
            self.cleanup_old_contexts(&mut contexts);
        }

        // Cleanup old completed spans
        if let Ok(mut completed_spans) = self.completed_spans.lock() {
            let old_keys: Vec<String> = completed_spans
                .iter()
                .filter(|(_, span)| {
                    span.start_time
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or(Duration::ZERO)
                        .as_secs() < cutoff_time
                })
                .map(|(key, _)| key.clone())
                .collect();

            for key in old_keys {
                completed_spans.remove(&key);
            }
        }

        tracing::debug!("Cleaned up old correlation data");
        Ok(())
    }
}

/// Statistics about the tracing system
#[derive(Debug, Clone)]
pub struct TracingStatistics {
    pub active_contexts: usize,
    pub active_spans: usize,
    pub completed_spans: usize,
    pub max_correlations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_correlation_context() {
        let context = CorrelationContext::new("test_operation")
            .with_user_id("user123")
            .with_session_id("session456")
            .with_metadata("key", "value");

        assert_eq!(context.operation, "test_operation");
        assert_eq!(context.user_id.unwrap(), "user123");
        assert_eq!(context.session_id.unwrap(), "session456");
        assert_eq!(context.metadata.get("key").unwrap(), "value");
        assert!(context.parent_id.is_none());

        let child = context.child("child_operation");
        assert_eq!(child.operation, "child_operation");
        assert_eq!(child.parent_id.unwrap(), context.correlation_id);
        assert_eq!(child.user_id.unwrap(), "user123"); // Inherited
    }

    #[tokio::test]
    async fn test_trace_span() {
        let context = CorrelationContext::new("test");
        let span = TraceSpan::new(context);

        assert!(!span.is_completed());
        assert!(span.duration().is_none());
        assert!(span.children.is_empty());

        let completed = span.complete_success();
        assert!(completed.is_completed());
        assert!(completed.success.unwrap());
        assert!(completed.duration().is_some());
    }

    #[tokio::test]
    async fn test_request_tracing() {
        let mut tracing = RequestTracing::new(100);
        tracing.initialize().await.unwrap();

        let context = tracing.start_operation("test_op");
        assert_eq!(context.operation, "test_op");

        let span_id = tracing.begin_span(&context);
        let span = tracing.get_span(&span_id).unwrap();
        assert!(!span.is_completed());

        tracing.end_span_success(&span_id);
        let completed_span = tracing.get_span(&span_id).unwrap();
        assert!(completed_span.is_completed());
        assert!(completed_span.success.unwrap());

        tracing.complete_operation(&context, true, None);

        let stats = tracing.get_statistics();
        assert_eq!(stats.completed_spans, 1);
    }

    #[tokio::test]
    async fn test_child_operations() {
        let tracing = RequestTracing::new(100);

        let parent_context = tracing.start_operation("parent");
        let child_context = tracing.start_child_operation(&parent_context, "child");

        assert_eq!(child_context.parent_id.unwrap(), parent_context.correlation_id);
        assert_eq!(child_context.operation, "child");
    }

    #[tokio::test]
    async fn test_cleanup() {
        let mut tracing = RequestTracing::new(100);
        let _context = tracing.start_operation("test");

        let result = tracing.cleanup_old_correlations().await;
        assert!(result.is_ok());
    }
}