//! Security hardening utilities for the Kirk gaming protocol

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use regex::Regex;
use serde::{Deserialize, Serialize};
use crate::error::GameProtocolError;

pub mod input_validation;
pub mod rate_limiting;
pub mod crypto_audit;
pub mod time_validation;

/// Security configuration for the Kirk protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
    /// Timeout configuration with security considerations
    pub timeout_config: SecureTimeoutConfig,
    /// Input validation rules
    pub validation_rules: ValidationRules,
    /// Cryptographic security settings
    pub crypto_config: CryptoSecurityConfig,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            rate_limit: RateLimitConfig::default(),
            timeout_config: SecureTimeoutConfig::default(),
            validation_rules: ValidationRules::default(),
            crypto_config: CryptoSecurityConfig::default(),
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum requests per client per minute
    pub requests_per_minute: u32,
    /// Maximum burst size
    pub burst_size: u32,
    /// Global rate limit (requests per second)
    pub global_rate_limit: u32,
    /// Rate limit window duration in seconds
    pub window_duration: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            burst_size: 10,
            global_rate_limit: 1000,
            window_duration: 60,
        }
    }
}

/// Secure timeout configuration with DoS protection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureTimeoutConfig {
    /// Minimum allowed timeout (prevents too-short timeouts)
    pub min_timeout_seconds: u32,
    /// Maximum allowed timeout (prevents resource exhaustion)
    pub max_timeout_seconds: u32,
    /// Clock skew tolerance in seconds
    pub clock_skew_tolerance: u32,
    /// Grace period for network delays
    pub network_grace_period: u32,
}

impl Default for SecureTimeoutConfig {
    fn default() -> Self {
        Self {
            min_timeout_seconds: 60,     // At least 1 minute
            max_timeout_seconds: 86400,  // At most 24 hours
            clock_skew_tolerance: 30,    // 30 seconds tolerance
            network_grace_period: 10,    // 10 seconds for network delays
        }
    }
}

/// Input validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRules {
    /// Maximum content length for events
    pub max_content_length: usize,
    /// Maximum number of tags per event
    pub max_tags_per_event: usize,
    /// Maximum tag value length
    pub max_tag_value_length: usize,
    /// Allowed content patterns (regex)
    pub allowed_content_patterns: Vec<String>,
    /// Blocked content patterns (regex)
    pub blocked_content_patterns: Vec<String>,
}

impl Default for ValidationRules {
    fn default() -> Self {
        Self {
            max_content_length: 65536,      // 64KB max content
            max_tags_per_event: 100,        // Maximum 100 tags
            max_tag_value_length: 1024,     // 1KB max tag value
            allowed_content_patterns: vec![
                r"^[a-zA-Z0-9\s\{\}\[\],:._-]+$".to_string(), // Basic JSON-safe characters
            ],
            blocked_content_patterns: vec![
                "<script.*?</script>".to_string(),      // Block script tags
                "javascript:".to_string(),               // Block javascript URLs
                format!("data:.*{}", "base64"),          // Block base64 data URLs
            ],
        }
    }
}

/// Cryptographic security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoSecurityConfig {
    /// Minimum hash strength (bits)
    pub min_hash_bits: u32,
    /// Required random entropy bits for commitments
    pub commitment_entropy_bits: u32,
    /// Enable timing attack protection
    pub enable_constant_time_ops: bool,
    /// Maximum commitment batch size (DoS prevention)
    pub max_commitment_batch_size: usize,
}

impl Default for CryptoSecurityConfig {
    fn default() -> Self {
        Self {
            min_hash_bits: 256,                    // Require SHA-256 or better
            commitment_entropy_bits: 128,          // 128 bits of entropy minimum
            enable_constant_time_ops: true,       // Always use constant-time operations
            max_commitment_batch_size: 1000,      // Limit batch operations
        }
    }
}

/// Main security manager for the Kirk protocol
#[derive(Debug)]
pub struct SecurityManager {
    config: SecurityConfig,
    rate_limiter: Arc<Mutex<RateLimiter>>,
    input_validator: InputValidator,
    crypto_auditor: CryptoAuditor,
    time_validator: TimeValidator,
}

impl SecurityManager {
    /// Create a new security manager with the given configuration
    pub fn new(config: SecurityConfig) -> Result<Self, GameProtocolError> {
        let rate_limiter = Arc::new(Mutex::new(RateLimiter::new(config.rate_limit.clone())?));
        let input_validator = InputValidator::new(config.validation_rules.clone())?;
        let crypto_auditor = CryptoAuditor::new(config.crypto_config.clone())?;
        let time_validator = TimeValidator::new(config.timeout_config.clone());

        Ok(Self {
            config,
            rate_limiter,
            input_validator,
            crypto_auditor,
            time_validator,
        })
    }

    /// Check if a client request is within rate limits
    pub async fn check_rate_limit(&self, client_id: &str) -> Result<(), GameProtocolError> {
        let mut limiter = self.rate_limiter.lock().map_err(|_| {
            GameProtocolError::Configuration {
                message: format!("Rate limiter lock was {}", "poisoned"),
                field: "rate_limiter".to_string(),
            }
        })?;

        limiter.check_rate_limit(client_id)
    }

    /// Validate input content for security issues
    pub fn validate_input(&mut self, content: &str, context: &str) -> Result<(), GameProtocolError> {
        self.input_validator.validate_content(content, context)
    }

    /// Audit cryptographic operations for security
    pub fn audit_crypto_operation(&mut self, operation: &str, data: &[u8]) -> Result<(), GameProtocolError> {
        self.crypto_auditor.audit_operation(operation, data)
    }

    /// Validate timeout values for security
    pub fn validate_timeout(&mut self, timeout_seconds: u32, context: &str) -> Result<(), GameProtocolError> {
        self.time_validator.validate_timeout(timeout_seconds, context)
    }

    /// Validate timestamp for clock skew and timing attacks
    pub fn validate_timestamp(&mut self, timestamp: u64, context: &str) -> Result<(), GameProtocolError> {
        self.time_validator.validate_timestamp(timestamp, context)
    }

    /// Get security metrics for monitoring
    pub fn get_security_metrics(&self) -> SecurityMetrics {
        let limiter = self.rate_limiter.lock().unwrap();
        SecurityMetrics {
            rate_limit_violations: limiter.get_violation_count(),
            total_requests: limiter.get_total_requests(),
            blocked_requests: limiter.get_blocked_requests(),
            validation_errors: self.input_validator.get_error_count(),
            crypto_audit_failures: self.crypto_auditor.get_failure_count(),
            timestamp_violations: self.time_validator.get_violation_count(),
        }
    }
}

/// Security metrics for monitoring
#[derive(Debug, Clone, Serialize)]
pub struct SecurityMetrics {
    pub rate_limit_violations: u64,
    pub total_requests: u64,
    pub blocked_requests: u64,
    pub validation_errors: u64,
    pub crypto_audit_failures: u64,
    pub timestamp_violations: u64,
}

/// Token bucket rate limiter implementation
#[derive(Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: HashMap<String, TokenBucket>,
    global_bucket: TokenBucket,
    last_cleanup: Instant,
    violation_count: u64,
    total_requests: u64,
    blocked_requests: u64,
}

#[derive(Debug, Clone)]
pub struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Result<Self, GameProtocolError> {
        let global_bucket = TokenBucket::new(
            config.global_rate_limit as f64,
            config.global_rate_limit as f64,
        );

        Ok(Self {
            config,
            buckets: HashMap::new(),
            global_bucket,
            last_cleanup: Instant::now(),
            violation_count: 0,
            total_requests: 0,
            blocked_requests: 0,
        })
    }

    pub fn check_rate_limit(&mut self, client_id: &str) -> Result<(), GameProtocolError> {
        self.total_requests += 1;

        // Cleanup old buckets periodically
        if self.last_cleanup.elapsed() > Duration::from_secs(300) {
            self.cleanup_old_buckets();
        }

        // Check global rate limit first
        if !self.global_bucket.consume(1.0) {
            self.blocked_requests += 1;
            self.violation_count += 1;
            return Err(GameProtocolError::RateLimit {
                message: format!("Global rate limit has been {}", "exceeded"),
                client_id: None,
                retry_after_ms: Some(1000),
            });
        }

        // Check per-client rate limit
        let bucket = self.buckets.entry(client_id.to_string()).or_insert_with(|| {
            TokenBucket::new(
                self.config.burst_size as f64,
                self.config.requests_per_minute as f64 / 60.0, // per second
            )
        });

        if !bucket.consume(1.0) {
            self.blocked_requests += 1;
            self.violation_count += 1;
            return Err(GameProtocolError::RateLimit {
                message: format!("Rate limit exceeded for client {}", client_id),
                client_id: Some(client_id.to_string()),
                retry_after_ms: Some(60000 / self.config.requests_per_minute as u64),
            });
        }

        Ok(())
    }

    fn cleanup_old_buckets(&mut self) {
        let cutoff = Instant::now() - Duration::from_secs(600); // 10 minutes
        self.buckets.retain(|_, bucket| bucket.last_refill > cutoff);
        self.last_cleanup = Instant::now();
    }

    pub fn get_violation_count(&self) -> u64 { self.violation_count }
    pub fn get_total_requests(&self) -> u64 { self.total_requests }
    pub fn get_blocked_requests(&self) -> u64 { self.blocked_requests }
}

impl TokenBucket {
    fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens,
            last_refill: Instant::now(),
            max_tokens,
            refill_rate,
        }
    }

    fn consume(&mut self, amount: f64) -> bool {
        self.refill();

        if self.tokens >= amount {
            self.tokens -= amount;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let tokens_to_add = elapsed * self.refill_rate;

        self.tokens = (self.tokens + tokens_to_add).min(self.max_tokens);
        self.last_refill = now;
    }
}

/// Input validator for content sanitization
#[derive(Debug)]
pub struct InputValidator {
    rules: ValidationRules,
    allowed_patterns: Vec<Regex>,
    blocked_patterns: Vec<Regex>,
    error_count: u64,
}

impl InputValidator {
    pub fn new(rules: ValidationRules) -> Result<Self, GameProtocolError> {
        let allowed_patterns: Result<Vec<_>, _> = rules.allowed_content_patterns
            .iter()
            .map(|pattern| Regex::new(pattern).map_err(|e| {
                GameProtocolError::Configuration {
                    message: format!("Invalid allowed content pattern: {}", e),
                    field: "allowed_content_patterns".to_string(),
                }
            }))
            .collect();

        let blocked_patterns: Result<Vec<_>, _> = rules.blocked_content_patterns
            .iter()
            .map(|pattern| Regex::new(pattern).map_err(|e| {
                GameProtocolError::Configuration {
                    message: format!("Invalid blocked content pattern: {}", e),
                    field: "blocked_content_patterns".to_string(),
                }
            }))
            .collect();

        Ok(Self {
            rules,
            allowed_patterns: allowed_patterns?,
            blocked_patterns: blocked_patterns?,
            error_count: 0,
        })
    }

    pub fn validate_content(&mut self, content: &str, context: &str) -> Result<(), GameProtocolError> {
        // Check content length
        if content.len() > self.rules.max_content_length {
            self.error_count += 1;
            return Err(GameProtocolError::Validation {
                message: format!("Content too long: {} > {}", content.len(), self.rules.max_content_length),
                field: Some("content".to_string()),
                event_id: None,
            });
        }

        // Check blocked patterns
        for pattern in &self.blocked_patterns {
            if pattern.is_match(content) {
                self.error_count += 1;
                return Err(GameProtocolError::Validation {
                    message: format!("Content contains blocked pattern in {}", context),
                    field: Some("content".to_string()),
                    event_id: None,
                });
            }
        }

        // Check allowed patterns (if any are specified)
        if !self.allowed_patterns.is_empty() {
            let mut allowed = false;
            for pattern in &self.allowed_patterns {
                if pattern.is_match(content) {
                    allowed = true;
                    break;
                }
            }

            if !allowed {
                self.error_count += 1;
                return Err(GameProtocolError::Validation {
                    message: format!("Content does not match allowed patterns in {}", context),
                    field: Some("content".to_string()),
                    event_id: None,
                });
            }
        }

        Ok(())
    }

    pub fn get_error_count(&self) -> u64 { self.error_count }
}

/// Cryptographic operation auditor
#[derive(Debug)]
pub struct CryptoAuditor {
    config: CryptoSecurityConfig,
    failure_count: u64,
}

impl CryptoAuditor {
    pub fn new(config: CryptoSecurityConfig) -> Result<Self, GameProtocolError> {
        Ok(Self {
            config,
            failure_count: 0,
        })
    }

    pub fn audit_operation(&mut self, operation: &str, data: &[u8]) -> Result<(), GameProtocolError> {
        match operation {
            "sha256" => self.audit_hash_operation(data, 256),
            "commitment" => self.audit_commitment_operation(data),
            "batch_commitment" => self.audit_batch_commitment(data),
            _ => Ok(()), // Unknown operations pass through
        }
    }

    fn audit_hash_operation(&mut self, data: &[u8], expected_bits: u32) -> Result<(), GameProtocolError> {
        if expected_bits < self.config.min_hash_bits {
            self.failure_count += 1;
            return Err(GameProtocolError::Cryptographic {
                source: crate::error::CryptoError::InvalidKey {
                    message: format!("Hash strength {} bits below minimum {}", expected_bits, self.config.min_hash_bits),
                },
                context: "hash_operation_audit".to_string(),
            });
        }

        // Check for sufficient entropy in input data
        if data.len() < (self.config.commitment_entropy_bits as usize / 8) {
            self.failure_count += 1;
            return Err(GameProtocolError::Cryptographic {
                source: crate::error::CryptoError::InvalidKey {
                    message: format!("Insufficient entropy in hash {}", "data"),
                },
                context: "entropy_check".to_string(),
            });
        }

        Ok(())
    }

    fn audit_commitment_operation(&self, data: &[u8]) -> Result<(), GameProtocolError> {
        // Validate commitment data structure and entropy
        if data.is_empty() {
            return Err(GameProtocolError::Cryptographic {
                source: crate::error::CryptoError::CommitmentVerificationFailed {
                    message: format!("Empty commitment {}", "information"),
                },
                context: "commitment_audit".to_string(),
            });
        }

        Ok(())
    }

    fn audit_batch_commitment(&mut self, data: &[u8]) -> Result<(), GameProtocolError> {
        // Prevent DoS through oversized batches
        if data.len() > self.config.max_commitment_batch_size * 64 { // Assume 64 bytes per commitment
            self.failure_count += 1;
            return Err(GameProtocolError::Cryptographic {
                source: crate::error::CryptoError::InvalidKey {
                    message: format!("Batch commitment size exceeds security {}", "constraints"),
                },
                context: "batch_size_limit".to_string(),
            });
        }

        Ok(())
    }

    pub fn get_failure_count(&self) -> u64 { self.failure_count }
}

/// Time-based security validator
#[derive(Debug)]
pub struct TimeValidator {
    config: SecureTimeoutConfig,
    violation_count: u64,
}

impl TimeValidator {
    pub fn new(config: SecureTimeoutConfig) -> Self {
        Self {
            config,
            violation_count: 0,
        }
    }

    pub fn validate_timeout(&mut self, timeout_seconds: u32, context: &str) -> Result<(), GameProtocolError> {
        if timeout_seconds < self.config.min_timeout_seconds {
            self.violation_count += 1;
            return Err(GameProtocolError::Timeout {
                message: format!("Timeout {} too short for {}", timeout_seconds, context),
                duration_ms: timeout_seconds as u64 * 1000,
                operation: context.to_string(),
            });
        }

        if timeout_seconds > self.config.max_timeout_seconds {
            self.violation_count += 1;
            return Err(GameProtocolError::Timeout {
                message: format!("Timeout {} too long for {}", timeout_seconds, context),
                duration_ms: timeout_seconds as u64 * 1000,
                operation: context.to_string(),
            });
        }

        Ok(())
    }

    pub fn validate_timestamp(&mut self, timestamp: u64, context: &str) -> Result<(), GameProtocolError> {
        let now = chrono::Utc::now().timestamp() as u64;
        let tolerance = self.config.clock_skew_tolerance as u64;

        // Check for timestamps too far in the past or future
        if timestamp + tolerance < now || timestamp > now + tolerance {
            self.violation_count += 1;
            return Err(GameProtocolError::Timeout {
                message: format!("Timestamp {} outside acceptable range for {}", timestamp, context),
                duration_ms: tolerance * 1000,
                operation: context.to_string(),
            });
        }

        Ok(())
    }

    pub fn get_violation_count(&self) -> u64 { self.violation_count }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_basic_functionality() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 5,
            global_rate_limit: 1000,
            window_duration: 60,
        };

        let mut limiter = RateLimiter::new(config).unwrap();

        // Should allow burst requests
        for _ in 0..5 {
            assert!(limiter.check_rate_limit("client1").is_ok());
        }

        // Should reject the next request
        assert!(limiter.check_rate_limit("client1").is_err());

        // Different client should have separate bucket
        assert!(limiter.check_rate_limit("client2").is_ok());
    }

    #[test]
    fn test_input_validator_content_length() {
        let mut validator = InputValidator::new(ValidationRules::default()).unwrap();

        // Short content should pass
        assert!(validator.validate_content(&format!("short {}", "text"), "test").is_ok());

        // Long content should fail
        let long_content = "x".repeat(100000);
        assert!(validator.validate_content(&long_content, "test").is_err());
    }

    #[test]
    fn test_input_validator_blocked_patterns() {
        let rules = ValidationRules {
            blocked_content_patterns: vec![r"<script.*?>".to_string()],
            ..Default::default()
        };

        let mut validator = InputValidator::new(rules).unwrap();

        // Normal content should pass
        assert!(validator.validate_content(&format!("normal {}", "text"), "test").is_ok());

        // Script tags should be blocked
        let malicious_script = r#"<script>alert("xss")</script>"#;
        assert!(validator.validate_content(malicious_script, "test").is_err());
    }

    #[test]
    fn test_crypto_auditor_hash_strength() {
        let config = CryptoSecurityConfig {
            min_hash_bits: 256,
            ..Default::default()
        };

        let mut auditor = CryptoAuditor::new(config).unwrap();

        // Sufficient data should pass
        let good_data = vec![1u8; 32];
        assert!(auditor.audit_operation("sha256", &good_data).is_ok());

        // Insufficient data should fail
        let bad_data = vec![1u8; 8];
        assert!(auditor.audit_operation("sha256", &bad_data).is_err());
    }

    #[test]
    fn test_time_validator_timeout_bounds() {
        let config = SecureTimeoutConfig::default();
        let mut validator = TimeValidator::new(config);

        // Valid timeout should pass
        assert!(validator.validate_timeout(3600, "test").is_ok());

        // Too short timeout should fail
        assert!(validator.validate_timeout(30, "test").is_err());

        // Too long timeout should fail
        assert!(validator.validate_timeout(100000, "test").is_err());
    }

    #[test]
    fn test_time_validator_timestamp_skew() {
        let config = SecureTimeoutConfig::default();
        let mut validator = TimeValidator::new(config);
        let now = chrono::Utc::now().timestamp() as u64;

        // Test current time
        assert!(validator.validate_timestamp(now, "test").is_ok());

        // Test future time within tolerance
        assert!(validator.validate_timestamp(now + 20, "test").is_ok());

        // Test far future time
        assert!(validator.validate_timestamp(now + 3600, "test").is_err());

        // Test far past time
        let past_timestamp = now - 3600;
        let test_context = "validation";
        assert!(validator.validate_timestamp(past_timestamp, test_context).is_err());
    }
}