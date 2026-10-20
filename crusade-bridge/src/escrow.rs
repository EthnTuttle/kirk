//! Wager escrow (V12-2; docs/WAGERS.md).
//!
//! Construction: **NUT-11 P2PK 2-of-2 multisig over the two duel pubkeys
//! with a timelocked depositor-only refund**. Each side locks
//! `sats_per_side` into proofs whose spending condition is
//! `data = counterparty key, pubkeys = [depositor key], num_sigs = 2,
//! locktime = refund_after, refund = [depositor key]`. Before
//! `refund_after` the mint enforces that BOTH duelists' Schnorr witness
//! signatures spend the proof — the same two nostr keys that seal the
//! kind-9265 attestations. After `refund_after` the refund key
//! (depositor alone) supersedes, at mint level, so an unsettled pot is
//! refundable unilaterally.
//!
//! Chosen over the HTLC/hashlock alternative: a hashlock-to-result has a
//! single preimage, and a duel has two possible winners, so the preimage
//! would live with a custodial adjudicator key — strictly more trust and
//! more machinery than the 2-of-2 CDK already verifies natively. An
//! honest settlement is BY CONSTRUCTION a 2-of-2 signature pair (both
//! duelists sign their seals); a withheld or poisoned seal folds
//! identically to "no settlement" and the refund window pays both sides
//! back. No house rake in v1: the pot is exactly `2 * sats_per_side`,
//! the winner takes it whole, disputes refund in full.
//!
//! Adjudication NEVER trusts a bare claim: it folds the seal stream
//! through [`crate::ladder::canonical_settlements`] — the exact shared
//! gates the ladder rates on (well-formed, no-equivocation, mutual
//! peer-naming, agreeing digest/winner/host, skew window) — and releases
//! only when a settlement whose pair IS the two escrowed duelists folds
//! for THIS battle id. Fold-equality: the winner the ladder would credit
//! is the winner the escrow pays, because both read the same function's
//! output.
//!
//! Mint seam: [`EscrowConfig::from_env`] reads `CRUSADE_MINT_URL`;
//! default is the in-memory test mint (`test-mint://in-memory`,
//! resolved by the CALLER — the cdk `Mint` handle is passed in here,
//! this module needs no network). Pointing at the owner's production
//! self-hosted CDK mint is a config line, never a code change.
//!
//! Clock discipline: unix time enters ONLY through caller-passed `now`
//! parameters (wasm-safe; the mint enforces locktime itself, this module
//! adds a fail-closed pre-check so a rejected spend surfaces as a typed
//! [`EscrowError`] instead of a mint rejection).

use std::collections::BTreeSet;

use cdk::dhke::{blind_message, construct_proofs};
use cdk::nuts::{
    nut10::Secret as Nut10Secret, BlindedMessage, CurrencyUnit, Id, Keys as MintKeys, Proof, Proofs,
    PublicKey as CashuPublicKey, SecretKey as CashuSecretKey, SigFlag, SpendingConditions,
    SwapRequest,
};
use cdk::Mint;
use cashu::secret::Secret;
use nostr::{Event, PublicKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ladder::canonical_settlements;

/// Environment variable carrying the mint URL. Deployment-time owner
/// config (G2 resolution, ROADMAP): the repo default is the in-memory
/// test mint; the owner's self-hosted production mint is swapped in by
/// setting this variable, never by editing code.
pub const CRUSADE_MINT_URL_ENV: &str = "CRUSADE_MINT_URL";

/// Sentinel URL for the in-memory test mint. Callers map it to a freshly
/// constructed in-process CDK mint (the `GameMint::new_test` precedent).
pub const IN_MEMORY_TEST_MINT_URL: &str = "test-mint://in-memory";

/// Default escrow lifetime in seconds (48h): sealed settlement must land
/// before this window closes; afterwards both sides refund. Override via
/// `CRUSADE_WAGER_GRACE_SECS`.
pub const DEFAULT_GRACE_SECS: u64 = 48 * 3600;

/// Environment override for the escrow lifetime.
pub const CRUSADE_WAGER_GRACE_SECS_ENV: &str = "CRUSADE_WAGER_GRACE_SECS";

/// Deployment config for the wager escrow. Pure data; no network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscrowConfig {
    /// Cashu mint base URL, or [`IN_MEMORY_TEST_MINT_URL`].
    pub mint_url: String,
    /// Escrow lifetime in seconds: settle-before-this-window / refund-after.
    pub grace_secs: u64,
}

impl EscrowConfig {
    /// Read config from the environment. The mint URL defaults to the
    /// in-memory test mint (G2: production mint URL is deployment-time
    /// owner config, NOT repo content).
    pub fn from_env() -> Self {
        let mint_url = std::env::var(CRUSADE_MINT_URL_ENV)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| IN_MEMORY_TEST_MINT_URL.to_string());
        let grace_secs = std::env::var(CRUSADE_WAGER_GRACE_SECS_ENV)
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|g| *g > 0)
            .unwrap_or(DEFAULT_GRACE_SECS);
        EscrowConfig {
            mint_url,
            grace_secs,
        }
    }

    /// Explicit test-mint config (the default content of `from_env`).
    pub fn test_mint() -> Self {
        EscrowConfig {
            mint_url: IN_MEMORY_TEST_MINT_URL.to_string(),
            grace_secs: DEFAULT_GRACE_SECS,
        }
    }
}

/// The terms both duelists agree to escrow under. Pubkeys are nostr
/// 64-hex (same "serde discipline: hex strings in the wire struct" as
/// [`crate::attest::Attestation`]); `battle` is the kind-9265 `d`-tagged
/// battle id, 64 lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WagerTerms {
    /// 64 lowercase hex battle id — the escrow is bound to ONE battle.
    pub battle: String,
    /// Sats locked per side; the pot is exactly twice this.
    pub sats_per_side: u64,
    /// Duelist A, nostr pubkey hex.
    pub duelist_a: String,
    /// Duelist B, nostr pubkey hex.
    pub duelist_b: String,
    /// Unix timestamp: settlement window closes / refund window opens.
    pub refund_after: u64,
}

/// What the adjudicated seal stream says about this escrow's battle.
///
/// `Expired` shadows everything else: once `now >= refund_after` the
/// mint itself gives refund keys precedence, so a late settlement can no
/// longer pay out — the escrow's only live path is the refund.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscrowVerdict {
    /// No mutual-peer attested settlement folds for this battle (bare
    /// claims, one-sided seals, forgeries, equivocations — all the same).
    Pending,
    /// A settlement formed; the winner pubkey pays out.
    Settle(PublicKey),
    /// Past `refund_after`: only the per-side refund path remains.
    Expired,
}

/// Escrow errors: every rejection is typed, nothing fails open.
#[derive(Debug, Error)]
pub enum EscrowError {
    #[error("terms invalid: {0}")]
    Terms(String),
    #[error("pubkey: {0}")]
    Key(String),
    #[error("secret/conditions: {0}")]
    Conditions(String),
    #[error("mint rejected the swap: {0}")]
    Mint(String),
    #[error("funding total {got} sats != sats_per_side {expected}")]
    AmountMismatch { expected: u64, got: u64 },
    #[error("a deposit proof does not match this escrow's lock conditions")]
    ForeignLock,
    #[error("no mutual-peer attested settlement folds for this battle")]
    NoSettlement,
    #[error("settlement window closed (refund_after = {0}) — only refunds remain")]
    SettlementClosed(u64),
    #[error("refund window not open yet (opens at {0})")]
    RefundNotOpen(u64),
}

/// nostr x-only pubkey -> cashu P2PK pubkey. Schnorr witness checks are
/// x-only internal, so the even-parity compression prefix is canonical.
fn cashu_pubkey(pk_hex: &str) -> Result<CashuPublicKey, EscrowError> {
    let npk = PublicKey::from_hex(pk_hex).map_err(|e| EscrowError::Key(e.to_string()))?;
    CashuPublicKey::from_hex(format!("02{}", npk.to_hex()))
        .map_err(|e| EscrowError::Key(e.to_string()))
}

fn nostr_pubkey(pk_hex: &str) -> Result<PublicKey, EscrowError> {
    PublicKey::from_hex(pk_hex).map_err(|e| EscrowError::Key(e.to_string()))
}

fn is_64_lower_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn proofs_total(proofs: &Proofs) -> u64 {
    proofs.iter().map(|p| u64::from(p.amount)).sum()
}

impl WagerTerms {
    /// Compose terms from config: `refund_after = now + config.grace_secs`.
    pub fn new(
        config: &EscrowConfig,
        battle: &str,
        sats_per_side: u64,
        duelist_a: &PublicKey,
        duelist_b: &PublicKey,
        now: u64,
    ) -> Result<Self, EscrowError> {
        let terms = WagerTerms {
            battle: battle.to_string(),
            sats_per_side,
            duelist_a: duelist_a.to_hex(),
            duelist_b: duelist_b.to_hex(),
            refund_after: now.saturating_add(config.grace_secs),
        };
        terms.validate()?;
        Ok(terms)
    }

    fn validate(&self) -> Result<(), EscrowError> {
        if !is_64_lower_hex(&self.battle) {
            return Err(EscrowError::Terms(
                "battle id must be 64 lowercase hex".to_string(),
            ));
        }
        if self.sats_per_side == 0 {
            return Err(EscrowError::Terms("zero wager".to_string()));
        }
        let a = nostr_pubkey(&self.duelist_a)?;
        let b = nostr_pubkey(&self.duelist_b)?;
        if a == b {
            return Err(EscrowError::Terms("duelists must be distinct".to_string()));
        }
        Ok(())
    }

    fn duelist_pubkeys(&self) -> Result<[PublicKey; 2], EscrowError> {
        Ok([
            nostr_pubkey(&self.duelist_a)?,
            nostr_pubkey(&self.duelist_b)?,
        ])
    }

    /// The NUT-11 lock condition for `depositor`'s deposit: 2-of-2 over
    /// both duelists before `refund_after`, depositor-only refund after.
    /// `depositor` must be one of the two escrowed duelists.
    pub fn deposit_conditions(
        &self,
        depositor: &PublicKey,
    ) -> Result<SpendingConditions, EscrowError> {
        let [a, b] = self.duelist_pubkeys()?;
        let counterparty = if *depositor == a {
            b
        } else if *depositor == b {
            a
        } else {
            return Err(EscrowError::Terms(
                "depositor is not an escrowed duelist".to_string(),
            ));
        };
        let conditions = cdk::nuts::Conditions {
            locktime: Some(self.refund_after),
            // Authorized set = data + pubkeys = {counterparty, depositor},
            // num_sigs = 2: BOTH duelists must sign any spend.
            pubkeys: Some(vec![cashu_pubkey(&depositor.to_hex())?]),
            refund_keys: Some(vec![cashu_pubkey(&depositor.to_hex())?]),
            num_sigs: Some(2),
            sig_flag: SigFlag::SigInputs,
            num_sigs_refund: None, // mint default (1): depositor refunds alone
        };
        Ok(SpendingConditions::P2PKConditions {
            data: cashu_pubkey(&counterparty.to_hex())?,
            conditions: Some(conditions),
        })
    }

    /// Does `proof`'s NUT-10 secret match this escrow's lock conditions
    /// for `depositor` exactly (nonce ignored)?
    fn proof_matches_lock(&self, proof: &Proof, depositor: &PublicKey) -> bool {
        let parsed: SpendingConditions = match SpendingConditions::try_from(&proof.secret) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let expected = match self.deposit_conditions(depositor) {
            Ok(c) => c,
            Err(_) => return false,
        };
        parsed == expected
    }
}

/// The wager escrow coordinator. Holds terms + config; all mint
/// interaction goes through the caller-supplied `cdk::Mint` handle (the
/// caller maps [`EscrowConfig::mint_url`] to a mint: in-process test
/// mint for the sentinel, HTTP client for a production URL).
pub struct WagerEscrow {
    terms: WagerTerms,
    config: EscrowConfig,
}

impl WagerEscrow {
    pub fn new(terms: WagerTerms, config: EscrowConfig) -> Result<Self, EscrowError> {
        terms.validate()?;
        Ok(WagerEscrow { terms, config })
    }

    pub fn terms(&self) -> &WagerTerms {
        &self.terms
    }

    pub fn config(&self) -> &EscrowConfig {
        &self.config
    }

    /// Adjudicate the seal stream for THIS battle. Reads the exact same
    /// canonical fold the ladder rates on
    /// ([`crate::ladder::canonical_settlements`], empty settlement cursor
    /// — escrow replay protection is the mint's double-spend rejection,
    /// checked below at spend time), then intersects with the escrowed
    /// pair. Fail-closed: anything that does not fold into a settlement
    /// of exactly these two duelists on exactly this battle id is
    /// `Pending`, and after `refund_after` everything is `Expired`.
    pub fn adjudicate<'a>(
        &self,
        seals: impl IntoIterator<Item = &'a Event>,
        now: u64,
    ) -> EscrowVerdict {
        if now >= self.terms.refund_after {
            return EscrowVerdict::Expired;
        }
        let Ok(duelists) = self.terms.duelist_pubkeys() else {
            return EscrowVerdict::Pending;
        };
        let mut pair = duelists;
        pair.sort();
        for s in canonical_settlements(seals, &BTreeSet::new()) {
            if s.battle() == self.terms.battle && s.pair() == pair {
                return EscrowVerdict::Settle(s.winner());
            }
        }
        EscrowVerdict::Pending
    }

    /// Lock one side's funding into this escrow. `funding` are plain
    /// (unlocked) proofs totaling exactly `sats_per_side`; the returned
    /// proofs carry the 2-of-2 lock conditions and are what the
    /// attendant custodies until settle/refund.
    pub async fn lock(
        &self,
        mint: &Mint,
        depositor: &PublicKey,
        funding: Proofs,
        now: u64,
    ) -> Result<Proofs, EscrowError> {
        if now >= self.terms.refund_after {
            return Err(EscrowError::SettlementClosed(self.terms.refund_after));
        }
        let got = proofs_total(&funding);
        if got != self.terms.sats_per_side {
            return Err(EscrowError::AmountMismatch {
                expected: self.terms.sats_per_side,
                got,
            });
        }
        let conditions = self.terms.deposit_conditions(depositor)?;
        self.swap(
            mint,
            funding,
            Some(&conditions),
            self.terms.sats_per_side,
        )
        .await
    }

    /// Settle: fold the seals, and ONLY on an attested settlement whose
    /// winner is one of the escrowed duelists, spend both locked sides
    /// (each proof witness-signed by BOTH duelists) to a fresh payout of
    /// the full pot. No rake. Returns the winner's payout proofs.
    pub async fn settle(
        &self,
        mint: &Mint,
        seals: &[Event],
        locked: Proofs,
        duelist_a: &nostr::Keys,
        duelist_b: &nostr::Keys,
        now: u64,
    ) -> Result<Proofs, EscrowError> {
        let winner = match self.adjudicate(seals.iter(), now) {
            EscrowVerdict::Settle(w) => w,
            EscrowVerdict::Expired => {
                return Err(EscrowError::SettlementClosed(self.terms.refund_after));
            }
            EscrowVerdict::Pending => return Err(EscrowError::NoSettlement),
        };
        let [a, b] = self.terms.duelist_pubkeys()?;
        if duelist_a.public_key() != a
            || duelist_b.public_key() != b
            || (winner != a && winner != b)
        {
            return Err(EscrowError::Terms(
                "settlement keys must be the escrowed duelists".to_string(),
            ));
        }
        // Bound the custody: every proof must be one of the two sides'
        // locks, totaling exactly the pot, before any money moves.
        self.check_locked(&locked, 2 * self.terms.sats_per_side, &[a, b])?;
        let sk_a = cashu_secret_key(duelist_a)?;
        let sk_b = cashu_secret_key(duelist_b)?;
        let mut signed = locked;
        for proof in &mut signed {
            proof
                .sign_p2pk(sk_a.clone())
                .map_err(|e| EscrowError::Conditions(e.to_string()))?;
            proof
                .sign_p2pk(sk_b.clone())
                .map_err(|e| EscrowError::Conditions(e.to_string()))?;
        }
        self.swap(mint, signed, None, 2 * self.terms.sats_per_side)
            .await
    }

    /// Refund one side's locked deposit after `refund_after`. The
    /// 2-of-2 lock is superseded by the depositor-only refund condition
    /// (mint-enforced); on conflicting seals / withheld counterseals /
    /// plain timeout this is the only path, and it pays each side back
    /// in full — v1 burns nothing.
    pub async fn refund(
        &self,
        mint: &Mint,
        locked_side: Proofs,
        depositor: &nostr::Keys,
        now: u64,
    ) -> Result<Proofs, EscrowError> {
        if now < self.terms.refund_after {
            return Err(EscrowError::RefundNotOpen(self.terms.refund_after));
        }
        let [a, b] = self.terms.duelist_pubkeys()?;
        let dep_pub = depositor.public_key();
        if dep_pub != a && dep_pub != b {
            return Err(EscrowError::Terms(
                "refund signer is not an escrowed duelist".to_string(),
            ));
        }
        self.check_locked(&locked_side, self.terms.sats_per_side, &[dep_pub])?;
        let sk = cashu_secret_key(depositor)?;
        let mut signed = locked_side;
        for proof in &mut signed {
            proof
                .sign_p2pk(sk.clone())
                .map_err(|e| EscrowError::Conditions(e.to_string()))?;
        }
        self.swap(mint, signed, None, self.terms.sats_per_side)
            .await
    }

    /// Every locked proof must match one of `depositors`' lock
    /// conditions exactly, and the totals must equal `expected`.
    fn check_locked(
        &self,
        locked: &Proofs,
        expected: u64,
        depositors: &[PublicKey],
    ) -> Result<(), EscrowError> {
        let got = proofs_total(locked);
        if got != expected {
            return Err(EscrowError::AmountMismatch { expected, got });
        }
        for proof in locked {
            if !depositors
                .iter()
                .any(|d| self.terms.proof_matches_lock(proof, d))
            {
                return Err(EscrowError::ForeignLock);
            }
        }
        Ok(())
    }

    /// One balanced swap against the mint: `inputs` in exchange for
    /// fresh proofs of `total` sats. `conditions: Some` puts the NUT-11
    /// lock on every output; `None` yields fresh random (bearer)
    /// secrets. Mirroring the cdk wallet (`PreMintSecrets::
    /// with_conditions`): EACH output re-derives `Nut10Secret` from the
    /// conditions so every secret gets a fresh random nonce — one secret
    /// string per output, never shared (shared Y values would collide).
    async fn swap(
        &self,
        mint: &Mint,
        inputs: Proofs,
        conditions: Option<&SpendingConditions>,
        total: u64,
    ) -> Result<Proofs, EscrowError> {
        let (keyset_id, keys) = active_keyset(mint)?;
        let amounts = split_amounts(total);
        let mut outputs = Vec::with_capacity(amounts.len());
        let mut secrets = Vec::with_capacity(amounts.len());
        let mut rs = Vec::with_capacity(amounts.len());
        for amount in &amounts {
            let secret: Secret = match conditions {
                Some(c) => {
                    let nut10: Nut10Secret = c.clone().into();
                    Secret::try_from(nut10)
                        .map_err(|e: cashu::nuts::nut10::Error| {
                            EscrowError::Conditions(e.to_string())
                        })?
                }
                None => Secret::generate(),
            };
            let (blinded, r) = blind_message(&secret.to_bytes(), None)
                .map_err(|e| EscrowError::Conditions(e.to_string()))?;
            outputs.push(BlindedMessage::new(*amount, keyset_id, blinded));
            secrets.push(secret);
            rs.push(r);
        }
        let request = SwapRequest::new(inputs, outputs);
        let response = mint
            .process_swap_request(request)
            .await
            .map_err(|e| EscrowError::Mint(e.to_string()))?;
        construct_proofs(response.signatures, rs, secrets, &keys)
            .map_err(|e| EscrowError::Mint(e.to_string()))
    }
}

fn cashu_secret_key(keys: &nostr::Keys) -> Result<CashuSecretKey, EscrowError> {
    CashuSecretKey::from_slice(keys.secret_key().secret_bytes().as_slice())
        .map_err(|e| EscrowError::Key(e.to_string()))
}

/// The mint's active Sat keyset: id + amount pubkeys (needed to unblind
/// the mint's signatures into proofs).
fn active_keyset(mint: &Mint) -> Result<(Id, MintKeys), EscrowError> {
    let id = *mint
        .get_active_keysets()
        .get(&CurrencyUnit::Sat)
        .ok_or_else(|| EscrowError::Mint("mint has no active sat keyset".to_string()))?;
    let response = mint
        .keyset_pubkeys(&id)
        .map_err(|e| EscrowError::Mint(e.to_string()))?;
    let keyset = response
        .keysets
        .into_iter()
        .next()
        .ok_or_else(|| EscrowError::Mint("empty keyset response".to_string()))?;
    Ok((id, keyset.keys))
}

/// Split `sats` into power-of-two keyset denominations.
fn split_amounts(mut sats: u64) -> Vec<cdk::Amount> {
    let mut out = Vec::new();
    let mut bit = 1u64;
    while sats > 0 {
        if sats & bit != 0 {
            out.push(cdk::Amount::from(bit));
            sats &= !bit;
        }
        bit <<= 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use cdk::nuts::{nut10::Secret as Nut10Secret, CurrencyUnit};
    use nostr::{Event, Keys};

    use super::*;
    use crate::attest::{sign_attestation, Attestation};
    use crate::ladder::fold_attestations;

    // ---------- test kit (the GameMint::new_test precedent) ----------

    const SATS: u64 = 21; // deliberately NOT a power of two

    /// Real wall-clock time: the in-memory mint enforces NUT-11 locktimes
    /// against real time, so escrow tests must use it too (short
    /// grace + a sleep flips the refund path mint-side).
    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// The exact in-memory mint construction kirk's GameMint::new_test
    /// uses: in-memory sqlite, embedded DbSignatory, one active Sat
    /// keyset (fee 0), no payment backend.
    async fn test_mint() -> Mint {
        let localstore = Arc::new(
            cdk_sqlite::mint::memory::empty()
                .await
                .expect("in-memory mint database must initialize"),
        );
        let mut supported_units = HashMap::new();
        supported_units.insert(CurrencyUnit::Sat, (0, 32));
        let signatory = Arc::new(
            cdk_signatory::db_signatory::DbSignatory::new(
                localstore.clone(),
                &[0x42u8; 32],
                supported_units,
                HashMap::new(),
            )
            .await
            .expect("test signatory must initialize"),
        );
        Mint::new(
            cdk::nuts::MintInfo::default(),
            signatory,
            localstore,
            HashMap::new(),
        )
        .await
        .expect("in-memory test mint must initialize")
    }

    /// Issue `sats` worth of plain bearer proofs (self-minted faucet).
    async fn faucet(mint: &Mint, sats: u64) -> Proofs {
        let (keyset_id, keys) = active_keyset(mint).expect("active keyset");
        let amounts = split_amounts(sats);
        let mut outputs = Vec::new();
        let mut secrets = Vec::new();
        let mut rs = Vec::new();
        for amount in &amounts {
            let secret = Secret::generate();
            let (blinded, r) = blind_message(&secret.to_bytes(), None).unwrap();
            outputs.push(BlindedMessage::new(*amount, keyset_id, blinded));
            secrets.push(secret);
            rs.push(r);
        }
        let promises = mint.blind_sign(outputs).await.expect("faucet blindsign");
        construct_proofs(promises, rs, secrets, &keys).expect("faucet proofs")
    }

    fn duel() -> (Keys, Keys, String) {
        let host = Keys::generate();
        let joiner = Keys::generate();
        let battle = hex::encode([0x77u8; 32]);
        (host, joiner, battle)
    }

    fn escrow(host: &Keys, joiner: &Keys, battle: &str, grace_secs: u64) -> WagerEscrow {
        let config = EscrowConfig {
            mint_url: IN_MEMORY_TEST_MINT_URL.to_string(),
            grace_secs,
        };
        let terms = WagerTerms::new(
            &config,
            battle,
            SATS,
            &host.public_key(),
            &joiner.public_key(),
            now(),
        )
        .expect("terms");
        WagerEscrow::new(terms, config).expect("escrow")
    }

    /// Both duelists seal an honest settlement of `battle`:
    /// mutual peer-naming, host named, agreeing digest/winner.
    fn honest_seals(host: &Keys, joiner: &Keys, battle: &str, winner: u8) -> Vec<Event> {
        let digest = hex::encode([0x42u8; 32]);
        let (hp, jp) = (host.public_key().to_hex(), joiner.public_key().to_hex());
        fn mk(digest: &str, battle: &str, winner: u8, hp: &str, peer: &str) -> Attestation {
            Attestation {
                digest: digest.to_string(),
                battle: battle.to_string(),
                winner,
                host: Some(hp.to_string()),
                peer: Some(peer.to_string()),
            }
        }
        vec![
            sign_attestation(host, &mk(&digest, battle, winner, &hp, &jp)).expect("seal"),
            sign_attestation(joiner, &mk(&digest, battle, winner, &hp, &hp)).expect("seal"),
        ]
    }

    fn total(proofs: &Proofs) -> u64 {
        proofs_total(proofs)
    }

    /// All proofs are plain bearer secrets (no NUT-10 conditions).
    fn assert_bearer(proofs: &Proofs) {
        for p in proofs {
            assert!(
                Nut10Secret::try_from(&p.secret).is_err(),
                "payout/refund proofs must be plain bearer secrets"
            );
        }
    }

    // ---------- the done-test battery ----------

    /// (a) honest duel: both lock, honest seals fold, winner paid in
    /// full; and the escrow's winner is the exact duelist the ladder
    /// fold credits (fold-equality pin, keyed only on winner identity).
    #[tokio::test]
    async fn honest_duel_locks_settles_and_pays_winner() {
        let mint = test_mint().await;
        let (host, joiner, battle) = duel();
        let esc = escrow(&host, &joiner, &battle, 3600);
        let t = now();

        // Both sides lock exactly SATS into the 2-of-2 escrow.
        let locked_host = esc
            .lock(&mint, &host.public_key(), faucet(&mint, SATS).await, t)
            .await
            .expect("host lock");
        let locked_joiner = esc
            .lock(&mint, &joiner.public_key(), faucet(&mint, SATS).await, t)
            .await
            .expect("joiner lock");
        assert_eq!(total(&locked_host), SATS);
        assert_eq!(total(&locked_joiner), SATS);
        for p in locked_host.iter().chain(locked_joiner.iter()) {
            assert!(
                Nut10Secret::try_from(&p.secret).is_ok(),
                "locked proofs must carry the NUT-11 condition"
            );
        }
        assert_eq!(
            esc.adjudicate(std::iter::empty(), t),
            EscrowVerdict::Pending,
            "no seals, nothing pays"
        );

        // Honest duel, host (side 0) wins; both seal.
        let seals = honest_seals(&host, &joiner, &battle, 0);
        assert_eq!(
            esc.adjudicate(seals.iter(), t),
            EscrowVerdict::Settle(host.public_key())
        );

        // Fold-equality pin: the ladder's own fold on the same stream
        // credits THE SAME winner (and moves the loser down).
        let table = fold_attestations(seals.iter(), &[], &Default::default(), &Default::default());
        assert_eq!(table.rating(&host.public_key()), Some(1016));
        assert_eq!(table.rating(&joiner.public_key()), Some(984));

        let mut pot = locked_host.clone();
        pot.extend(locked_joiner.clone());
        let payout = esc
            .settle(&mint, &seals, pot, &host, &joiner, t)
            .await
            .expect("honest settlement pays the winner");
        assert_eq!(total(&payout), 2 * SATS, "full pot, zero rake");
        assert_bearer(&payout);
    }

    /// (b) loser never seals: adjudication stays pending, settle is
    /// refused (fail-closed before the window too), and after the
    /// window both sides refund in full at the mint.
    #[tokio::test]
    async fn no_counterseal_times_out_into_full_refunds() {
        let mint = test_mint().await;
        let (host, joiner, battle) = duel();
        // Long-grace escrow object carries the pending / pre-window gate
        // assertions (no funds needed: the gates fire before the mint).
        let esc_open = escrow(&host, &joiner, &battle, 3600);
        // Short-grace escrow carries real locks -> post-window refunds
        // (the mint enforces locktime against real wall-clock time).
        let esc_short = escrow(&host, &joiner, &battle, 2);
        let t = now();

        // Winner's bare claim: one seal only. Nothing folds.
        let one_seal = vec![honest_seals(&host, &joiner, &battle, 0).swap_remove(0)];
        assert_eq!(
            esc_open.adjudicate(one_seal.iter(), t),
            EscrowVerdict::Pending
        );
        let err = esc_open
            .settle(&mint, &one_seal, vec![], &host, &joiner, t)
            .await
            .expect_err("one-sided claim cannot settle");
        assert!(matches!(err, EscrowError::NoSettlement));

        // Refund before the window: refused.
        let early = esc_open
            .refund(&mint, vec![], &host, t)
            .await
            .expect_err("refund before window is refused");
        assert!(matches!(early, EscrowError::RefundNotOpen(_)));

        // Once the window closes, adjudication is Expired and settle is
        // dead even if the seals now land.
        let seals = honest_seals(&host, &joiner, &battle, 0);
        assert_eq!(
            esc_short.adjudicate(seals.iter(), t + 10),
            EscrowVerdict::Expired
        );
        let closed = esc_short
            .settle(&mint, &seals, vec![], &host, &joiner, t + 10)
            .await
            .expect_err("late settlement cannot pay");
        assert!(matches!(closed, EscrowError::SettlementClosed(_)));

        // Real locks on the short-grace escrow; the window passes (real
        // time, mint-enforced); both sides refund in full.
        let locked_host = esc_short
            .lock(&mint, &host.public_key(), faucet(&mint, SATS).await, t)
            .await
            .expect("host lock");
        let locked_joiner = esc_short
            .lock(&mint, &joiner.public_key(), faucet(&mint, SATS).await, t)
            .await
            .expect("joiner lock");
        std::thread::sleep(std::time::Duration::from_secs(3));
        let t = now();
        let refunded_host = esc_short
            .refund(&mint, locked_host, &host, t)
            .await
            .expect("host refund");
        let refunded_joiner = esc_short
            .refund(&mint, locked_joiner, &joiner, t)
            .await
            .expect("joiner refund");
        assert_eq!(total(&refunded_host), SATS);
        assert_eq!(total(&refunded_joiner), SATS);
        assert_bearer(&refunded_host);
        assert_bearer(&refunded_joiner);
    }

    /// (c) forged/mismatched seals: nothing moves; refunds on timeout.
    #[tokio::test]
    async fn forged_and_mismatched_seals_move_nothing_then_refund() {
        let mint = test_mint().await;
        let (host, joiner, battle) = duel();
        let esc_open = escrow(&host, &joiner, &battle, 3600);
        let esc_short = escrow(&host, &joiner, &battle, 2);
        let t = now();

        // Disagreeing pair: joiner seals winner=1 against host's 0.
        let mut seals = honest_seals(&host, &joiner, &battle, 0);
        seals[1] = {
            let att = Attestation {
                digest: hex::encode([0x42u8; 32]),
                battle: battle.clone(),
                winner: 1, // forged: joiner claims the win
                host: Some(host.public_key().to_hex()),
                peer: Some(host.public_key().to_hex()),
            };
            sign_attestation(&joiner, &att).expect("seal")
        };
        assert_eq!(
            esc_open.adjudicate(seals.iter(), t),
            EscrowVerdict::Pending
        );

        // Retro-poison: host equivocates with a second disagreeing seal.
        // Still nothing folds — for anyone.
        let poison = {
            let att = Attestation {
                digest: hex::encode([0x42u8; 32]),
                battle: battle.clone(),
                winner: 1,
                host: Some(host.public_key().to_hex()),
                peer: Some(joiner.public_key().to_hex()),
            };
            sign_attestation(&host, &att).expect("seal")
        };
        let mut poisoned = seals.clone();
        poisoned.push(poison);
        assert_eq!(
            esc_open.adjudicate(poisoned.iter(), t),
            EscrowVerdict::Pending
        );

        // Mob: a third party's agreeing seal on the battle id is inert.
        let impostor = Keys::generate();
        let mob = {
            let att = Attestation {
                digest: hex::encode([0x42u8; 32]),
                battle: battle.clone(),
                winner: 0,
                host: Some(host.public_key().to_hex()),
                peer: Some(host.public_key().to_hex()), // misnamed counterparty
            };
            sign_attestation(&impostor, &att).expect("seal")
        };
        let mut mobbed = honest_seals(&host, &joiner, &battle, 0);
        mobbed.push(mob);
        assert_eq!(
            esc_open.adjudicate(mobbed.iter(), t),
            EscrowVerdict::Settle(host.public_key()),
            "mob seals cannot disturb a formed settlement"
        );

        // On the forged stream: no settle at the gate...
        let err = esc_open
            .settle(&mint, &seals, vec![], &host, &joiner, t)
            .await
            .expect_err("forged seals cannot settle");
        assert!(matches!(err, EscrowError::NoSettlement));

        // ...and with real locks + an expired window, the mint refunds
        // both sides in full.
        let locked_host = esc_short
            .lock(&mint, &host.public_key(), faucet(&mint, SATS).await, t)
            .await
            .expect("host lock");
        let locked_joiner = esc_short
            .lock(&mint, &joiner.public_key(), faucet(&mint, SATS).await, t)
            .await
            .expect("joiner lock");
        std::thread::sleep(std::time::Duration::from_secs(3));
        let back_host = esc_short
            .refund(&mint, locked_host, &host, now())
            .await
            .expect("host refund");
        let back_joiner = esc_short
            .refund(&mint, locked_joiner, &joiner, now())
            .await
            .expect("joiner refund");
        assert_eq!(total(&back_host) + total(&back_joiner), 2 * SATS);
    }

    /// (d) replayed settlement cannot pay twice: the second settle fails
    /// on the mint's spent-input rejection, and a post-settle refund is
    /// likewise dead.
    #[tokio::test]
    async fn settled_escrow_cannot_pay_twice() {
        let mint = test_mint().await;
        let (host, joiner, battle) = duel();
        let esc = escrow(&host, &joiner, &battle, 3600);
        let t = now();

        let locked_host = esc
            .lock(&mint, &host.public_key(), faucet(&mint, SATS).await, t)
            .await
            .expect("host lock");
        let locked_joiner = esc
            .lock(&mint, &joiner.public_key(), faucet(&mint, SATS).await, t)
            .await
            .expect("joiner lock");
        let seals = honest_seals(&host, &joiner, &battle, 0);

        let mut pot = locked_host.clone();
        pot.extend(locked_joiner.clone());
        let payout = esc
            .settle(&mint, &seals, pot.clone(), &host, &joiner, t)
            .await
            .expect("first settle pays");
        assert_eq!(total(&payout), 2 * SATS);

        // Replay: same seals, same locked proofs — the mint rejects the
        // double spend.
        let replay = esc
            .settle(&mint, &seals, pot, &host, &joiner, t)
            .await
            .expect_err("replayed settlement cannot pay twice");
        assert!(matches!(replay, EscrowError::Mint(_)));

        // And the loser cannot claw back from the (already spent) lock —
        // the proofs are spent at the mint, window or no window.
        let clawback = esc
            .refund(&mint, locked_joiner, &joiner, t + 100_000)
            .await
            .expect_err("spent lock cannot refund");
        assert!(matches!(clawback, EscrowError::Mint(_)));
    }

    /// (pin) escrow adjudication reads the SAME fold as the ladder: a
    /// heavy mixed stream (honest pair, sockpuppet noise, second battle)
    /// leaves the ladder crediting exactly the duelist the escrow pays,
    /// and a settlement on a DIFFERENT battle id unlocks nothing here.
    #[tokio::test]
    async fn adjudication_and_ladder_agree_on_winner_identity() {
        let mint = test_mint().await;
        let (host, joiner, battle) = duel();
        let esc = escrow(&host, &joiner, &battle, 3600);
        let t = now();
        let locked_host = esc
            .lock(&mint, &host.public_key(), faucet(&mint, SATS).await, t)
            .await
            .expect("host lock");
        let locked_joiner = esc
            .lock(&mint, &joiner.public_key(), faucet(&mint, SATS).await, t)
            .await
            .expect("joiner lock");

        // A different battle settles in the same stream — inert here.
        let other_battle = hex::encode([0x88u8; 32]);
        let mut stream = honest_seals(&host, &joiner, &other_battle, 1);
        assert_eq!(
            esc.adjudicate(stream.iter(), t),
            EscrowVerdict::Pending,
            "another battle's settlement unlocks nothing"
        );

        // Joiner actually wins THIS battle (winner=1).
        stream.extend(honest_seals(&host, &joiner, &battle, 1));
        // Sockpuppet noise agreeing, misnamed — inert either way.
        let sock = Keys::generate();
        let sock_seal = {
            let att = Attestation {
                digest: hex::encode([0x42u8; 32]),
                battle: battle.clone(),
                winner: 1,
                host: Some(host.public_key().to_hex()),
                peer: Some(host.public_key().to_hex()),
            };
            sign_attestation(&sock, &att).expect("seal")
        };
        stream.push(sock_seal);

        // THE PIN: escrow verdict == the shared fold's winner, and the
        // ladder fold rates THAT SAME duelist ahead of the loser.
        let folded = canonical_settlements(stream.iter(), &Default::default());
        let for_battle = folded
            .iter()
            .find(|s| s.battle() == battle)
            .expect("this battle settles in the shared fold");
        assert_eq!(for_battle.winner(), joiner.public_key());
        assert_eq!(
            esc.adjudicate(stream.iter(), t),
            EscrowVerdict::Settle(for_battle.winner()),
            "escrow pays exactly the duelist the shared fold names"
        );
        let table = fold_attestations(stream.iter(), &[], &Default::default(), &Default::default());
        let wr = table.rating(&for_battle.winner()).expect("winner rated");
        let lr = table.rating(&for_battle.loser()).expect("loser rated");
        assert!(wr > 1016 && lr < 984, "winner up, loser down");
        assert_eq!(table.rating(&sock.public_key()), None);

        // The win pays out whole.
        let mut pot = locked_host.clone();
        pot.extend(locked_joiner.clone());
        let payout = esc
            .settle(&mint, &stream, pot, &host, &joiner, t)
            .await
            .expect("joiner-side win pays out");
        assert_eq!(total(&payout), 2 * SATS);
    }

    /// Boundary: a 2-of-2 release requires BOTH duelist signatures —
    /// the winner alone cannot forge the second witness, even with a
    /// fully folded settlement on the stream.
    #[tokio::test]
    async fn winner_alone_cannot_spend_the_pot() {
        let mint = test_mint().await;
        let (host, joiner, battle) = duel();
        let esc = escrow(&host, &joiner, &battle, 3600);
        let t = now();
        let locked_host = esc
            .lock(&mint, &host.public_key(), faucet(&mint, SATS).await, t)
            .await
            .expect("host lock");
        let seals = honest_seals(&host, &joiner, &battle, 0);
        assert_eq!(
            esc.adjudicate(seals.iter(), t),
            EscrowVerdict::Settle(host.public_key())
        );
        // Hand-sign the settled winner's OWN lock with ONLY the winner's
        // key and try to spend it via a raw swap: 1-of-2 sigs, and the
        // mint must reject.
        let mut stolen = locked_host;
        let sk = cashu_secret_key(&host).unwrap();
        for p in &mut stolen {
            p.sign_p2pk(sk.clone()).unwrap();
        }
        let res = esc.swap(&mint, stolen, None, SATS).await;
        assert!(
            matches!(res, Err(EscrowError::Mint(_))),
            "1-of-2 signature set must be rejected by the mint: {res:?}"
        );
    }

    #[test]
    fn split_amounts_pows_of_two() {
        let parts = split_amounts(42);
        let total: u64 = parts.iter().map(|a| u64::from(*a)).sum();
        assert_eq!(total, 42);
        for p in parts {
            let v = u64::from(p);
            assert!(v.is_power_of_two());
        }
    }

    #[test]
    fn config_defaults_to_test_mint() {
        // Guard: env unset in test harness => test-mint default.
        std::env::remove_var(CRUSADE_MINT_URL_ENV);
        std::env::remove_var(CRUSADE_WAGER_GRACE_SECS_ENV);
        let cfg = EscrowConfig::from_env();
        assert_eq!(cfg.mint_url, IN_MEMORY_TEST_MINT_URL);
        assert_eq!(cfg.grace_secs, DEFAULT_GRACE_SECS);
    }

    #[test]
    fn terms_validation() {
        let a = nostr::Keys::generate();
        let b = nostr::Keys::generate();
        let cfg = EscrowConfig::test_mint();
        let t = now();
        let battle = hex::encode([0xAAu8; 32]);
        assert!(WagerTerms::new(&cfg, &battle, 0, &a.public_key(), &b.public_key(), t).is_err());
        let upper = battle.to_uppercase();
        assert!(upper != battle);
        assert!(WagerTerms::new(&cfg, &upper, 10, &a.public_key(), &b.public_key(), t).is_err());
        assert!(WagerTerms::new(&cfg, &battle, 10, &a.public_key(), &a.public_key(), t).is_err());
        let terms =
            WagerTerms::new(&cfg, &battle, 10, &a.public_key(), &b.public_key(), t).unwrap();
        assert_eq!(terms.refund_after, t + DEFAULT_GRACE_SECS);
        // Only the two duelists can produce deposit conditions.
        let c = nostr::Keys::generate();
        assert!(terms.deposit_conditions(&c.public_key()).is_err());
    }
}
