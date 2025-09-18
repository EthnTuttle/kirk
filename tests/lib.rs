//! Comprehensive test suite for Kirk gaming protocol
//!
//! This test suite covers:
//! - Unit tests for all commitment methods and validation
//! - Integration tests for full game sequences  
//! - Property-based tests for C value randomness and commitment security
//! - Mock implementations for testing infrastructure
//! - End-to-end game simulation tests

// Test modules
pub mod mocks;
pub mod unit;
pub mod integration;
pub mod property;

// Re-export mocks for use in other test files
pub use mocks::*;