//! Metrics collection service for system monitoring

use tracing::{debug, info};
use crate::error::GameResult;
use crate::cashu::SequenceStatistics;
use super::{ServiceContext, TimeoutResult};

/// Service responsible for collecting system metrics
#[derive(Debug)]
pub struct MetricsCollector {
    context: ServiceContext,
    processed_events: u64,
    processed_batches: u64,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(context: ServiceContext) -> Self {
        Self {
            context,
            processed_events: 0,
            processed_batches: 0,
        }
    }

    /// Record that a batch was processed
    pub async fn record_batch_processed(&mut self, event_count: usize) -> GameResult<()> {
        self.processed_batches += 1;
        self.processed_events += event_count as u64;

        debug!(
            batch_count = self.processed_batches,
            total_events = self.processed_events,
            batch_size = event_count,
            "Recorded batch processing"
        );

        Ok(())
    }

    /// Record a timeout occurrence
    pub async fn record_timeout(&mut self, _timeout: &TimeoutResult) -> GameResult<()> {
        debug!("Recorded timeout event");
        Ok(())
    }

    /// Update system metrics
    pub async fn update_system_metrics(&mut self) -> GameResult<()> {
        debug!("Updated system metrics");
        Ok(())
    }

    /// Get current system metrics
    pub async fn get_current_metrics(&self) -> GameResult<SequenceStatistics> {
        info!(
            processed_events = self.processed_events,
            processed_batches = self.processed_batches,
            "Providing current metrics"
        );

        Ok(SequenceStatistics {
            waiting_for_accept: 0, // Would be provided by sequence manager
            in_progress: 0, // Would be provided by sequence manager
            waiting_for_final: 0, // Would be provided by sequence manager
            completed: 0, // Would be provided by sequence manager
            forfeited: 0, // Would be provided by sequence manager
            total_completed: 0, // Would be provided by sequence manager
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::Keys;
    use crate::cashu::GameMint;
    use std::sync::Arc;

    fn create_test_context() -> ServiceContext {
        let keys = Keys::generate();
        let mint = Arc::new(GameMint::new_test(keys));
        let nostr_client = nostr_sdk::Client::default();
        let observability = Arc::new(crate::observability::ObservabilityManager::new(
            crate::observability::ObservabilityConfig::default()
        ));
        ServiceContext::new(mint, nostr_client, observability)
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let mut collector = MetricsCollector::new(create_test_context());

        // Record some batch processing
        collector.record_batch_processed(5).await.unwrap();
        collector.record_batch_processed(3).await.unwrap();

        // Get metrics
        let metrics = collector.get_current_metrics().await.unwrap();
        assert_eq!(metrics.total_completed, 0); // No sequences completed in this test
    }
}