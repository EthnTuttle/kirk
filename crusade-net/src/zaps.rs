//! NIP-57 zaps ("alms"): kind-9734 zap-request building plus the LNURL-pay
//! switchboard (LUD-06/LUD-16) used to fetch an invoice for that request.
//!
//! INTEGRITY FIREWALL (docs/ZAP-MONETIZATION.md §2): this module is pure
//! request-building/HTTP. It must never import or observe battle engines,
//! RNG, Elo, or matchmaking kinds 9259-9263 — zaps may influence what a
//! player SEES, never what a player IS.
//!
//! Failure is always cosmetic: every fallible step returns a typed
//! [`ZapError`] and the caller's duel flow must never block on it.
//!
//! Signing stays with the caller's keys, exactly like
//! `nostr_transport::encode`: [`ZapRequest::to_event_builder`] builds the
//! unsigned template and the shell signs it with the player's `Keys`
//! (or any `NostrSigner`) at the call site.
//!
//! Transport is injected: crusade-net has no HTTP client dependency
//! (wasm-safe over websocket relays only), so the shell provides an [`Http`]
//! implementation (browser `fetch`, native `reqwest`, ...).

use async_trait::async_trait;
use nostr::nips::nip57::ZapRequestData;
use nostr::prelude::*;
use serde::Deserialize;
use thiserror::Error;

/// Hard cap on any LNURL HTTP response body (64 KiB). Passed to the [`Http`]
/// implementation on every call; implementations MUST stop reading and fail
/// with [`ZapError::TooLarge`] once it is exceeded.
pub const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

/// Every fallible step of the zap flow. All cosmetic: surface as a toast.
#[derive(Debug, Error)]
pub enum ZapError {
    #[error("invalid lightning address or lnurl: {0}")]
    InvalidAddress(String),
    #[error("bad url: {0}")]
    Url(String),
    /// Everything here is https-only; plaintext would leak zap metadata.
    #[error("insecure url (https required): {0}")]
    NotHttps(String),
    /// Transport-level failure reported by the [`Http`] implementation.
    #[error("http: {0}")]
    Http(String),
    #[error("response exceeded {0} bytes")]
    TooLarge(u64),
    /// The LNURL server returned `{"status":"ERROR","reason":...}`.
    #[error("lnurl server error: {0}")]
    Lnurl(String),
    #[error("malformed pay params: {0}")]
    BadParams(String),
    /// The invoice callback lives on a different origin than the LNURL
    /// endpoint — a theft/phishing vector (ZAP-MONETIZATION risk 6). We
    /// never follow it.
    #[error("lnurl callback is cross-origin")]
    CrossOriginCallback,
    #[error("amount {amount} msat outside [{min}, {max}] msat")]
    AmountOutOfRange { amount: u64, min: u64, max: u64 },
    /// The endpoint doesn't advertise `allowsNostr`, so a kind-9734 cannot
    /// be attached.
    #[error("endpoint does not allow nostr zaps")]
    ZapsNotAllowed,
    /// Caller handed something that wasn't a signed kind-9734 event.
    #[error("expected a kind-9734 zap request, got kind {0}")]
    WrongKind(u16),
    #[error("malformed invoice response: {0}")]
    BadInvoice(String),
    #[error("nostr event construction failed: {0}")]
    Nostr(String),
}

/// A NIP-57 zap request template: the kind-9734 event the sender signs to
/// tip `recipient` for `event_id` (the duel attestation event id).
/// Data only — signing stays with the caller's keys.
#[derive(Debug, Clone)]
pub struct ZapRequest {
    /// Recipient's pubkey (`p` tag).
    pub recipient: PublicKey,
    /// Event being tipped (`e` tag) — the duel attestation id.
    pub event_id: EventId,
    /// Intended amount in millisatoshis (`amount` tag; the bolt11 is added
    /// to the 9735 receipt by the recipient's wallet).
    pub amount_msat: u64,
    /// Relays the recipient's wallet should publish the receipt to.
    pub relays: Vec<String>,
    /// Free-form message (the event content).
    pub message: String,
    /// Bech32 lnurl of the recipient's endpoint, when known (`lnurl` tag,
    /// NIP-57 SHOULD).
    pub lnurl: Option<String>,
}

impl ZapRequest {
    pub fn new(recipient: PublicKey, event_id: EventId, amount_msat: u64) -> Self {
        Self {
            recipient,
            event_id,
            amount_msat,
            relays: Vec::new(),
            message: String::new(),
            lnurl: None,
        }
    }

    pub fn relays<I, S>(mut self, relays: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.relays = relays.into_iter().map(Into::into).collect();
        self
    }

    pub fn message<S: Into<String>>(mut self, message: S) -> Self {
        self.message = message.into();
        self
    }

    pub fn lnurl<S: Into<String>>(mut self, lnurl: S) -> Self {
        self.lnurl = Some(lnurl.into());
        self
    }

    /// The unsigned kind-9734 event template, built from nostr 0.35's
    /// vendored NIP-57 types (`ZapRequestData` → `e`/`p`/`amount`/`relays`
    /// tags + content). Pure; sign with the sender's keys:
    /// `zap.to_event_builder().to_event(&keys)`.
    pub fn to_event_builder(&self) -> EventBuilder {
        let mut data = ZapRequestData::new(
            self.recipient,
            self.relays.iter().map(|r| UncheckedUrl::from(r.as_str())),
        )
        .amount(self.amount_msat)
        .message(self.message.clone())
        .event_id(self.event_id);
        if let Some(lnurl) = &self.lnurl {
            data = data.lnurl(lnurl.clone());
        }
        EventBuilder::public_zap_request(data)
    }

    /// Convenience: build + sign with the caller's keys in one step.
    pub fn sign(&self, keys: &Keys) -> Result<Event, ZapError> {
        self.to_event_builder()
            .to_event(keys)
            .map_err(|e| ZapError::Nostr(e.to_string()))
    }
}

/// Minimal HTTP GET abstracted over the host shell (browser fetch on wasm,
/// reqwest/native later). Transport-agnostic by design — crusade-net itself
/// holds no HTTP client dependency.
///
/// SECURITY REQUIREMENTS — the module's cross-origin and response-size
/// invariants rest entirely on the implementation honoring both clauses
/// below; violating either voids those invariants:
/// - an implementation MUST NOT follow a redirect to a different origin
///   than the requested `url` (refusing all redirects is the simplest
///   compliant choice);
/// - an implementation MUST enforce `max_bytes`: stop reading and fail with
///   [`ZapError::TooLarge`] once more than `max_bytes` of body have arrived.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Http {
    async fn get_text(&self, url: &str, max_bytes: u64) -> Result<String, ZapError>;
}

/// Validated LUD-06 payRequest parameters for one recipient endpoint.
/// Constructed by [`fetch_pay_params`] (fields are public so a shell may
/// also build known-good params directly).
#[derive(Debug, Clone)]
pub struct PayParams {
    /// Invoice callback URL (https; same origin as the LNURL endpoint when
    /// built by [`fetch_pay_params`]).
    pub callback: String,
    /// Normalized `host[:port]` of the LNURL endpoint this callback was
    /// validated against. [`request_invoice`] re-checks the callback's
    /// origin against it at call time — belt-braces so even hand-built
    /// PayParams cannot smuggle a signed zap to another origin.
    pub endpoint_host: String,
    /// Minimum payable amount, millisatoshis.
    pub min_sendable_msat: u64,
    /// Maximum payable amount, millisatoshis.
    pub max_sendable_msat: u64,
    /// Whether the endpoint accepts NIP-57 zaps (`allowsNostr`).
    pub allows_nostr: bool,
    /// Server pubkey advertised for kind-9735 receipt verification. This
    /// module never verifies receipts — NIP-57 appendix D signature checking
    /// is a downstream concern (relays/wallets).
    pub nostr_pubkey: Option<String>,
    /// Original bech32 `lnurl1...` when the caller supplied one; echoed on
    /// the callback per NIP-57. `None` for lud16-derived endpoints.
    pub lnurl: Option<String>,
}

/// LUD-06 wire shape (`status:"ERROR"` shares the endpoint).
#[derive(Debug, Deserialize)]
struct PayParamsResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    callback: Option<String>,
    #[serde(default, rename = "minSendable")]
    min_sendable: Option<u64>,
    #[serde(default, rename = "maxSendable")]
    max_sendable: Option<u64>,
    #[serde(default, rename = "allowsNostr")]
    allows_nostr: Option<bool>,
    #[serde(default, rename = "nostrPubkey")]
    nostr_pubkey: Option<String>,
}

/// LUD-06 callback response: the bolt11 invoice is `pr`.
#[derive(Debug, Deserialize)]
struct InvoiceResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    pr: Option<String>,
}

/// LUD-16 endpoint derivation: `name@domain` →
/// `https://domain/.well-known/lnurlp/name`. A bech32 `lnurl1...` string is
/// also accepted (decoded, never dereferenced).
pub fn lud16_to_url(lud16_or_lnurl: &str) -> Result<String, ZapError> {
    Ok(endpoint_from(lud16_or_lnurl)?.url)
}

/// LNURL-pay step 1: resolve the recipient's lightning address/lnurl and
/// fetch+validate its payRequest parameters.
pub async fn fetch_pay_params<H: Http + ?Sized>(
    http: &H,
    lud16_or_lnurl: &str,
) -> Result<PayParams, ZapError> {
    let ep = endpoint_from(lud16_or_lnurl)?;
    let body = http.get_text(&ep.url, MAX_RESPONSE_BYTES).await?;
    let resp: PayParamsResponse = serde_json::from_str(&body)
        .map_err(|e| ZapError::BadParams(format!("not payRequest json: {e}")))?;
    if resp.status.as_deref() == Some("ERROR") {
        return Err(ZapError::Lnurl(
            resp.reason.unwrap_or_else(|| "unknown".into()),
        ));
    }
    if resp.tag.as_deref() != Some("payRequest") {
        return Err(ZapError::BadParams("tag is not payRequest".into()));
    }
    let callback = resp
        .callback
        .ok_or_else(|| ZapError::BadParams("missing callback".into()))?;
    let min = resp
        .min_sendable
        .ok_or_else(|| ZapError::BadParams("missing minSendable".into()))?;
    let max = resp
        .max_sendable
        .ok_or_else(|| ZapError::BadParams("missing maxSendable".into()))?;
    if min > max {
        return Err(ZapError::BadParams("minSendable > maxSendable".into()));
    }
    let cb = parse_url(&callback)?;
    require_https(&cb)?;
    if !same_origin(&ep.parsed, &cb) {
        return Err(ZapError::CrossOriginCallback);
    }
    Ok(PayParams {
        callback,
        endpoint_host: match ep.parsed.port {
            Some(port) => format!("{}:{port}", ep.parsed.host),
            None => ep.parsed.host.clone(),
        },
        min_sendable_msat: min,
        max_sendable_msat: max,
        allows_nostr: resp.allows_nostr.unwrap_or(false),
        nostr_pubkey: resp.nostr_pubkey,
        lnurl: ep.lnurl,
    })
}

/// LNURL-pay step 2: ask the callback for a bolt11 invoice of
/// `amount_msat`, attaching the signed kind-9734 zap request via the
/// `nostr` query param (NIP-57). Returns the bolt11 string; paying it is
/// the caller's wallet's problem (share intent / `lightning:` URI).
pub async fn request_invoice<H: Http + ?Sized>(
    http: &H,
    params: &PayParams,
    amount_msat: u64,
    zap_request: &Event,
) -> Result<String, ZapError> {
    if zap_request.kind != Kind::ZapRequest {
        return Err(ZapError::WrongKind(zap_request.kind.as_u16()));
    }
    if !params.allows_nostr {
        return Err(ZapError::ZapsNotAllowed);
    }
    if amount_msat < params.min_sendable_msat || amount_msat > params.max_sendable_msat {
        return Err(ZapError::AmountOutOfRange {
            amount: amount_msat,
            min: params.min_sendable_msat,
            max: params.max_sendable_msat,
        });
    }
    let cb = parse_url(&params.callback)?;
    require_https(&cb)?;
    // Belt-braces: re-validate the callback against the endpoint origin even
    // here, so a hand-built PayParams that skipped fetch_pay_params cannot
    // smuggle the signed zap to another origin.
    let ep = parse_url(&format!("https://{}", params.endpoint_host))?;
    if !same_origin(&ep, &cb) {
        return Err(ZapError::CrossOriginCallback);
    }
    let sep = if params.callback.contains('?') {
        '&'
    } else {
        '?'
    };
    let mut url = params.callback.clone();
    url.push(sep);
    url.push_str("amount=");
    url.push_str(&amount_msat.to_string());
    url.push_str("&nostr=");
    url.push_str(&percent_encode(&zap_request.as_json()));
    if let Some(lnurl) = &params.lnurl {
        // Hand-built PayParams can carry anything; encode the value so a
        // '&' or '#' inside it can never smuggle extra query params. Real
        // bech32 lnurls pass through unchanged — their charset is url-safe.
        url.push_str("&lnurl=");
        url.push_str(&percent_encode(lnurl));
    }
    let body = http.get_text(&url, MAX_RESPONSE_BYTES).await?;
    let resp: InvoiceResponse = serde_json::from_str(&body)
        .map_err(|e| ZapError::BadInvoice(format!("invoice json: {e}")))?;
    if resp.status.as_deref() == Some("ERROR") {
        return Err(ZapError::Lnurl(
            resp.reason.unwrap_or_else(|| "unknown".into()),
        ));
    }
    let pr = resp
        .pr
        .ok_or_else(|| ZapError::BadInvoice("missing pr".into()))?;
    let bolt11_shaped = pr.len() >= 6 && pr.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("ln"));
    if !bolt11_shaped {
        return Err(ZapError::BadInvoice("pr is not bolt11-shaped".into()));
    }
    Ok(pr)
}

/// A resolved, validated LNURL-pay endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Endpoint {
    /// The exact https URL to GET.
    url: String,
    /// Normalized origin of `url` (https enforced).
    parsed: ParsedUrl,
    /// Original bech32 input, when the caller supplied one.
    lnurl: Option<String>,
}

/// Resolve a lightning address or bech32 lnurl to its endpoint, without any
/// network access.
fn endpoint_from(input: &str) -> Result<Endpoint, ZapError> {
    let bad = |m: &str| ZapError::InvalidAddress(m.into());
    let input = input.trim();
    if input.is_empty() {
        return Err(bad("empty lightning address"));
    }
    // LUD-01 bech32 form.
    if input.len() >= 6 && input[..6].eq_ignore_ascii_case("lnurl1") {
        let url = lnurl_decode(input)?;
        let parsed = parse_url(&url)?;
        require_https(&parsed)?;
        return Ok(Endpoint {
            url,
            parsed,
            lnurl: Some(input.to_ascii_lowercase()),
        });
    }
    // LUD-16 form: exactly one '@', non-empty on both sides.
    let (first, last) = (input.find('@'), input.rfind('@'));
    let at = match (first, last) {
        (Some(f), Some(l)) if f == l && f > 0 && f + 1 < input.len() => f,
        _ => return Err(bad("expected <name>@<domain> or a bech32 lnurl")),
    };
    let (name, domain) = (&input[..at], &input[at + 1..]);
    if name.len() > 64
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-' || b == b'+')
    {
        return Err(bad("bad characters in lightning address name"));
    }
    // Validate the domain via a throwaway URL, then rebuild the well-known
    // endpoint from the normalized host.
    let probe = format!("https://{domain}");
    let parsed =
        parse_url(&probe).map_err(|_| ZapError::InvalidAddress(format!("bad domain: {domain}")))?;
    let mut url = format!("https://{}", parsed.host);
    if let Some(port) = parsed.port {
        url.push_str(&format!(":{port}"));
    }
    url.push_str("/.well-known/lnurlp/");
    url.push_str(&percent_encode(name));
    Ok(Endpoint {
        url,
        parsed,
        lnurl: None,
    })
}

/// Just-enough URL parsing for LUD-06 validation: scheme, normalized host,
/// port. Not a general URL crate (the `url` crate isn't a dependency), and
/// deliberately strict — LNURL endpoints are plain domains.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedUrl {
    /// Lowercase scheme (only "https" survives [`require_https`]).
    scheme: String,
    /// Lowercase ASCII host. IPv6/userinfo are rejected, never normalized.
    host: String,
    port: Option<u16>,
}

fn parse_url(url: &str) -> Result<ParsedUrl, ZapError> {
    let err = || ZapError::Url(url.into());
    let (scheme, rest) = url.split_once("://").ok_or_else(err)?;
    let valid_scheme = scheme
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_lowercase())
        && scheme.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'+' || b == b'-' || b == b'.'
        });
    if !valid_scheme {
        return Err(err());
    }
    let auth_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..auth_end];
    if authority.is_empty() || authority.contains('@') {
        // No host, or userinfo (phishing vector: never allowed).
        return Err(err());
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() => {
            let port: u16 = p.parse().map_err(|_| err())?;
            if port == 0 {
                return Err(err());
            }
            (h, Some(port))
        }
        Some(_) => return Err(err()),
        None => (authority, None),
    };
    let valid_host = !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        && host.split('.').all(|label| !label.is_empty())
        // Reject IP-literal hosts: https endpoints must be names, and an IP
        // spelling is the classic way to disguise one. A host whose labels
        // are ALL numeric catches dotted-quad IPv4, inet_aton dword
        // (2130706433) and octal (0177.0.0.1) forms; names that merely
        // start numeric ("1password.example") keep a letter label and pass.
        && !host
            .split('.')
            .all(|label| !label.is_empty() && label.bytes().all(|b| b.is_ascii_digit()));
    if !valid_host {
        return Err(err());
    }
    Ok(ParsedUrl {
        scheme: scheme.into(),
        host: host.to_ascii_lowercase(),
        port,
    })
}

fn require_https(parsed: &ParsedUrl) -> Result<(), ZapError> {
    if parsed.scheme != "https" {
        return Err(ZapError::NotHttps(format!(
            "{}://{}",
            parsed.scheme, parsed.host
        )));
    }
    Ok(())
}

fn effective_port(scheme: &str, port: Option<u16>) -> Option<u16> {
    port.or(match scheme {
        "https" => Some(443),
        "http" => Some(80),
        _ => None,
    })
}

/// Same origin = same scheme, host, and (default-normalized) port.
fn same_origin(a: &ParsedUrl, b: &ParsedUrl) -> bool {
    a.scheme == b.scheme
        && a.host == b.host
        && effective_port(&a.scheme, a.port) == effective_port(&b.scheme, b.port)
}

/// RFC 3986 unreserved-only percent encoding (UTF-8 bytes). Hand-rolled:
/// no URL crate in this crate's dependency set.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

/// LUD-01 bech32 decode (classic bech32, `lnurl` HRP) → the inner URL
/// string. Hand-rolled and decode-only: nostr's bech32 dependency is not
/// re-exported and the manifest is owned elsewhere; we never mint lnurls.
fn lnurl_decode(input: &str) -> Result<String, ZapError> {
    const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    let bad = |m: &str| ZapError::InvalidAddress(m.into());
    if input.is_empty() || input.len() > 4096 {
        return Err(bad("lnurl: bad length"));
    }
    if input.bytes().any(|b| !(b'!'..=b'~').contains(&b)) {
        return Err(bad("bech32: non-printable characters"));
    }
    if input.bytes().any(|b| b.is_ascii_uppercase())
        && input.bytes().any(|b| b.is_ascii_lowercase())
    {
        return Err(bad("bech32: mixed case"));
    }
    let lower = input.to_ascii_lowercase();
    let sep = lower
        .rfind('1')
        .ok_or_else(|| bad("bech32: no separator"))?;
    // HRP of 1+ chars, then 6+ chars of checksum.
    if sep == 0 || sep + 7 > lower.len() {
        return Err(bad("bech32: bad hrp or checksum length"));
    }
    let hrp = &lower[..sep];
    if hrp != "lnurl" {
        return Err(ZapError::InvalidAddress(format!(
            "expected lnurl hrp, got {hrp}"
        )));
    }
    let mut values = Vec::with_capacity(lower.len() - sep - 1);
    for c in lower[sep + 1..].bytes() {
        let v = CHARSET
            .iter()
            .position(|&x| x == c)
            .ok_or_else(|| bad("bech32: char outside charset"))?;
        values.push(v as u8);
    }
    let mut expanded: Vec<u8> = hrp.bytes().map(|b| b >> 5).collect();
    expanded.push(0);
    expanded.extend(hrp.bytes().map(|b| b & 31));
    expanded.extend_from_slice(&values);
    if bech32_polymod(&expanded) != 1 {
        return Err(bad("bech32: bad checksum"));
    }
    let data = &values[..values.len() - 6];
    let bytes = convert_bits_5_to_8(data)?;
    String::from_utf8(bytes).map_err(|_| bad("lnurl payload is not utf-8"))
}

fn bech32_polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
    let mut chk: u32 = 1;
    for &v in values {
        let top = chk >> 25;
        chk = ((chk & 0x1ffffff) << 5) ^ (v as u32);
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

/// BIP-173 convertbits, from=5, to=8, pad=false.
fn convert_bits_5_to_8(data: &[u8]) -> Result<Vec<u8>, ZapError> {
    const MAXV: u32 = 0xff;
    const MAXACC: u32 = (1 << (5 + 8 - 1)) - 1;
    let bad = || ZapError::InvalidAddress("bech32: bad padding".into());
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut out = Vec::with_capacity(data.len() * 5 / 8);
    for &v in data {
        let v = v as u32;
        acc = ((acc << 5) | v) & MAXACC;
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & MAXV) as u8);
        }
    }
    if bits >= 5 || ((acc << (8 - bits)) & MAXV) != 0 {
        return Err(bad());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const LUD16: &str = "alice@wallet.example";
    const WELL_KNOWN: &str = "https://wallet.example/.well-known/lnurlp/alice";
    const CALLBACK: &str = "https://wallet.example/lnurl/cb";
    /// bech32 (LUD-01 `Variant::Bech32`, hrp "lnurl") of WELL_KNOWN.
    const ALICE_LNURL: &str =
        "lnurl1dp68gurn8ghj7ampd3kx2apwv4uxzmtsd3jj7tnhv4kxctttdehhwm30d3h82unvwqhkzmrfvdjskx890f";
    const PAY_PARAMS_JSON: &str = r#"{
        "tag": "payRequest",
        "callback": "https://wallet.example/lnurl/cb",
        "minSendable": 1000,
        "maxSendable": 1000000,
        "allowsNostr": true,
        "nostrPubkey": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "metadata": "[[\"text/plain\",\"alms\"]]"
    }"#;
    const INVOICE_JSON: &str =
        r#"{"pr": "lnbc210n1pjdqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq"}"#;

    /// Canned LNURL server. Exact-match routes first, then prefix routes.
    #[derive(Default)]
    struct MockHttp {
        exact: Vec<(String, String)>,
        prefix: Vec<(String, String)>,
        calls: Mutex<Vec<(String, u64)>>,
    }

    impl MockHttp {
        fn with_exact(mut self, url: &str, body: &str) -> Self {
            self.exact.push((url.into(), body.into()));
            self
        }
        fn with_prefix_prefix(mut self, prefix: &str, body: &str) -> Self {
            self.prefix.push((prefix.into(), body.into()));
            self
        }
        fn calls(&self) -> Vec<(String, u64)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl Http for MockHttp {
        async fn get_text(&self, url: &str, max_bytes: u64) -> Result<String, ZapError> {
            self.calls.lock().unwrap().push((url.into(), max_bytes));
            for (u, body) in &self.exact {
                if u == url {
                    return Ok(body.clone());
                }
            }
            for (p, body) in &self.prefix {
                if url.starts_with(p) {
                    return Ok(body.clone());
                }
            }
            Err(ZapError::Http(format!("mock: no route for {url}")))
        }
    }

    fn bob_pubkey() -> PublicKey {
        // secp256k1 generator point x — a known-valid x-only pubkey constant.
        PublicKey::from_hex("79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798")
            .unwrap()
    }

    fn attestation_id() -> EventId {
        EventId::from_slice(&[7u8; 32]).unwrap()
    }

    fn test_zap() -> ZapRequest {
        ZapRequest::new(bob_pubkey(), attestation_id(), 21_000)
            .relays(["wss://relay.example", "wss://relay2.example"])
            .message("well fought brother")
            .lnurl(ALICE_LNURL)
    }

    /// Hand-built params as a shell might construct them (bypassing fetch).
    fn hand_params() -> PayParams {
        PayParams {
            callback: CALLBACK.into(),
            endpoint_host: "wallet.example".into(),
            min_sendable_msat: 1000,
            max_sendable_msat: 1_000_000,
            allows_nostr: true,
            nostr_pubkey: None,
            lnurl: None,
        }
    }

    /// The signed kind-9734 round-trips through JSON with identical
    /// e/p/amount/relays tags + content — the exact read path a NIP-57
    /// wallet server uses.
    #[test]
    fn zap_request_tag_roundtrip() {
        let keys = Keys::generate();
        let zap = test_zap();
        let event = zap.sign(&keys).unwrap();
        assert_eq!(event.kind, Kind::ZapRequest);
        assert_eq!(event.kind.as_u16(), 9734);
        assert_eq!(event.content, "well fought brother");

        let json = event.as_json();
        let parsed = Event::from_json(&json).unwrap();
        assert_eq!(parsed, event); // sig/id survive the roundtrip
        parsed.verify().unwrap();

        let e_tags: Vec<EventId> = parsed
            .tags
            .iter()
            .filter_map(|t| match t.as_standardized() {
                Some(TagStandard::Event { event_id, .. }) => Some(*event_id),
                _ => None,
            })
            .collect();
        assert_eq!(e_tags, vec![attestation_id()]);

        let p_tags: Vec<PublicKey> = parsed
            .tags
            .iter()
            .filter_map(|t| match t.as_standardized() {
                Some(TagStandard::PublicKey { public_key, .. }) => Some(*public_key),
                _ => None,
            })
            .collect();
        assert_eq!(p_tags, vec![bob_pubkey()]);

        let amounts: Vec<u64> = parsed
            .tags
            .iter()
            .filter_map(|t| match t.as_standardized() {
                Some(TagStandard::Amount { millisats, .. }) => Some(*millisats),
                _ => None,
            })
            .collect();
        assert_eq!(amounts, vec![21_000]);

        let relays: Vec<String> = parsed
            .tags
            .iter()
            .filter_map(|t| match t.as_standardized() {
                Some(TagStandard::Relays(urls)) => Some(
                    urls.iter()
                        .map(|u| u.as_str().to_string())
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(relays, vec!["wss://relay.example", "wss://relay2.example"]);

        let lnurls: Vec<&String> = parsed
            .tags
            .iter()
            .filter_map(|t| match t.as_standardized() {
                Some(TagStandard::Lnurl(l)) => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(lnurls, vec![&ALICE_LNURL.to_string()]);
    }

    #[test]
    fn lud16_endpoint_derivation() {
        assert_eq!(
            lud16_to_url("satoshi@wallet.com").unwrap(),
            "https://wallet.com/.well-known/lnurlp/satoshi"
        );
        // Name case is preserved; the domain is normalized.
        assert_eq!(
            lud16_to_url("AL-ice_9@Sub.Domain.Example").unwrap(),
            "https://sub.domain.example/.well-known/lnurlp/AL-ice_9"
        );
        // An explicit port survives (test wallets).
        assert_eq!(
            lud16_to_url("alice@wallet.example:8443").unwrap(),
            "https://wallet.example:8443/.well-known/lnurlp/alice"
        );
    }

    #[test]
    fn bad_lud16_errors() {
        for input in [
            "",
            "   ",
            "noatsign",
            "user@",
            "@domain",
            "a@b@c",
            "bad name@x.com",
            "user@exa mple.com",
            "user@",
            "user@.example",
            "user@example..com",
            "user@example.com:0",
            "user@example.com:notaport",
            "//evil.com/x@y",
        ] {
            assert!(
                lud16_to_url(input).is_err(),
                "input {input:?} must be rejected"
            );
        }
    }

    #[test]
    fn bech32_lnurl_decodes() {
        let ep = endpoint_from(ALICE_LNURL).unwrap();
        assert_eq!(ep.url, WELL_KNOWN);
        assert_eq!(ep.lnurl.as_deref(), Some(ALICE_LNURL));
        assert_eq!(ep.parsed.scheme, "https");
        assert_eq!(ep.parsed.host, "wallet.example");
        assert_eq!(ep.parsed.port, None);
        // Uppercase is fine (bech32 forbids mixed case only).
        assert_eq!(
            endpoint_from(&ALICE_LNURL.to_ascii_uppercase())
                .unwrap()
                .url,
            WELL_KNOWN
        );
        // Corrupted checksum must fail.
        let mut corrupted = ALICE_LNURL.to_string();
        let last = corrupted.len() - 1;
        let flipped = if &corrupted[last..] == "q" { "p" } else { "q" };
        corrupted.replace_range(last.., flipped);
        assert!(endpoint_from(&corrupted).is_err());
        // Wrong HRP is not an lnurl.
        assert!(endpoint_from("lnbc1dp68gurn8ghj7ampd3kx2apwv4uxzmtsd3jj7tnhv4kxctttdehhwm30d3h82unvwqhkzmrfvdjskx890f").is_err());
    }

    #[tokio::test]
    async fn fetch_pay_params_happy() {
        let mock = MockHttp::default().with_exact(WELL_KNOWN, PAY_PARAMS_JSON);
        let params = fetch_pay_params(&mock, LUD16).await.unwrap();
        assert_eq!(params.callback, CALLBACK);
        assert_eq!(params.endpoint_host, "wallet.example");
        assert_eq!(params.min_sendable_msat, 1000);
        assert_eq!(params.max_sendable_msat, 1_000_000);
        assert!(params.allows_nostr);
        assert_eq!(
            params.nostr_pubkey.as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert_eq!(params.lnurl, None);
        let calls = mock.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, WELL_KNOWN);
        // The response-size cap is plumbed through to the transport.
        assert_eq!(calls[0].1, MAX_RESPONSE_BYTES);
    }

    #[tokio::test]
    async fn fetch_pay_params_accepts_bech32_lnurl() {
        let mock = MockHttp::default().with_exact(WELL_KNOWN, PAY_PARAMS_JSON);
        let params = fetch_pay_params(&mock, ALICE_LNURL).await.unwrap();
        assert_eq!(params.callback, CALLBACK);
        assert_eq!(params.lnurl.as_deref(), Some(ALICE_LNURL));
    }

    #[tokio::test]
    async fn cross_origin_callback_rejected() {
        for bad_callback in [
            "https://evil.example/cb",            // different host
            "https://wallet.example.evil.com/cb", // suffix host
            "https://wallet.example:4443/cb",     // different port
            "http://wallet.example/cb",           // scheme downgrade
        ] {
            let json = PAY_PARAMS_JSON.replace(CALLBACK, bad_callback);
            let mock = MockHttp::default().with_exact(WELL_KNOWN, &json);
            let err = fetch_pay_params(&mock, LUD16).await.unwrap_err();
            assert!(
                matches!(err, ZapError::CrossOriginCallback | ZapError::NotHttps(_)),
                "{bad_callback} must be rejected, got {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn fetch_pay_params_error_shapes() {
        // LNURL-level error.
        let mock = MockHttp::default()
            .with_exact(WELL_KNOWN, r#"{"status":"ERROR","reason":"no zaps here"}"#);
        let err = fetch_pay_params(&mock, LUD16).await.unwrap_err();
        assert!(matches!(err, ZapError::Lnurl(ref r) if r == "no zaps here"));

        // Wrong tag.
        let mock = MockHttp::default().with_exact(
            WELL_KNOWN,
            &PAY_PARAMS_JSON.replace("payRequest", "channelRequest"),
        );
        let err = fetch_pay_params(&mock, LUD16).await.unwrap_err();
        assert!(matches!(err, ZapError::BadParams(_)));

        // min > max is nonsense.
        let mock = MockHttp::default().with_exact(
            WELL_KNOWN,
            &PAY_PARAMS_JSON.replace(r#""minSendable": 1000"#, r#""minSendable": 2000000"#),
        );
        let err = fetch_pay_params(&mock, LUD16).await.unwrap_err();
        assert!(matches!(err, ZapError::BadParams(_)));

        // Transport failure propagates.
        let mock = MockHttp::default();
        let err = fetch_pay_params(&mock, LUD16).await.unwrap_err();
        assert!(matches!(err, ZapError::Http(_)));

        // Not json at all.
        let mock = MockHttp::default().with_exact(WELL_KNOWN, "<html>oops</html>");
        let err = fetch_pay_params(&mock, LUD16).await.unwrap_err();
        assert!(matches!(err, ZapError::BadParams(_)));
    }

    #[tokio::test]
    async fn request_invoice_happy_query_shape() {
        let keys = Keys::generate();
        let signed = test_zap().sign(&keys).unwrap();
        let params = hand_params();
        let mock = MockHttp::default().with_prefix_prefix(CALLBACK, INVOICE_JSON);
        let pr = request_invoice(&mock, &params, 21_000, &signed)
            .await
            .unwrap();
        assert!(pr.starts_with("lnbc"));

        let calls = mock.calls();
        assert_eq!(calls.len(), 1);
        let (url, _) = &calls[0];
        // amount + nostr (percent-encoded signed-event json) params in order.
        let prefix = format!("{CALLBACK}?amount=21000&nostr=");
        assert!(url.starts_with(&prefix), "got {url}");
        // Hex stays unencoded in the URL: both tagged ids are visible to the
        // server, exactly as NIP-57 intends.
        assert!(url.contains(&attestation_id().to_hex()));
        assert!(url.contains(&bob_pubkey().to_hex()));
        assert!(url.contains("%7B%22id%22%3A%22")); // {"id":"... percent-encoded
    }

    #[tokio::test]
    async fn request_invoice_echoes_lnurl_param() {
        let keys = Keys::generate();
        let signed = test_zap().sign(&keys).unwrap();
        let params = PayParams {
            lnurl: Some(ALICE_LNURL.into()),
            ..hand_params()
        };
        let mock = MockHttp::default().with_prefix_prefix(CALLBACK, INVOICE_JSON);
        request_invoice(&mock, &params, 21_000, &signed)
            .await
            .unwrap();
        let calls = mock.calls();
        assert!(calls[0].0.contains(&format!("&lnurl={ALICE_LNURL}")));
    }

    #[tokio::test]
    async fn request_invoice_failure_paths() {
        let keys = Keys::generate();
        let signed = test_zap().sign(&keys).unwrap();
        let mut params = hand_params();

        // Amount outside the advertised window — never even hits the wire.
        let mock = MockHttp::default();
        for bad_amount in [999u64, 1_000_001] {
            let err = request_invoice(&mock, &params, bad_amount, &signed)
                .await
                .unwrap_err();
            assert!(matches!(err, ZapError::AmountOutOfRange { .. }));
        }
        assert!(mock.calls().is_empty());

        // Endpoint doesn't take zaps.
        params.allows_nostr = false;
        let mock = MockHttp::default();
        let err = request_invoice(&mock, &params, 21_000, &signed)
            .await
            .unwrap_err();
        assert!(matches!(err, ZapError::ZapsNotAllowed));
        params.allows_nostr = true;

        // LNURL ERROR status on the callback.
        let mock = MockHttp::default().with_prefix_prefix(
            CALLBACK,
            r#"{"status":"ERROR","reason":"amount too small"}"#,
        );
        let err = request_invoice(&mock, &params, 21_000, &signed)
            .await
            .unwrap_err();
        assert!(matches!(err, ZapError::Lnurl(ref r) if r == "amount too small"));

        // Garbage invoice shapes.
        for body in [
            r#"{"pr": 42}"#,                    // wrong type
            r#"{"successAction":{}}"#,          // missing pr
            r#"{"pr": "not-a-bolt11-string"}"#, // wrong prefix
        ] {
            let mock = MockHttp::default().with_prefix_prefix(CALLBACK, body);
            let err = request_invoice(&mock, &params, 21_000, &signed)
                .await
                .unwrap_err();
            assert!(
                matches!(err, ZapError::BadInvoice(_)),
                "{body} must be BadInvoice, got {err:?}"
            );
        }

        // Hand-built params can still not sneak in a plaintext callback.
        params.callback = CALLBACK.replace("https://", "http://");
        let mock = MockHttp::default();
        let err = request_invoice(&mock, &params, 21_000, &signed)
            .await
            .unwrap_err();
        assert!(matches!(err, ZapError::NotHttps(_)));
    }

    #[tokio::test]
    async fn request_invoice_rejects_non_zap_kind() {
        let keys = Keys::generate();
        // A kind-1 note (any non-9734 kind) must NOT be sent as a zap.
        let not_a_zap = EventBuilder::new(Kind::from_u16(1), "", [])
            .to_event(&keys)
            .unwrap();
        let params = hand_params();
        let mock = MockHttp::default();
        let err = request_invoice(&mock, &params, 21_000, &not_a_zap)
            .await
            .unwrap_err();
        assert!(matches!(err, ZapError::WrongKind(1)));
    }

    #[test]
    fn ip_literal_hosts_rejected() {
        for input in [
            "user@127.0.0.1",  // dotted-quad IPv4
            "user@2130706433", // inet_aton dword spelling of 127.0.0.1
            "user@0177.0.0.1", // octal spelling
            "user@1.2.3.4",
        ] {
            assert!(lud16_to_url(input).is_err(), "{input:?} must be rejected");
        }
        // Names that merely START numeric are domains, not literals.
        assert!(lud16_to_url("user@1password.example").is_ok());
    }

    #[tokio::test]
    async fn request_invoice_rechecks_callback_origin() {
        // Belt-braces: a hand-built PayParams whose callback points off the
        // endpoint origin is refused even though fetch validation was
        // skipped entirely.
        let keys = Keys::generate();
        let signed = test_zap().sign(&keys).unwrap();
        let mut params = hand_params();
        params.callback = "https://evil.example/cb".into();
        let mock = MockHttp::default();
        let err = request_invoice(&mock, &params, 21_000, &signed)
            .await
            .unwrap_err();
        assert!(matches!(err, ZapError::CrossOriginCallback));
        assert!(mock.calls().is_empty(), "must never hit the wire");
    }

    #[tokio::test]
    async fn request_invoice_encodes_smuggled_lnurl_params() {
        // A hand-built lnurl value containing query delimiters must be
        // percent-encoded into inertness, not parsed as extra params.
        let keys = Keys::generate();
        let signed = test_zap().sign(&keys).unwrap();
        let params = PayParams {
            lnurl: Some("lnurl1funky&callback=evil#x".into()),
            ..hand_params()
        };
        let mock = MockHttp::default().with_prefix_prefix(CALLBACK, INVOICE_JSON);
        request_invoice(&mock, &params, 21_000, &signed)
            .await
            .unwrap();
        let url = &mock.calls()[0].0;
        assert!(
            url.contains("&lnurl=lnurl1funky%26callback%3Devil%23x"),
            "got {url}"
        );
        assert!(!url.contains("&callback="), "got {url}");
    }
}
