//! Modular service components for the Kirk gaming protocol
//!
//! This module breaks down the monolithic SequenceProcessor into focused,
//! maintainable service components with clear responsibilities.

pub mod event_processor;
pub mod sequence_manager;
pub mod fraud_detector;
pub mod reward_distributor;
pub mod timeout_manager;
pub mod metrics_collector;

// Re-export main service interfaces
pub use event_processor::EventProcessor;
pub use sequence_manager::SequenceManager;
pub use fraud_detector::FraudDetector;
pub use reward_distributor::RewardDistributor;
pub use timeout_manager::TimeoutManager;
pub use metrics_collector::MetricsCollector;

use std::sync::Arc;
use nostr::{Event, EventId, PublicKey};
use nostr_sdk::Client as NostrClient;
use crate::error::GameResult;
use crate::game::GameSequence;
use crate::cashu::GameMint;
use crate::observability::ObservabilityManager;

/// Configuration constants extracted from magic numbers
#[derive(Debug, Clone)]
pub struct ServiceConstants {
    /// Default final event timeout in seconds
    pub default_final_event_timeout: u64,
    /// Default move timeout in seconds
    pub default_move_timeout: u64,
    /// Maximum batch size for event processing
    pub max_batch_size: usize,
    /// Maximum concurrent sequences per client
    pub max_concurrent_sequences: usize,
    /// Statistics collection interval in seconds
    pub stats_collection_interval: u64,
    /// Cleanup interval for completed sequences
    pub cleanup_interval: u64,
}

impl Default for ServiceConstants {
    fn default() -> Self {
        Self {
            default_final_event_timeout: 3600, // 1 hour
            default_move_timeout: 1800,        // 30 minutes
            max_batch_size: 100,
            max_concurrent_sequences: 10,
            stats_collection_interval: 300,    // 5 minutes
            cleanup_interval: 86400,           // 24 hours
        }
    }
}

/// Shared service context containing dependencies and configuration
#[derive(Debug, Clone)]
pub struct ServiceContext {
    pub mint: Arc<GameMint>,
    pub nostr_client: NostrClient,
    pub constants: ServiceConstants,
    pub observability: Arc<ObservabilityManager>,
}

impl ServiceContext {
    pub fn new(mint: Arc<GameMint>, nostr_client: NostrClient, observability: Arc<ObservabilityManager>) -> Self {
        Self {
            mint,
            nostr_client,
            constants: ServiceConstants::default(),
            observability,
        }
    }

    pub fn with_constants(mut self, constants: ServiceConstants) -> Self {
        self.constants = constants;
        self
    }
}

/// Processing result from service operations
#[derive(Debug, Clone)]
pub enum ServiceResult {
    /// Event was processed successfully
    EventProcessed {
        event_id: EventId,
        sequence_id: EventId,
        action_taken: String,
    },
    /// Sequence was completed
    SequenceCompleted {
        sequence_id: EventId,
        winner: Option<PublicKey>,
        rewards_distributed: bool,
    },
    /// Fraud was detected
    FraudDetected {
        sequence_id: EventId,
        fraudulent_player: PublicKey,
        violation_type: String,
    },
    /// Timeout occurred
    TimeoutOccurred {
        sequence_id: EventId,
        timeout_type: String,
        affected_player: Option<PublicKey>,
    },
    /// No action taken
    NoAction {
        reason: String,
    },
}

impl ServiceResult {
    /// Get the sequence ID associated with this result, if any
    pub fn sequence_id(&self) -> Option<EventId> {
        match self {
            ServiceResult::EventProcessed { sequence_id, .. } => Some(*sequence_id),
            ServiceResult::SequenceCompleted { sequence_id, .. } => Some(*sequence_id),
            ServiceResult::FraudDetected { sequence_id, .. } => Some(*sequence_id),
            ServiceResult::TimeoutOccurred { sequence_id, .. } => Some(*sequence_id),
            ServiceResult::NoAction { .. } => None,
        }
    }

    /// Check if this result indicates a successful operation
    pub fn is_success(&self) -> bool {
        matches!(self,
            ServiceResult::EventProcessed { .. } |
            ServiceResult::SequenceCompleted { .. }
        )
    }

    /// Check if this result indicates an error or problematic condition
    pub fn is_error(&self) -> bool {
        matches!(self,
            ServiceResult::FraudDetected { .. } |
            ServiceResult::TimeoutOccurred { .. }
        )
    }
}

/// Main orchestrator service that coordinates all sub-services
#[derive(Debug)]
pub struct GameService {
    context: ServiceContext,
    event_processor: EventProcessor,
    sequence_manager: SequenceManager,
    fraud_detector: FraudDetector,
    reward_distributor: RewardDistributor,
    timeout_manager: TimeoutManager,
    metrics_collector: MetricsCollector,
}

impl GameService {
    /// Create a new game service with all components
    pub fn new(context: ServiceContext) -> Self {
        let event_processor = EventProcessor::new(context.clone());
        let sequence_manager = SequenceManager::new(context.clone());
        let fraud_detector = FraudDetector::new(context.clone());
        let reward_distributor = RewardDistributor::new(context.clone());
        let timeout_manager = TimeoutManager::new(context.clone());
        let metrics_collector = MetricsCollector::new(context.clone());

        Self {
            context,
            event_processor,
            sequence_manager,
            fraud_detector,
            reward_distributor,
            timeout_manager,
            metrics_collector,
        }
    }

    /// Process a batch of events through the service pipeline
    pub async fn process_events(&mut self, events: Vec<Event>) -> GameResult<Vec<ServiceResult>> {
        let mut results = Vec::new();

        // Validate batch size
        if events.len() > self.context.constants.max_batch_size {
            return Err(crate::error::GameProtocolError::Configuration {
                message: format!("Batch size {} exceeds maximum {}", events.len(), self.context.constants.max_batch_size),
                field: "batch_size".to_string(),
            });
        }

        for event in events {
            // Process each event through the pipeline
            let result = self.process_single_event(event).await?;
            results.push(result);
        }

        // Update metrics
        self.metrics_collector.record_batch_processed(results.len()).await?;

        Ok(results)
    }

    /// Process a single event through the service pipeline
    async fn process_single_event(&mut self, event: Event) -> GameResult<ServiceResult> {
        // 1. Parse and validate the event
        let parsed_event = self.event_processor.parse_event(&event).await?;

        // 2. Update sequence state
        let sequence_update = self.sequence_manager.handle_event(&event, &parsed_event).await?;

        // 3. Check for fraud if applicable
        if let Some(sequence) = sequence_update.sequence.as_ref() {
            if let Some(fraud_result) = self.fraud_detector.check_event(&event, sequence).await? {
                return Ok(ServiceResult::FraudDetected {
                    sequence_id: sequence_update.sequence_id,
                    fraudulent_player: fraud_result.player,
                    violation_type: fraud_result.violation_type,
                });
            }
        }

        // 4. Distribute rewards if sequence is complete
        if sequence_update.is_complete {
            if let Some(sequence) = sequence_update.sequence {
                let reward_result = self.reward_distributor.distribute_rewards(&sequence).await?;
                return Ok(ServiceResult::SequenceCompleted {
                    sequence_id: sequence_update.sequence_id,
                    winner: reward_result.winner,
                    rewards_distributed: reward_result.success,
                });
            }
        }

        // 5. Check for timeouts
        let timeout_result = self.timeout_manager.check_timeouts_for_event(&event).await?;
        if let Some(timeout) = timeout_result {
            return Ok(ServiceResult::TimeoutOccurred {
                sequence_id: timeout.sequence_id,
                timeout_type: timeout.timeout_type,
                affected_player: timeout.affected_player,
            });
        }

        // 6. Return successful processing result
        Ok(ServiceResult::EventProcessed {
            event_id: event.id,
            sequence_id: sequence_update.sequence_id,
            action_taken: sequence_update.action_taken,
        })
    }

    /// Get system metrics and statistics
    pub async fn get_metrics(&self) -> GameResult<crate::cashu::SequenceStatistics> {
        self.metrics_collector.get_current_metrics().await
    }

    /// Perform periodic maintenance tasks
    pub async fn perform_maintenance(&mut self) -> GameResult<()> {
        // Clean up old sequences
        self.sequence_manager.cleanup_old_sequences().await?;

        // Check for global timeouts
        let timeout_results = self.timeout_manager.check_all_timeouts().await?;
        for timeout in timeout_results {
            self.metrics_collector.record_timeout(&timeout).await?;
        }

        // Update metrics
        self.metrics_collector.update_system_metrics().await?;

        Ok(())
    }
}

/// Result of updating a sequence
#[derive(Debug)]
pub struct SequenceUpdate {
    pub sequence_id: EventId,
    pub sequence: Option<GameSequence>,
    pub is_complete: bool,
    pub action_taken: String,
}

/// Result of fraud detection
#[derive(Debug)]
pub struct FraudResult {
    pub player: PublicKey,
    pub violation_type: String,
    pub evidence: String,
}

/// Result of reward distribution
#[derive(Debug)]
pub struct RewardResult {
    pub winner: Option<PublicKey>,
    pub success: bool,
    pub amount: Option<cdk::Amount>,
}

/// Result of timeout checking
#[derive(Debug)]
pub struct TimeoutResult {
    pub sequence_id: EventId,
    pub timeout_type: String,
    pub affected_player: Option<PublicKey>,
}