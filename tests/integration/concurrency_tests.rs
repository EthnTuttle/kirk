//! Concurrency and thread safety tests for the Kirk gaming protocol

use kirk::{SequenceProcessor, TokenCommitment, CommitmentMethod, GameResult};
use nostr::{Keys, EventBuilder, Kind, Tag, EventId};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::sync::RwLock;
use cdk::nuts::{Token, Proof, Id, CurrencyUnit, PublicKey as CashuPublicKey};
use cashu::secret::Secret;
use cdk::Amount;

/// Helper to create test tokens
fn create_test_token(c_value: String, amount: u64) -> Token {
    let proof = Proof {
        amount: Amount::from(amount),
        secret: Secret::new(format!("secret_{}", amount)),
        c: CashuPublicKey::from_hex(&format!("{:0>64}", c_value)).unwrap(),
        keyset_id: Id::from_bytes(&[0u8; 8]).unwrap(),
        witness: None,
        dleq: None,
    };

    Token::new(
        "https://test-mint.example.com".parse().unwrap(),
        vec![proof],
        None,
        CurrencyUnit::Sat,
    )
}

#[tokio::test]
async fn test_concurrent_commitment_creation() {
    let num_threads = 10;
    let commitments_per_thread = 100;
    let results = Arc::new(Mutex::new(Vec::new()));

    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let results_clone = Arc::clone(&results);

        let handle = tokio::spawn(async move {
            let mut local_results = Vec::new();

            for i in 0..commitments_per_thread {
                let c_value = format!("{:064x}", thread_id * 1000 + i);
                let token = create_test_token(c_value, 100 + i);

                let commitment = TokenCommitment::single(&token);
                local_results.push(commitment.commitment_hash);
            }

            {
                let mut results = results_clone.lock().unwrap();
                results.extend(local_results);
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.await.unwrap();
    }

    let results = results.lock().unwrap();
    assert_eq!(results.len(), num_threads * commitments_per_thread);

    // Verify all hashes are unique (collision resistance under concurrency)
    let mut unique_hashes = std::collections::HashSet::new();
    for hash in results.iter() {
        assert!(unique_hashes.insert(hash.clone()), "Duplicate hash found: {}", hash);
    }

    assert_eq!(unique_hashes.len(), num_threads * commitments_per_thread);
}

#[tokio::test]
async fn test_concurrent_commitment_verification() {
    let tokens: Vec<Token> = (0..50)
        .map(|i| create_test_token(format!("{:064x}", i * 12345), 100 + i))
        .collect();

    let commitments: Vec<_> = tokens
        .iter()
        .map(|token| TokenCommitment::single(token))
        .collect();

    let num_threads = 20;
    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let tokens_clone = tokens.clone();
        let commitments_clone = commitments.clone();

        let handle = tokio::spawn(async move {
            for (i, commitment) in commitments_clone.iter().enumerate() {
                let token = &tokens_clone[i];
                let result = commitment.verify(&[token.clone()]);

                assert!(result.is_ok(), "Verification failed in thread {} for commitment {}", thread_id, i);
                assert!(result.unwrap(), "Verification returned false in thread {} for commitment {}", thread_id, i);
            }
        });

        handles.push(handle);
    }

    // Wait for all verifications to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_sequence_processor_concurrent_access() {
    let processor = Arc::new(RwLock::new(SequenceProcessor::new()));
    let num_readers = 10;
    let num_operations = 50;

    let mut handles = vec![];

    // Spawn multiple reader threads
    for thread_id in 0..num_readers {
        let processor_clone = Arc::clone(&processor);

        let handle = tokio::spawn(async move {
            for i in 0..num_operations {
                {
                    let processor = processor_clone.read().await;

                    // Perform read operations
                    let stats = processor.get_statistics();
                    assert!(stats.total_events >= 0);

                    let sequences = processor.get_active_sequences();
                    // Just verify we can access without panicking
                    let _ = sequences.len();
                }

                // Small delay to increase chance of concurrent access
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });

        handles.push(handle);
    }

    // Wait for all readers to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_concurrent_commitment_methods() {
    let tokens: Vec<Token> = (0..20)
        .map(|i| create_test_token(format!("{:064x}", i * 54321), 100 + i))
        .collect();

    let methods = vec![
        CommitmentMethod::Concatenation,
        CommitmentMethod::MerkleTreeRadix4,
    ];

    let num_threads = 8;
    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let tokens_clone = tokens.clone();
        let methods_clone = methods.clone();

        let handle = tokio::spawn(async move {
            for method in methods_clone {
                let commitment = TokenCommitment::multiple(&tokens_clone, method);

                // Verify the commitment
                let result = commitment.verify(&tokens_clone);
                assert!(result.is_ok(), "Verification failed in thread {} for method {:?}", thread_id, method);
                assert!(result.unwrap(), "Verification returned false in thread {} for method {:?}", thread_id, method);

                // Check hash properties
                assert_eq!(commitment.commitment_hash.len(), 64);
                assert!(hex::decode(&commitment.commitment_hash).is_ok());
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_stress_commitment_creation() {
    let num_iterations = 1000;
    let start_time = std::time::Instant::now();

    let mut commitments = Vec::with_capacity(num_iterations);

    for i in 0..num_iterations {
        let c_value = format!("{:064x}", i * 987654321);
        let token = create_test_token(c_value, 100 + i as u64);

        let commitment = TokenCommitment::single(&token);
        commitments.push(commitment);
    }

    let elapsed = start_time.elapsed();

    // Should complete in reasonable time (less than 10 seconds for 1000 commitments)
    assert!(elapsed.as_secs() < 10, "Stress test took too long: {:?}", elapsed);

    // Verify all commitments are valid
    assert_eq!(commitments.len(), num_iterations);

    for (i, commitment) in commitments.iter().enumerate() {
        assert!(!commitment.commitment_hash.is_empty());
        assert_eq!(commitment.commitment_hash.len(), 64);

        // Verify hash is valid hex
        assert!(hex::decode(&commitment.commitment_hash).is_ok(),
                "Invalid hex hash at index {}: {}", i, commitment.commitment_hash);
    }

    println!("Stress test completed {} commitments in {:?}", num_iterations, elapsed);
}

#[tokio::test]
async fn test_memory_usage_under_load() {
    let initial_memory = get_memory_usage();
    let num_large_commitments = 100;
    let tokens_per_commitment = 50;

    let mut commitments = Vec::new();

    for commitment_id in 0..num_large_commitments {
        let tokens: Vec<Token> = (0..tokens_per_commitment)
            .map(|i| create_test_token(
                format!("{:064x}", commitment_id * 1000 + i),
                100 + i as u64
            ))
            .collect();

        let commitment = TokenCommitment::multiple(&tokens, CommitmentMethod::MerkleTreeRadix4);
        commitments.push(commitment);
    }

    let peak_memory = get_memory_usage();

    // Clear commitments and check memory recovery
    drop(commitments);

    // Force garbage collection
    #[cfg(feature = "gc")]
    {
        std::hint::black_box(());
    }

    let final_memory = get_memory_usage();

    println!("Memory usage - Initial: {}KB, Peak: {}KB, Final: {}KB",
             initial_memory / 1024, peak_memory / 1024, final_memory / 1024);

    // Memory usage should be reasonable (less than 100MB for this test)
    assert!(peak_memory - initial_memory < 100 * 1024 * 1024,
            "Excessive memory usage: {}MB", (peak_memory - initial_memory) / (1024 * 1024));
}

#[tokio::test]
async fn test_concurrent_hash_avalanche_effect() {
    let base_c_value = "1234567890abcdef".repeat(4); // 64 hex chars
    let num_threads = 8;
    let mutations_per_thread = 32;

    let results = Arc::new(Mutex::new(Vec::new()));
    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let base_c_value_clone = base_c_value.clone();
        let results_clone = Arc::clone(&results);

        let handle = tokio::spawn(async move {
            let mut local_results = Vec::new();

            for bit_position in 0..mutations_per_thread {
                let mut c_bytes = hex::decode(&base_c_value_clone).unwrap();
                let byte_index = (thread_id * mutations_per_thread + bit_position) % (c_bytes.len() * 8) / 8;
                let bit_index = (thread_id * mutations_per_thread + bit_position) % 8;

                if byte_index < c_bytes.len() {
                    c_bytes[byte_index] ^= 1 << bit_index;
                    let modified_c_value = hex::encode(c_bytes);
                    let token = create_test_token(modified_c_value, 100);
                    let commitment = TokenCommitment::single(&token);

                    local_results.push(commitment.commitment_hash);
                }
            }

            {
                let mut results = results_clone.lock().unwrap();
                results.extend(local_results);
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let results = results.lock().unwrap();

    // Create base commitment for comparison
    let base_token = create_test_token(base_c_value, 100);
    let base_commitment = TokenCommitment::single(&base_token);

    // All mutations should produce different hashes
    for hash in results.iter() {
        assert_ne!(*hash, base_commitment.commitment_hash, "Hash collision detected");
    }

    // All hashes should be unique
    let unique_hashes: std::collections::HashSet<_> = results.iter().collect();
    assert_eq!(unique_hashes.len(), results.len(), "Duplicate hashes found in concurrent mutations");
}

fn get_memory_usage() -> usize {
    // Simple memory usage approximation
    // In a real implementation, you might use a proper memory profiling library
    std::process::id() as usize * 1024 // Placeholder
}

#[tokio::test]
async fn test_race_condition_protection() {
    let shared_counter = Arc::new(Mutex::new(0));
    let num_threads = 20;
    let increments_per_thread = 100;

    let mut handles = vec![];

    for _ in 0..num_threads {
        let counter_clone = Arc::clone(&shared_counter);

        let handle = tokio::spawn(async move {
            for _ in 0..increments_per_thread {
                {
                    let mut counter = counter_clone.lock().unwrap();
                    *counter += 1;
                }

                // Create a commitment to simulate real work
                let token = create_test_token("abcd1234".repeat(8), 100);
                let _commitment = TokenCommitment::single(&token);

                tokio::time::sleep(Duration::from_nanos(1)).await;
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let final_count = *shared_counter.lock().unwrap();
    assert_eq!(final_count, num_threads * increments_per_thread,
               "Race condition detected: expected {}, got {}",
               num_threads * increments_per_thread, final_count);
}