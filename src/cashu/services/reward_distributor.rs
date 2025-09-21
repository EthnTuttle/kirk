//! Reward distribution service for handling game payouts

use tracing::{debug, info};
use cdk::Amount;
use crate::error::GameResult;
use crate::game::GameSequence;
use super::{ServiceContext, RewardResult};

/// Service responsible for calculating and distributing game rewards
#[derive(Debug)]
pub struct RewardDistributor {
    context: ServiceContext,
}

impl RewardDistributor {
    /// Create a new reward distributor
    pub fn new(context: ServiceContext) -> Self {
        Self { context }
    }

    /// Extract winner from sequence state
    fn get_winner_from_state(state: &crate::game::SequenceState) -> Option<nostr::PublicKey> {
        match state {
            crate::game::SequenceState::Complete { winner } => *winner,
            crate::game::SequenceState::Forfeited { winner } => Some(*winner),
            _ => None,
        }
    }

    /// Distribute rewards for a completed sequence
    pub async fn distribute_rewards(&self, sequence: &GameSequence) -> GameResult<RewardResult> {
        let winner = Self::get_winner_from_state(&sequence.state);
        debug!(
            winner = ?winner,
            "Distributing rewards for completed sequence"
        );

        // For now, return a successful but no-op result
        // This would be expanded with actual reward distribution logic

        info!(
            winner = ?winner,
            "Reward distribution completed"
        );

        Ok(RewardResult {
            winner,
            success: true,
            amount: Some(Amount::from(0)), // Placeholder
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
        ServiceContext::new(mint, nostr_client)
    }

    #[tokio::test]
    async fn test_reward_distribution() {
        let distributor = RewardDistributor::new(create_test_context());
        let keys = Keys::generate();
        let challenge_event = nostr::EventBuilder::new(
            nostr::Kind::from(crate::events::CHALLENGE_KIND),
            r#"{"game_type": "test", "commitment_hashes": ["test"], "game_parameters": {}, "expiry": null}"#,
            []
        ).to_event(&keys).unwrap();
        let sequence = crate::game::GameSequence::new(challenge_event, keys.public_key()).unwrap();

        let result = distributor.distribute_rewards(&sequence).await.unwrap();
        assert!(result.success);
        assert!(result.amount.is_some());
    }
}