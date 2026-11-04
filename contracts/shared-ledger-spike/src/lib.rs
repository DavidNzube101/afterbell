use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::collections::{LookupMap, UnorderedSet};
use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::{
    env, near, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError, PromiseOrValue,
};
use serde::{Deserialize, Serialize};

mod engine_math;

const NEP_413_MESSAGE_PREFIX: u32 = (1 << 31) + 413;
const MAX_AUTHORIZATION_TTL_MS: u64 = 5 * 60 * 1000;
const MAX_AGENT_ACTION_TTL_MS: u64 = 60 * 1000;
const MAX_AGENT_POSITIONS: u64 = 32;
const MAX_AGENT_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const MAX_MARKET_PRICE_AGE_MS: u64 = 60_000;
const SHARED_VAULT_RECORD_ID: &str = "protocol:vault:usdc";
const TESTNET_PRICE_SIGNER_PUBLIC_KEY: [u8; 32] = [
    139, 77, 195, 229, 231, 45, 144, 52, 17, 118, 68, 169, 124, 217, 68, 39, 243, 233, 180, 42,
    255, 20, 33, 168, 211, 193, 42, 34, 61, 25, 147, 213,
];
const OWNER_PREFIX: &[u8] = b"afterbell:a1:owner:";
const OWNER_KEY_ROTATION_NONCE_PREFIX: &[u8] = b"afterbell:a1:owner_rotation_nonce:";
const WITHDRAWAL_NONCE_PREFIX: &[u8] = b"afterbell:a1:withdrawal_nonce:";
const AGENT_CONFIG_PREFIX: &[u8] = b"afterbell:a1:agent_config:";
const USER_AGENT_PREFIX: &[u8] = b"afterbell:a1:user_agent:";
const AGENT_ADMIN_NONCE_PREFIX: &[u8] = b"afterbell:a1:agent_admin_nonce:";
const AGENT_ACTION_NONCE_PREFIX: &[u8] = b"afterbell:a1:agent_action_nonce:";
const AGENT_POSITION_PREFIX: &[u8] = b"afterbell:a1:agent_position:";
const AGENT_OPEN_NOTIONAL_PREFIX: &[u8] = b"afterbell:a1:agent_open_notional:";
const AGENT_POSITION_COUNT_PREFIX: &[u8] = b"afterbell:a1:agent_position_count:";
const AGENT_RESERVED_MARGIN_PREFIX: &[u8] = b"afterbell:a1:agent_reserved_margin:";
const MARKET_PRICE_PREFIX: &[u8] = b"afterbell:a1:market_price:";
const VAULT_RESERVED_PREFIX: &[u8] = b"afterbell:a1:vault_reserved_payout";
const STORAGE_CREDIT_PREFIX: &[u8] = b"afterbell:a1:storage_credit:";

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct SharedLedger {
    usdc_token: AccountId,
    balances: LookupMap<String, u128>,
    // Retained for serialized-state compatibility with deployed A1. New
    // deposits no longer read or add entries to this legacy set.
    #[allow(dead_code)]
    processed_deposits: UnorderedSet<String>,
}

/// Public wallet identity used by the shared account contract to authorize a
/// logical user record. This is a verification key/address, never a secret.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerKey {
    NearEd25519 { public_key: [u8; 32] },
    SolanaEd25519 { public_key: [u8; 32] },
    Ethereum { address: [u8; 20] },
}

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde")]
struct DepositMessage {
    // Opaque logical record id; normally namespaced by wallet type and address.
    account_id: String,
    // Correlation id for the off-chain indexer. NEAR receipt execution provides
    // on-chain exactly-once processing, so this must not grow a permanent set.
    deposit_id: String,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct FtTransferArgs {
    receiver_id: AccountId,
    amount: U128,
    memo: Option<String>,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct WithdrawCallbackArgs {
    account_id: String,
    amount: U128,
}

#[derive(Serialize)]
struct StorageWithdrawCallbackArgs {
    account_id: String,
    amount: U128,
}

#[near(serializers = [json])]
pub struct SigningPayload {
    pub message: String,
    /// NEP-413 nonce. Solana and EVM sign the UTF-8 `message` directly.
    pub nonce: Base64VecU8,
    /// NEP-413 recipient. For this contract it is the shared A1 account.
    pub recipient: AccountId,
}

/// User-authorized capability envelope for an off-chain trading agent. The
/// agent is a logical record inside A1, not a separate NEAR account.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfig {
    pub user_record_id: String,
    pub agent_record_id: String,
    pub agent_key: OwnerKey,
    pub allowed_markets: Vec<String>,
    pub max_position_usdc: U128,
    pub max_open_notional_usdc: U128,
    /// Leverage in basis points (10,000 = 1x; 50,000 = 5x).
    pub max_leverage_bps: u32,
    pub max_allocation_usdc: U128,
    pub expires_at_ms: u64,
    pub enabled: bool,
}

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPosition {
    pub position_id: String,
    pub agent_record_id: String,
    pub market_id: String,
    pub is_long: bool,
    pub notional_usdc: U128,
    pub margin_usdc: U128,
    pub leverage_bps: u32,
    /// Missing on positions opened through the earlier reservation-only path.
    #[serde(default)]
    pub entry_price: Option<U128>,
}

/// Latest accepted price update signed by A1's configured publisher.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketPrice {
    pub market_id: String,
    /// Price uses 8 decimal places (for example, $100.00 is 10_000_000_000).
    pub price: U128,
    pub observed_at_ms: u64,
    pub sequence: u64,
}

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionPnl {
    pub entry_price: U128,
    pub mark_price: U128,
    pub mark_observed_at_ms: u64,
    /// Signed raw USDC units, rounded toward zero; excludes fees and funding.
    pub unrealized_pnl_usdc: String,
}

#[near]
impl SharedLedger {
    #[init]
    pub fn new(usdc_token: AccountId) -> Self {
        Self {
            usdc_token,
            balances: LookupMap::new(b"b"),
            processed_deposits: UnorderedSet::new(b"d"),
        }
    }

    pub fn balance_of(&self, account_id: String) -> U128 {
        U128(self.balances.get(&account_id).unwrap_or_default())
    }

    /// NEAR-denominated credit available to pay for this record's future state.
    pub fn storage_credit_of(&self, account_id: String) -> U128 {
        U128(read_storage_credit(&account_id))
    }

    /// Anyone may sponsor storage for a record. Attached NEAR first covers the
    /// storage-credit entry itself; the remainder is available to that record.
    #[payable]
    pub fn storage_deposit_for(&mut self, account_id: String) -> U128 {
        assert_valid_record_id(&account_id);
        let attached = env::attached_deposit().as_yoctonear();
        assert!(attached > 0, "storage deposit must be positive");
        let before = env::storage_usage();
        let current = read_storage_credit(&account_id);
        let updated = current
            .checked_add(attached)
            .expect("storage credit overflow");
        write_storage_credit(&account_id, updated);
        let growth = env::storage_usage().saturating_sub(before);
        let entry_cost = env::storage_byte_cost()
            .as_yoctonear()
            .checked_mul(u128::from(growth))
            .expect("storage credit entry cost overflow");
        assert!(
            updated >= entry_cost,
            "attached NEAR does not cover storage entry"
        );
        let available = updated - entry_cost;
        write_storage_credit(&account_id, available);
        U128(available)
    }

    pub fn owner_of(&self, account_id: String) -> Option<OwnerKey> {
        read_owner(&account_id)
    }

    /// Last accepted withdrawal nonce for a logical user record (zero initially).
    pub fn withdrawal_nonce(&self, account_id: String) -> u64 {
        read_withdrawal_nonce(&account_id)
    }

    pub fn owner_key_rotation_nonce(&self, account_id: String) -> u64 {
        read_owner_key_rotation_nonce(&account_id)
    }

    pub fn agent_record_of(&self, user_record_id: String) -> Option<String> {
        read_agent_record(&user_record_id)
    }

    pub fn agent_config(&self, agent_record_id: String) -> Option<AgentConfig> {
        read_agent_config(&agent_record_id)
    }

    pub fn agent_admin_nonce(&self, user_record_id: String) -> u64 {
        read_agent_admin_nonce(&user_record_id)
    }

    pub fn agent_action_nonce(&self, agent_record_id: String) -> u64 {
        read_agent_action_nonce(&agent_record_id)
    }

    pub fn agent_open_notional(&self, agent_record_id: String) -> U128 {
        U128(read_agent_open_notional(&agent_record_id))
    }

    pub fn agent_reserved_margin(&self, agent_record_id: String) -> U128 {
        U128(read_agent_reserved_margin(&agent_record_id))
    }

    pub fn vault_balance(&self) -> U128 {
        U128(
            self.balances
                .get(&SHARED_VAULT_RECORD_ID.to_string())
                .unwrap_or_default(),
        )
    }

    pub fn vault_reserved_payout(&self) -> U128 {
        U128(read_vault_reserved_payout())
    }

    pub fn vault_available_payout(&self) -> U128 {
        let balance = self
            .balances
            .get(&SHARED_VAULT_RECORD_ID.to_string())
            .unwrap_or_default();
        U128(balance.saturating_sub(read_vault_reserved_payout()))
    }

    pub fn vault_record_id(&self) -> String {
        SHARED_VAULT_RECORD_ID.to_string()
    }

    pub fn agent_position(
        &self,
        agent_record_id: String,
        position_id: String,
    ) -> Option<AgentPosition> {
        read_agent_position(&agent_record_id, &position_id)
    }

    /// Return the exact message the testnet price publisher must sign.
    pub fn market_price_signing_payload(
        &self,
        market_id: String,
        price: U128,
        observed_at_ms: u64,
        sequence: u64,
    ) -> String {
        assert_valid_market_id(&market_id);
        assert!(price.0 > 0, "market price must be positive");
        market_price_message(&market_id, price, observed_at_ms, sequence)
    }

    /// Accept a fresh, strictly increasing market price signed by A1's
    /// testnet publisher key. This key is a prototype trust root, not a
    /// production oracle configuration.
    pub fn publish_market_price(
        &mut self,
        market_id: String,
        price: U128,
        observed_at_ms: u64,
        sequence: u64,
        signature: Base64VecU8,
    ) {
        assert_valid_market_id(&market_id);
        assert!(price.0 > 0, "market price must be positive");
        let now_ms = env::block_timestamp() / 1_000_000;
        assert!(
            observed_at_ms <= now_ms,
            "market price timestamp is in the future"
        );
        assert!(
            now_ms.saturating_sub(observed_at_ms) <= MAX_MARKET_PRICE_AGE_MS,
            "market price is stale"
        );
        let previous = read_market_price(&market_id);
        assert!(
            previous
                .as_ref()
                .is_none_or(|update| sequence > update.sequence),
            "market price sequence must increase"
        );
        let signature: &[u8; 64] = signature
            .as_ref()
            .try_into()
            .expect("invalid market price signature length");
        let message = market_price_message(&market_id, price, observed_at_ms, sequence);
        assert!(
            env::ed25519_verify(
                signature,
                message.as_bytes(),
                &TESTNET_PRICE_SIGNER_PUBLIC_KEY
            ),
            "invalid market price signature"
        );
        let storage_before = env::storage_usage();
        write_market_price(&MarketPrice {
            market_id,
            price,
            observed_at_ms,
            sequence,
        });
        settle_storage_credit("protocol:market_prices", storage_before);
    }

    pub fn market_price(&self, market_id: String) -> Option<MarketPrice> {
        assert_valid_market_id(&market_id);
        read_market_price(&market_id)
    }

    /// Calculate unrealized PnL for a price-aware agent position using A1's
    /// latest accepted market price. Returns None for legacy positions or if
    /// the market has no accepted price. Stale prices are rejected.
    pub fn agent_position_pnl(
        &self,
        agent_record_id: String,
        position_id: String,
    ) -> Option<PositionPnl> {
        let position = read_agent_position(&agent_record_id, &position_id)?;
        let entry_price = position.entry_price?;
        let mark = read_market_price(&position.market_id)?;
        let now_ms = env::block_timestamp() / 1_000_000;
        if now_ms.saturating_sub(mark.observed_at_ms) > MAX_MARKET_PRICE_AGE_MS {
            return None;
        }
        Some(PositionPnl {
            entry_price,
            mark_price: mark.price,
            mark_observed_at_ms: mark.observed_at_ms,
            unrealized_pnl_usdc: calculate_linear_pnl_raw(
                position.notional_usdc,
                entry_price,
                mark.price,
                position.is_long,
            ),
        })
    }

    /// Calculate indicative linear-perpetual PnL in raw USDC units.
    /// Prices use 8 decimals; notional is the position's USDC notional.
    /// The supplied mark price is not authenticated and this view never settles funds.
    pub fn calculate_linear_pnl(
        &self,
        notional_usdc: U128,
        entry_price: U128,
        mark_price: U128,
        is_long: bool,
    ) -> String {
        calculate_linear_pnl_raw(notional_usdc, entry_price, mark_price, is_long)
    }

    pub fn agent_open_position_signing_payload(
        &self,
        agent_record_id: String,
        position_id: String,
        market_id: String,
        is_long: bool,
        notional_usdc: U128,
        leverage_bps: u32,
        expires_at_ms: u64,
    ) -> SigningPayload {
        assert_valid_record_id(&position_id);
        assert_valid_agent_action_expiry(expires_at_ms);
        let nonce = read_agent_action_nonce(&agent_record_id)
            .checked_add(1)
            .expect("agent action nonce overflow");
        SigningPayload {
            message: agent_open_message(
                &agent_record_id,
                &position_id,
                &market_id,
                is_long,
                notional_usdc,
                leverage_bps,
                nonce,
                expires_at_ms,
            ),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn agent_market_open_signing_payload(
        &self,
        agent_record_id: String,
        position_id: String,
        market_id: String,
        is_long: bool,
        notional_usdc: U128,
        leverage_bps: u32,
        reference_price: U128,
        price_sequence: u64,
        max_slippage_bps: u16,
        expires_at_ms: u64,
    ) -> SigningPayload {
        let price = require_fresh_market_price(&market_id);
        assert_eq!(
            price.sequence, price_sequence,
            "market price sequence changed"
        );
        assert_eq!(price.price, reference_price, "reference price mismatch");
        assert!(
            max_slippage_bps <= 1_000,
            "slippage limit exceeds 10 percent"
        );
        assert_valid_record_id(&position_id);
        assert_valid_agent_action_expiry(expires_at_ms);
        let nonce = read_agent_action_nonce(&agent_record_id)
            .checked_add(1)
            .expect("agent action nonce overflow");
        SigningPayload {
            message: agent_market_open_message(
                &agent_record_id,
                &position_id,
                &market_id,
                is_long,
                notional_usdc,
                leverage_bps,
                reference_price,
                price_sequence,
                max_slippage_bps,
                nonce,
                expires_at_ms,
            ),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    pub fn agent_close_position_signing_payload(
        &self,
        agent_record_id: String,
        position_id: String,
        expires_at_ms: u64,
    ) -> SigningPayload {
        assert_valid_agent_action_expiry(expires_at_ms);
        let position = read_agent_position(&agent_record_id, &position_id)
            .expect("agent position does not exist");
        let nonce = read_agent_action_nonce(&agent_record_id)
            .checked_add(1)
            .expect("agent action nonce overflow");
        SigningPayload {
            message: agent_close_message(&position, nonce, expires_at_ms),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    pub fn owner_close_agent_position_signing_payload(
        &self,
        user_record_id: String,
        position_id: String,
        expires_at_ms: u64,
    ) -> SigningPayload {
        assert_valid_expiry(expires_at_ms);
        let agent_record_id =
            read_agent_record(&user_record_id).expect("user agent record is not configured");
        let position = read_agent_position(&agent_record_id, &position_id)
            .expect("agent position does not exist");
        let nonce = read_agent_admin_nonce(&user_record_id)
            .checked_add(1)
            .expect("agent admin nonce overflow");
        SigningPayload {
            message: owner_close_agent_message(&user_record_id, &position, nonce, expires_at_ms),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn agent_market_close_signing_payload(
        &self,
        agent_record_id: String,
        position_id: String,
        reference_price: U128,
        price_sequence: u64,
        max_slippage_bps: u16,
        expires_at_ms: u64,
    ) -> SigningPayload {
        let position = read_agent_position(&agent_record_id, &position_id)
            .expect("agent position does not exist");
        assert!(
            position.entry_price.is_some(),
            "position is not price-aware"
        );
        let nonce = read_agent_action_nonce(&agent_record_id)
            .checked_add(1)
            .expect("agent action nonce overflow");
        SigningPayload {
            message: market_close_message(
                "agent",
                "",
                &position,
                reference_price,
                price_sequence,
                max_slippage_bps,
                nonce,
                expires_at_ms,
            ),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn owner_market_close_signing_payload(
        &self,
        user_record_id: String,
        position_id: String,
        reference_price: U128,
        price_sequence: u64,
        max_slippage_bps: u16,
        expires_at_ms: u64,
    ) -> SigningPayload {
        let agent_record_id =
            read_agent_record(&user_record_id).expect("user agent record is not configured");
        let position = read_agent_position(&agent_record_id, &position_id)
            .expect("agent position does not exist");
        assert!(
            position.entry_price.is_some(),
            "position is not price-aware"
        );
        let nonce = read_agent_admin_nonce(&user_record_id)
            .checked_add(1)
            .expect("agent admin nonce overflow");
        SigningPayload {
            message: market_close_message(
                "owner",
                &user_record_id,
                &position,
                reference_price,
                price_sequence,
                max_slippage_bps,
                nonce,
                expires_at_ms,
            ),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    /// Build the user's signing payload for creating the linked agent record
    /// and setting its initial capability limits.
    #[allow(clippy::too_many_arguments)]
    pub fn agent_setup_signing_payload(
        &self,
        user_record_id: String,
        agent_record_id: String,
        agent_key: OwnerKey,
        allowed_markets: Vec<String>,
        max_position_usdc: U128,
        max_open_notional_usdc: U128,
        max_leverage_bps: u32,
        max_allocation_usdc: U128,
        expires_at_ms: u64,
    ) -> SigningPayload {
        assert_valid_agent_config(
            &user_record_id,
            &agent_record_id,
            &allowed_markets,
            max_position_usdc,
            max_open_notional_usdc,
            max_leverage_bps,
            max_allocation_usdc,
            expires_at_ms,
        );
        let nonce = read_agent_admin_nonce(&user_record_id)
            .checked_add(1)
            .expect("agent admin nonce overflow");
        SigningPayload {
            message: agent_setup_message(
                &user_record_id,
                &agent_record_id,
                &agent_key,
                &allowed_markets,
                max_position_usdc,
                max_open_notional_usdc,
                max_leverage_bps,
                max_allocation_usdc,
                nonce,
                expires_at_ms,
            ),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    /// Create the user's linked agent record. Only the user's registered
    /// wallet can authorize its key and capability limits. The agent record
    /// deliberately has no owner entry, so it cannot use `withdraw`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_agent_record(
        &mut self,
        user_record_id: String,
        agent_record_id: String,
        agent_key: OwnerKey,
        allowed_markets: Vec<String>,
        max_position_usdc: U128,
        max_open_notional_usdc: U128,
        max_leverage_bps: u32,
        max_allocation_usdc: U128,
        expires_at_ms: u64,
        nonce: u64,
        signature: Base64VecU8,
    ) {
        assert_valid_agent_config(
            &user_record_id,
            &agent_record_id,
            &allowed_markets,
            max_position_usdc,
            max_open_notional_usdc,
            max_leverage_bps,
            max_allocation_usdc,
            expires_at_ms,
        );
        let user_owner = read_owner(&user_record_id).expect("user owner is not registered");
        if let Some(existing_agent) = read_agent_record(&user_record_id) {
            let existing_config =
                read_agent_config(&existing_agent).expect("existing agent config is missing");
            assert!(!existing_config.enabled, "user already has an active agent");
            assert_eq!(
                read_agent_position_count(&existing_agent),
                0,
                "close agent positions before replacing the agent"
            );
        }
        assert!(
            read_owner(&agent_record_id).is_none() && read_agent_config(&agent_record_id).is_none(),
            "agent record cannot have an external owner"
        );
        let expected_nonce = read_agent_admin_nonce(&user_record_id)
            .checked_add(1)
            .expect("agent admin nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid agent admin nonce");
        let message = agent_setup_message(
            &user_record_id,
            &agent_record_id,
            &agent_key,
            &allowed_markets,
            max_position_usdc,
            max_open_notional_usdc,
            max_leverage_bps,
            max_allocation_usdc,
            nonce,
            expires_at_ms,
        );
        assert!(
            verify_owner_signature(&user_owner, &message, signature.as_ref(), nonce),
            "invalid agent setup signature"
        );

        let storage_before = env::storage_usage();
        write_agent_config(
            &agent_record_id,
            &AgentConfig {
                user_record_id: user_record_id.clone(),
                agent_record_id: agent_record_id.clone(),
                agent_key,
                allowed_markets,
                max_position_usdc,
                max_open_notional_usdc,
                max_leverage_bps,
                max_allocation_usdc,
                expires_at_ms,
                enabled: true,
            },
        );
        write_agent_record(&user_record_id, &agent_record_id);
        write_agent_admin_nonce(&user_record_id, nonce);
        settle_storage_credit(&user_record_id, storage_before);
    }

    pub fn agent_funding_signing_payload(
        &self,
        user_record_id: String,
        amount: U128,
        expires_at_ms: u64,
    ) -> SigningPayload {
        assert!(amount.0 > 0, "funding amount must be greater than zero");
        assert_valid_expiry(expires_at_ms);
        let agent_record_id =
            read_agent_record(&user_record_id).expect("user agent record is not configured");
        let nonce = read_agent_admin_nonce(&user_record_id)
            .checked_add(1)
            .expect("agent admin nonce overflow");
        SigningPayload {
            message: agent_funding_message(
                &user_record_id,
                &agent_record_id,
                amount,
                nonce,
                expires_at_ms,
            ),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    /// Move USDC ledger credit from the user record into its agent record.
    /// This is an internal accounting move; it makes no external token call.
    pub fn fund_agent_record(
        &mut self,
        user_record_id: String,
        amount: U128,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) {
        assert!(amount.0 > 0, "funding amount must be greater than zero");
        let agent_record_id =
            read_agent_record(&user_record_id).expect("user agent record is not configured");
        let config = read_agent_config(&agent_record_id).expect("agent config is missing");
        assert!(config.enabled, "agent is disabled");
        assert_valid_expiry(expires_at_ms);
        assert!(
            env::block_timestamp() / 1_000_000 < config.expires_at_ms,
            "agent authorization expired"
        );
        let expected_nonce = read_agent_admin_nonce(&user_record_id)
            .checked_add(1)
            .expect("agent admin nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid agent admin nonce");
        let message = agent_funding_message(
            &user_record_id,
            &agent_record_id,
            amount,
            nonce,
            expires_at_ms,
        );
        let owner = read_owner(&user_record_id).expect("user owner is not registered");
        assert!(
            verify_owner_signature(&owner, &message, signature.as_ref(), nonce),
            "invalid agent funding signature"
        );

        let user_balance = self.balances.get(&user_record_id).unwrap_or_default();
        assert!(user_balance >= amount.0, "insufficient user balance");
        let agent_balance = self.balances.get(&agent_record_id).unwrap_or_default();
        let updated_agent_balance = agent_balance
            .checked_add(amount.0)
            .expect("agent balance overflow");
        let total_allocated = updated_agent_balance
            .checked_add(read_agent_reserved_margin(&agent_record_id))
            .expect("agent allocation overflow");
        assert!(
            total_allocated <= config.max_allocation_usdc.0,
            "agent allocation limit exceeded"
        );
        let storage_before = env::storage_usage();
        self.balances
            .insert(&user_record_id, &(user_balance - amount.0));
        self.balances
            .insert(&agent_record_id, &updated_agent_balance);
        write_agent_admin_nonce(&user_record_id, nonce);
        settle_storage_credit(&user_record_id, storage_before);
    }

    /// Reserve margin for a reservation-only agent position. Price-aware
    /// positions use `open_agent_market_position` and the funded-vault path.
    #[allow(clippy::too_many_arguments)]
    pub fn open_agent_position(
        &mut self,
        agent_record_id: String,
        position_id: String,
        market_id: String,
        is_long: bool,
        notional_usdc: U128,
        leverage_bps: u32,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) -> U128 {
        assert_valid_record_id(&position_id);
        let config = read_agent_config(&agent_record_id).expect("agent config is missing");
        assert!(config.enabled, "agent is disabled");
        let now_ms = env::block_timestamp() / 1_000_000;
        assert!(now_ms < config.expires_at_ms, "agent authorization expired");
        assert_valid_agent_action_expiry(expires_at_ms);
        assert!(
            expires_at_ms <= config.expires_at_ms,
            "agent authorization expired"
        );
        assert!(
            config.allowed_markets.contains(&market_id),
            "market is not allowed for this agent"
        );
        assert!(notional_usdc.0 > 0, "position notional must be positive");
        assert!(
            notional_usdc.0 <= config.max_position_usdc.0,
            "max position limit exceeded"
        );
        assert!(
            leverage_bps >= 10_000 && leverage_bps <= config.max_leverage_bps,
            "agent leverage limit exceeded"
        );
        assert!(
            read_agent_position(&agent_record_id, &position_id).is_none(),
            "position id already exists"
        );
        let expected_nonce = read_agent_action_nonce(&agent_record_id)
            .checked_add(1)
            .expect("agent action nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid agent action nonce");
        let message = agent_open_message(
            &agent_record_id,
            &position_id,
            &market_id,
            is_long,
            notional_usdc,
            leverage_bps,
            nonce,
            expires_at_ms,
        );
        assert!(
            verify_owner_signature(&config.agent_key, &message, signature.as_ref(), nonce),
            "invalid agent action signature"
        );

        let new_open_notional = read_agent_open_notional(&agent_record_id)
            .checked_add(notional_usdc.0)
            .expect("agent open notional overflow");
        assert!(
            new_open_notional <= config.max_open_notional_usdc.0,
            "max open notional limit exceeded"
        );
        let margin = notional_usdc
            .0
            .checked_mul(10_000)
            .and_then(|value| value.checked_add(u128::from(leverage_bps) - 1))
            .expect("required margin overflow")
            / u128::from(leverage_bps);
        let agent_balance = self.balances.get(&agent_record_id).unwrap_or_default();
        assert!(agent_balance >= margin, "insufficient agent free balance");
        let reserved_margin = read_agent_reserved_margin(&agent_record_id)
            .checked_add(margin)
            .expect("reserved margin overflow");
        let total_allocated = reserved_margin
            .checked_add(agent_balance - margin)
            .expect("agent allocation overflow");
        assert!(
            total_allocated <= config.max_allocation_usdc.0,
            "agent allocation limit exceeded"
        );

        let storage_before = env::storage_usage();
        self.balances
            .insert(&agent_record_id, &(agent_balance - margin));
        write_agent_reserved_margin(&agent_record_id, reserved_margin);
        write_agent_open_notional(&agent_record_id, new_open_notional);
        write_agent_position(
            &agent_record_id,
            &position_id,
            &AgentPosition {
                position_id: position_id.clone(),
                agent_record_id: agent_record_id.clone(),
                market_id,
                is_long,
                notional_usdc,
                margin_usdc: U128(margin),
                leverage_bps,
                entry_price: None,
            },
        );
        let position_count = read_agent_position_count(&agent_record_id)
            .checked_add(1)
            .expect("agent position count overflow");
        assert!(
            position_count <= MAX_AGENT_POSITIONS,
            "agent position count limit exceeded"
        );
        write_agent_position_count(&agent_record_id, position_count);
        write_agent_action_nonce(&agent_record_id, nonce);
        settle_storage_credit(&config.user_record_id, storage_before);
        U128(margin)
    }

    /// Open a price-aware agent position using the latest signed market
    /// price, bounded by a slippage limit included in the agent signature.
    #[allow(clippy::too_many_arguments)]
    pub fn open_agent_market_position(
        &mut self,
        agent_record_id: String,
        position_id: String,
        market_id: String,
        is_long: bool,
        notional_usdc: U128,
        leverage_bps: u32,
        reference_price: U128,
        price_sequence: u64,
        max_slippage_bps: u16,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) -> U128 {
        assert_valid_record_id(&position_id);
        assert_valid_market_id(&market_id);
        assert!(
            max_slippage_bps <= 1_000,
            "slippage limit exceeds 10 percent"
        );
        let config = read_agent_config(&agent_record_id).expect("agent config is missing");
        assert!(config.enabled, "agent is disabled");
        let now_ms = env::block_timestamp() / 1_000_000;
        assert!(now_ms < config.expires_at_ms, "agent authorization expired");
        assert_valid_agent_action_expiry(expires_at_ms);
        assert!(
            expires_at_ms <= config.expires_at_ms,
            "agent authorization expired"
        );
        assert!(
            config.allowed_markets.contains(&market_id),
            "market is not allowed for this agent"
        );
        assert!(notional_usdc.0 > 0, "position notional must be positive");
        assert!(
            notional_usdc.0 <= config.max_position_usdc.0,
            "max position limit exceeded"
        );
        assert!(
            leverage_bps >= 10_000 && leverage_bps <= config.max_leverage_bps,
            "agent leverage limit exceeded"
        );
        assert!(
            read_agent_position(&agent_record_id, &position_id).is_none(),
            "position id already exists"
        );

        let price = require_fresh_market_price(&market_id);
        assert!(
            price.sequence >= price_sequence,
            "market price sequence regressed"
        );
        assert!(reference_price.0 > 0, "reference price must be positive");
        let price_delta = price.price.0.abs_diff(reference_price.0);
        assert!(
            price_delta.checked_mul(10_000).is_some_and(|delta| {
                delta
                    <= reference_price
                        .0
                        .saturating_mul(u128::from(max_slippage_bps))
            }),
            "market price exceeded signed slippage"
        );

        let expected_nonce = read_agent_action_nonce(&agent_record_id)
            .checked_add(1)
            .expect("agent action nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid agent action nonce");
        let message = agent_market_open_message(
            &agent_record_id,
            &position_id,
            &market_id,
            is_long,
            notional_usdc,
            leverage_bps,
            reference_price,
            price_sequence,
            max_slippage_bps,
            nonce,
            expires_at_ms,
        );
        assert!(
            verify_owner_signature(&config.agent_key, &message, signature.as_ref(), nonce),
            "invalid agent action signature"
        );

        let new_open_notional = read_agent_open_notional(&agent_record_id)
            .checked_add(notional_usdc.0)
            .expect("agent open notional overflow");
        assert!(
            new_open_notional <= config.max_open_notional_usdc.0,
            "max open notional limit exceeded"
        );
        let margin = notional_usdc
            .0
            .checked_mul(10_000)
            .and_then(|value| value.checked_add(u128::from(leverage_bps) - 1))
            .expect("required margin overflow")
            / u128::from(leverage_bps);
        let agent_balance = self.balances.get(&agent_record_id).unwrap_or_default();
        assert!(agent_balance >= margin, "insufficient agent free balance");
        let vault_balance = self
            .balances
            .get(&SHARED_VAULT_RECORD_ID.to_string())
            .unwrap_or_default();
        let vault_reserved = read_vault_reserved_payout();
        assert!(
            vault_balance.saturating_sub(vault_reserved) >= notional_usdc.0,
            "insufficient unreserved vault liquidity"
        );
        let reserved_margin = read_agent_reserved_margin(&agent_record_id)
            .checked_add(margin)
            .expect("reserved margin overflow");
        assert!(
            reserved_margin
                .checked_add(agent_balance - margin)
                .is_some_and(|total| total <= config.max_allocation_usdc.0),
            "agent allocation limit exceeded"
        );

        let storage_before = env::storage_usage();
        self.balances
            .insert(&agent_record_id, &(agent_balance - margin));
        write_vault_reserved_payout(
            vault_reserved
                .checked_add(notional_usdc.0)
                .expect("vault reserve overflow"),
        );
        write_agent_reserved_margin(&agent_record_id, reserved_margin);
        write_agent_open_notional(&agent_record_id, new_open_notional);
        write_agent_position(
            &agent_record_id,
            &position_id,
            &AgentPosition {
                position_id: position_id.clone(),
                agent_record_id: agent_record_id.clone(),
                market_id,
                is_long,
                notional_usdc,
                margin_usdc: U128(margin),
                leverage_bps,
                entry_price: Some(price.price),
            },
        );
        let position_count = read_agent_position_count(&agent_record_id)
            .checked_add(1)
            .expect("agent position count overflow");
        assert!(
            position_count <= MAX_AGENT_POSITIONS,
            "agent position limit exceeded"
        );
        write_agent_position_count(&agent_record_id, position_count);
        write_agent_action_nonce(&agent_record_id, nonce);
        settle_storage_credit(&config.user_record_id, storage_before);
        U128(margin)
    }

    /// Close one fully reserved agent position with the agent's signature.
    /// The remaining collateral returns to the user directly after revocation.
    pub fn close_agent_position(
        &mut self,
        agent_record_id: String,
        position_id: String,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) -> U128 {
        let position = read_agent_position(&agent_record_id, &position_id)
            .expect("agent position does not exist");
        assert!(
            position.entry_price.is_none(),
            "price-aware positions require market close"
        );
        let config = read_agent_config(&agent_record_id).expect("agent config is missing");
        assert_valid_agent_action_expiry(expires_at_ms);
        assert!(
            expires_at_ms <= config.expires_at_ms,
            "agent authorization expired"
        );
        let expected_nonce = read_agent_action_nonce(&agent_record_id)
            .checked_add(1)
            .expect("agent action nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid agent action nonce");
        let message = agent_close_message(&position, nonce, expires_at_ms);
        assert!(
            verify_owner_signature(&config.agent_key, &message, signature.as_ref(), nonce),
            "invalid agent action signature"
        );
        let storage_before = env::storage_usage();
        write_agent_action_nonce(&agent_record_id, nonce);
        let released = release_agent_position(&mut self.balances, &position, &config);
        settle_storage_credit(&config.user_record_id, storage_before);
        released
    }

    #[allow(clippy::too_many_arguments)]
    pub fn close_agent_market_position(
        &mut self,
        agent_record_id: String,
        position_id: String,
        reference_price: U128,
        price_sequence: u64,
        max_slippage_bps: u16,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) -> U128 {
        let position = read_agent_position(&agent_record_id, &position_id)
            .expect("agent position does not exist");
        assert!(
            position.entry_price.is_some(),
            "position is not price-aware"
        );
        let config = read_agent_config(&agent_record_id).expect("agent config is missing");
        assert!(config.enabled, "agent is disabled");
        assert!(
            env::block_timestamp() / 1_000_000 < config.expires_at_ms,
            "agent authorization expired"
        );
        assert_valid_agent_action_expiry(expires_at_ms);
        assert!(
            expires_at_ms <= config.expires_at_ms,
            "agent authorization expired"
        );
        let expected_nonce = read_agent_action_nonce(&agent_record_id)
            .checked_add(1)
            .expect("agent action nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid agent action nonce");
        let message = market_close_message(
            "agent",
            "",
            &position,
            reference_price,
            price_sequence,
            max_slippage_bps,
            nonce,
            expires_at_ms,
        );
        assert!(
            verify_owner_signature(&config.agent_key, &message, signature.as_ref(), nonce),
            "invalid agent market close signature"
        );
        validate_close_mark(&position, reference_price, price_sequence, max_slippage_bps);
        let storage_before = env::storage_usage();
        write_agent_action_nonce(&agent_record_id, nonce);
        let payout = settle_market_position(
            &mut self.balances,
            &position,
            &config,
            require_fresh_market_price(&position.market_id).price,
        );
        settle_storage_credit(&config.user_record_id, storage_before);
        payout
    }

    /// The user's wallet can close an agent position, including after the
    /// agent has been disabled or its authorization has expired.
    pub fn owner_close_agent_position(
        &mut self,
        user_record_id: String,
        position_id: String,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) -> U128 {
        let agent_record_id =
            read_agent_record(&user_record_id).expect("user agent record is not configured");
        let position = read_agent_position(&agent_record_id, &position_id)
            .expect("agent position does not exist");
        assert!(
            position.entry_price.is_none(),
            "price-aware positions require market close"
        );
        let expected_nonce = read_agent_admin_nonce(&user_record_id)
            .checked_add(1)
            .expect("agent admin nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid agent admin nonce");
        assert_valid_expiry(expires_at_ms);
        let message = owner_close_agent_message(&user_record_id, &position, nonce, expires_at_ms);
        let owner = read_owner(&user_record_id).expect("user owner is not registered");
        assert!(
            verify_owner_signature(&owner, &message, signature.as_ref(), nonce),
            "invalid owner close signature"
        );
        let config = read_agent_config(&agent_record_id).expect("agent config is missing");
        let storage_before = env::storage_usage();
        write_agent_admin_nonce(&user_record_id, nonce);
        let released = release_agent_position(&mut self.balances, &position, &config);
        settle_storage_credit(&user_record_id, storage_before);
        released
    }

    #[allow(clippy::too_many_arguments)]
    pub fn owner_close_agent_market_position(
        &mut self,
        user_record_id: String,
        position_id: String,
        reference_price: U128,
        price_sequence: u64,
        max_slippage_bps: u16,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) -> U128 {
        let agent_record_id =
            read_agent_record(&user_record_id).expect("user agent record is not configured");
        let position = read_agent_position(&agent_record_id, &position_id)
            .expect("agent position does not exist");
        assert!(
            position.entry_price.is_some(),
            "position is not price-aware"
        );
        let expected_nonce = read_agent_admin_nonce(&user_record_id)
            .checked_add(1)
            .expect("agent admin nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid agent admin nonce");
        assert_valid_expiry(expires_at_ms);
        let message = market_close_message(
            "owner",
            &user_record_id,
            &position,
            reference_price,
            price_sequence,
            max_slippage_bps,
            nonce,
            expires_at_ms,
        );
        let owner = read_owner(&user_record_id).expect("user owner is not registered");
        assert!(
            verify_owner_signature(&owner, &message, signature.as_ref(), nonce),
            "invalid owner market close signature"
        );
        validate_close_mark(&position, reference_price, price_sequence, max_slippage_bps);
        let config = read_agent_config(&agent_record_id).expect("agent config is missing");
        let storage_before = env::storage_usage();
        write_agent_admin_nonce(&user_record_id, nonce);
        let payout = settle_market_position(
            &mut self.balances,
            &position,
            &config,
            require_fresh_market_price(&position.market_id).price,
        );
        settle_storage_credit(&user_record_id, storage_before);
        payout
    }

    pub fn agent_revocation_signing_payload(
        &self,
        user_record_id: String,
        expires_at_ms: u64,
    ) -> SigningPayload {
        let agent_record_id =
            read_agent_record(&user_record_id).expect("user agent record is not configured");
        assert_valid_expiry(expires_at_ms);
        let nonce = read_agent_admin_nonce(&user_record_id)
            .checked_add(1)
            .expect("agent admin nonce overflow");
        SigningPayload {
            message: agent_revocation_message(
                &user_record_id,
                &agent_record_id,
                nonce,
                expires_at_ms,
            ),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    /// Disable new agent actions and return its free logical balance. Margin
    /// reserved by open positions remains locked; the user can close those
    /// positions and the collateral then returns directly to their record.
    pub fn revoke_agent(
        &mut self,
        user_record_id: String,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) -> U128 {
        let agent_record_id =
            read_agent_record(&user_record_id).expect("user agent record is not configured");
        let expected_nonce = read_agent_admin_nonce(&user_record_id)
            .checked_add(1)
            .expect("agent admin nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid agent admin nonce");
        assert_valid_expiry(expires_at_ms);
        let message =
            agent_revocation_message(&user_record_id, &agent_record_id, nonce, expires_at_ms);
        let owner = read_owner(&user_record_id).expect("user owner is not registered");
        assert!(
            verify_owner_signature(&owner, &message, signature.as_ref(), nonce),
            "invalid agent revocation signature"
        );

        let mut config = read_agent_config(&agent_record_id).expect("agent config is missing");
        config.enabled = false;
        let reclaimed = self.balances.get(&agent_record_id).unwrap_or_default();
        let user_balance = self.balances.get(&user_record_id).unwrap_or_default();
        let updated_user_balance = user_balance
            .checked_add(reclaimed)
            .expect("user balance overflow");
        let storage_before = env::storage_usage();
        self.balances.insert(&agent_record_id, &0);
        self.balances.insert(&user_record_id, &updated_user_balance);
        write_agent_config(&agent_record_id, &config);
        write_agent_admin_nonce(&user_record_id, nonce);
        settle_storage_credit(&user_record_id, storage_before);
        U128(reclaimed)
    }

    /// Return the exact message and NEP-413 envelope a wallet needs to sign to
    /// bind its public key to a logical user record. Registration is one-time.
    pub fn owner_registration_signing_payload(
        &self,
        account_id: String,
        owner: OwnerKey,
        expires_at_ms: u64,
    ) -> SigningPayload {
        assert_valid_record_id(&account_id);
        SigningPayload {
            message: registration_message(&account_id, &owner, expires_at_ms),
            nonce: Base64VecU8::from([0u8; 32].to_vec()),
            recipient: env::current_account_id(),
        }
    }

    /// Return the domain-bound message that both the current and proposed
    /// owner keys must sign to rotate a logical record's owner key.
    pub fn owner_key_rotation_signing_payload(
        &self,
        account_id: String,
        new_owner: OwnerKey,
        expires_at_ms: u64,
    ) -> SigningPayload {
        assert_valid_record_id(&account_id);
        assert!(
            !account_id.starts_with("protocol:"),
            "protocol records cannot rotate owners"
        );
        assert!(
            read_agent_config(&account_id).is_none(),
            "agent records cannot rotate owners"
        );
        let current_owner = read_owner(&account_id).expect("owner is not registered");
        assert_ne!(current_owner, new_owner, "new owner must differ");
        assert_valid_expiry(expires_at_ms);
        let nonce = read_owner_key_rotation_nonce(&account_id)
            .checked_add(1)
            .expect("owner key rotation nonce overflow");
        SigningPayload {
            message: owner_key_rotation_message(
                &account_id,
                &current_owner,
                &new_owner,
                nonce,
                expires_at_ms,
            ),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    /// Rotate a logical record's owner only after both the existing owner and
    /// the proposed new owner prove control of their respective keys.
    pub fn rotate_owner_key(
        &mut self,
        account_id: String,
        new_owner: OwnerKey,
        nonce: u64,
        expires_at_ms: u64,
        current_owner_signature: Base64VecU8,
        new_owner_signature: Base64VecU8,
    ) {
        assert_valid_record_id(&account_id);
        assert!(
            !account_id.starts_with("protocol:"),
            "protocol records cannot rotate owners"
        );
        assert!(
            read_agent_config(&account_id).is_none(),
            "agent records cannot rotate owners"
        );
        let current_owner = read_owner(&account_id).expect("owner is not registered");
        assert_ne!(current_owner, new_owner, "new owner must differ");
        let expected_nonce = read_owner_key_rotation_nonce(&account_id)
            .checked_add(1)
            .expect("owner key rotation nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid owner key rotation nonce");
        assert_valid_expiry(expires_at_ms);
        let message = owner_key_rotation_message(
            &account_id,
            &current_owner,
            &new_owner,
            nonce,
            expires_at_ms,
        );
        assert!(
            verify_owner_signature(
                &current_owner,
                &message,
                current_owner_signature.as_ref(),
                nonce
            ),
            "invalid current owner rotation signature"
        );
        assert!(
            verify_owner_signature(&new_owner, &message, new_owner_signature.as_ref(), nonce),
            "invalid new owner rotation signature"
        );
        let storage_before = env::storage_usage();
        write_owner(&account_id, &new_owner);
        write_owner_key_rotation_nonce(&account_id, nonce);
        settle_storage_credit(&account_id, storage_before);
    }

    /// Register a logical record's owner. The proof binds the record, contract,
    /// key, and expiry. Anyone may relay it, but only the named key can create it.
    pub fn register_owner(
        &mut self,
        account_id: String,
        owner: OwnerKey,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) {
        assert_valid_record_id(&account_id);
        assert!(
            !account_id.starts_with("protocol:"),
            "protocol records cannot have an owner"
        );
        assert!(
            read_owner(&account_id).is_none(),
            "owner already registered"
        );
        assert!(
            read_agent_config(&account_id).is_none(),
            "agent records cannot have an external owner"
        );
        assert_valid_expiry(expires_at_ms);

        let message = registration_message(&account_id, &owner, expires_at_ms);
        assert!(
            verify_owner_signature(&owner, &message, signature.as_ref(), 0),
            "invalid owner registration signature"
        );
        let storage_before = env::storage_usage();
        write_owner(&account_id, &owner);
        settle_storage_credit(&account_id, storage_before);
    }

    /// Return the next withdrawal authorization message and NEP-413 envelope.
    pub fn withdrawal_signing_payload(
        &self,
        account_id: String,
        receiver_id: AccountId,
        amount: U128,
        expires_at_ms: u64,
        max_fee: U128,
    ) -> SigningPayload {
        assert_valid_record_id(&account_id);
        assert!(
            !account_id.starts_with("protocol:"),
            "protocol records cannot withdraw"
        );
        assert!(
            read_agent_config(&account_id).is_none(),
            "agent records cannot withdraw externally"
        );
        let nonce = read_withdrawal_nonce(&account_id)
            .checked_add(1)
            .expect("withdrawal nonce overflow");
        SigningPayload {
            message: withdrawal_message(
                &account_id,
                &receiver_id,
                amount,
                nonce,
                expires_at_ms,
                max_fee,
            ),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    /// Return the next authorization message for withdrawing unused storage
    /// credit as NEAR. This shares the record's nonce stream with USDC exits.
    pub fn storage_credit_withdrawal_signing_payload(
        &self,
        account_id: String,
        receiver_id: AccountId,
        amount: U128,
        expires_at_ms: u64,
    ) -> SigningPayload {
        assert_valid_record_id(&account_id);
        assert!(amount.0 > 0, "storage withdrawal must be positive");
        let nonce = read_withdrawal_nonce(&account_id)
            .checked_add(1)
            .expect("withdrawal nonce overflow");
        SigningPayload {
            message: storage_credit_withdrawal_message(
                &account_id,
                &receiver_id,
                amount,
                nonce,
                expires_at_ms,
            ),
            nonce: Base64VecU8::from(nep413_nonce(nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    /// Withdraw unspent NEAR storage credit to a wallet-selected NEAR account.
    /// The record owner signs the exact destination, amount, nonce, and expiry.
    pub fn withdraw_storage_credit(
        &mut self,
        account_id: String,
        receiver_id: AccountId,
        amount: U128,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) -> Promise {
        assert_valid_record_id(&account_id);
        assert!(amount.0 > 0, "storage withdrawal must be positive");
        assert_valid_expiry(expires_at_ms);
        let expected_nonce = read_withdrawal_nonce(&account_id)
            .checked_add(1)
            .expect("withdrawal nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid withdrawal nonce");
        let owner = read_owner(&account_id).expect("owner is not registered");
        let message = storage_credit_withdrawal_message(
            &account_id,
            &receiver_id,
            amount,
            nonce,
            expires_at_ms,
        );
        assert!(
            verify_owner_signature(&owner, &message, signature.as_ref(), nonce),
            "invalid storage credit withdrawal signature"
        );

        let storage_before = env::storage_usage();
        write_withdrawal_nonce(&account_id, nonce);
        settle_storage_credit(&account_id, storage_before);
        let available = read_storage_credit(&account_id);
        assert!(available >= amount.0, "insufficient storage credit");
        write_storage_credit(&account_id, available - amount.0);

        Promise::new(receiver_id)
            .transfer(NearToken::from_yoctonear(amount.0))
            .then(
                Promise::new(env::current_account_id()).function_call(
                    "on_storage_credit_withdraw_complete".to_string(),
                    near_sdk::serde_json::to_vec(&StorageWithdrawCallbackArgs {
                        account_id,
                        amount,
                    })
                    .expect("serialize storage withdrawal callback"),
                    NearToken::from_yoctonear(0),
                    Gas::from_tgas(10),
                ),
            )
    }

    /// Restore credit if its NEAR transfer fails; the nonce remains consumed.
    #[private]
    pub fn on_storage_credit_withdraw_complete(
        &mut self,
        account_id: String,
        amount: U128,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        if result.is_ok() {
            return true;
        }
        let current = read_storage_credit(&account_id);
        write_storage_credit(
            &account_id,
            current
                .checked_add(amount.0)
                .expect("storage withdrawal refund overflow"),
        );
        false
    }

    /// Verify the owner's signature inside the shared account contract, debit
    /// only that logical record, and transfer USDC. The relayer is permissionless.
    pub fn withdraw(
        &mut self,
        account_id: String,
        receiver_id: AccountId,
        amount: U128,
        nonce: u64,
        expires_at_ms: u64,
        max_fee: U128,
        signature: Base64VecU8,
    ) -> Promise {
        assert_valid_record_id(&account_id);
        assert!(
            !account_id.starts_with("protocol:"),
            "protocol records cannot withdraw"
        );
        assert!(
            read_agent_config(&account_id).is_none(),
            "agent records cannot withdraw externally"
        );
        assert!(amount.0 > 0, "withdrawal amount must be greater than zero");
        assert!(max_fee.0 == 0, "withdrawal fees are not supported yet");

        let expected_nonce = read_withdrawal_nonce(&account_id)
            .checked_add(1)
            .expect("withdrawal nonce overflow");
        assert!(nonce == expected_nonce, "invalid withdrawal nonce");
        assert_valid_expiry(expires_at_ms);

        let owner = read_owner(&account_id).expect("owner is not registered");
        let message = withdrawal_message(
            &account_id,
            &receiver_id,
            amount,
            nonce,
            expires_at_ms,
            max_fee,
        );
        assert!(
            verify_owner_signature(&owner, &message, signature.as_ref(), nonce),
            "invalid withdrawal signature"
        );

        let balance = self.balances.get(&account_id).unwrap_or_default();
        assert!(balance >= amount.0, "insufficient free balance");

        // Consume nonce and reserve the balance before the asynchronous FT call.
        // On transfer failure the balance is restored, but the nonce stays spent.
        let storage_before = env::storage_usage();
        write_withdrawal_nonce(&account_id, nonce);
        self.balances.insert(&account_id, &(balance - amount.0));
        settle_storage_credit(&account_id, storage_before);

        let transfer_args = near_sdk::serde_json::to_vec(&FtTransferArgs {
            receiver_id,
            amount,
            memo: None,
        })
        .expect("serialize USDC transfer arguments");
        let callback_args =
            near_sdk::serde_json::to_vec(&WithdrawCallbackArgs { account_id, amount })
                .expect("serialize withdrawal callback arguments");

        Promise::new(self.usdc_token.clone())
            .function_call(
                "ft_transfer".to_string(),
                transfer_args,
                NearToken::from_yoctonear(1),
                Gas::from_tgas(20),
            )
            .then(Promise::new(env::current_account_id()).function_call(
                "on_withdraw_complete".to_string(),
                callback_args,
                NearToken::from_yoctonear(0),
                Gas::from_tgas(10),
            ))
    }

    /// Restore ledger credit if the NEP-141 transfer fails. The nonce remains
    /// consumed, so the signed withdrawal cannot be replayed.
    #[private]
    pub fn on_withdraw_complete(
        &mut self,
        account_id: String,
        amount: U128,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        if result.is_ok() {
            return true;
        }

        let storage_before = env::storage_usage();
        let balance = self.balances.get(&account_id).unwrap_or_default();
        let restored = balance
            .checked_add(amount.0)
            .expect("withdrawal refund balance overflow");
        self.balances.insert(&account_id, &restored);
        settle_storage_credit(&account_id, storage_before);
        false
    }
}

fn release_agent_position(
    balances: &mut LookupMap<String, u128>,
    position: &AgentPosition,
    config: &AgentConfig,
) -> U128 {
    let reserved_margin = read_agent_reserved_margin(&position.agent_record_id);
    assert!(
        reserved_margin >= position.margin_usdc.0,
        "agent reserved margin invariant failed"
    );
    let open_notional = read_agent_open_notional(&position.agent_record_id);
    assert!(
        open_notional >= position.notional_usdc.0,
        "agent open notional invariant failed"
    );
    let count = read_agent_position_count(&position.agent_record_id);
    assert!(count > 0, "agent position count invariant failed");

    let recipient_record = if config.enabled {
        &position.agent_record_id
    } else {
        &config.user_record_id
    };
    let recipient_balance = balances.get(recipient_record).unwrap_or_default();
    let updated_balance = recipient_balance
        .checked_add(position.margin_usdc.0)
        .expect("released margin overflow");
    balances.insert(recipient_record, &updated_balance);
    write_agent_reserved_margin(
        &position.agent_record_id,
        reserved_margin - position.margin_usdc.0,
    );
    write_agent_open_notional(
        &position.agent_record_id,
        open_notional - position.notional_usdc.0,
    );
    write_agent_position_count(&position.agent_record_id, count - 1);
    delete_agent_position(&position.agent_record_id, &position.position_id);
    position.margin_usdc
}

fn settle_storage_credit(account_id: &str, storage_before: u64) {
    let storage_after = env::storage_usage();
    if storage_after == storage_before {
        return;
    }
    let current = read_storage_credit(account_id);
    if storage_after > storage_before {
        assert!(
            env::storage_read(&storage_credit_storage_key(account_id)).is_some(),
            "storage credit is not funded"
        );
        let bytes = storage_after - storage_before;
        let cost = env::storage_byte_cost()
            .as_yoctonear()
            .checked_mul(u128::from(bytes))
            .expect("storage cost overflow");
        assert!(current >= cost, "insufficient record storage credit");
        write_storage_credit(account_id, current - cost);
    } else if storage_after < storage_before {
        let bytes = storage_before - storage_after;
        let released = env::storage_byte_cost()
            .as_yoctonear()
            .checked_mul(u128::from(bytes))
            .expect("released storage value overflow");
        let credit_key = storage_credit_storage_key(account_id);
        if env::storage_read(&credit_key).is_some() {
            write_storage_credit(
                account_id,
                current
                    .checked_add(released)
                    .expect("storage credit refund overflow"),
            );
            return;
        }

        // Legacy records may release state before receiving any storage
        // sponsorship. Create a credit entry only when the released storage
        // also pays for that entry; otherwise leave the storage freed.
        let before_credit_entry = env::storage_usage();
        write_storage_credit(account_id, 0);
        let credit_entry_bytes = env::storage_usage().saturating_sub(before_credit_entry);
        let credit_entry_cost = env::storage_byte_cost()
            .as_yoctonear()
            .checked_mul(u128::from(credit_entry_bytes))
            .expect("storage credit entry cost overflow");
        if released >= credit_entry_cost {
            write_storage_credit(account_id, released - credit_entry_cost);
        } else {
            env::storage_remove(&credit_key);
        }
    }
}

fn read_vault_reserved_payout() -> u128 {
    read_u128_storage(VAULT_RESERVED_PREFIX)
}

fn write_vault_reserved_payout(value: u128) {
    write_u128_storage(VAULT_RESERVED_PREFIX, value);
}

fn validate_close_mark(
    position: &AgentPosition,
    reference_price: U128,
    sequence: u64,
    max_slippage_bps: u16,
) {
    assert!(
        max_slippage_bps <= 1_000,
        "slippage limit exceeds 10 percent"
    );
    assert!(reference_price.0 > 0, "reference price must be positive");
    let mark = require_fresh_market_price(&position.market_id);
    assert!(mark.sequence >= sequence, "market price sequence regressed");
    let delta = mark.price.0.abs_diff(reference_price.0);
    assert!(
        delta.checked_mul(10_000).is_some_and(|d| d
            <= reference_price
                .0
                .saturating_mul(u128::from(max_slippage_bps))),
        "market price exceeded signed slippage"
    );
}

/// Prototype settlement: isolated loss is capped at margin and profit at
/// notional. Fees, funding, liquidation and insurance/bad-debt mechanics are
/// deliberately outside this first funded-vault milestone.
fn settle_market_position(
    balances: &mut LookupMap<String, u128>,
    position: &AgentPosition,
    config: &AgentConfig,
    mark_price: U128,
) -> U128 {
    let entry = position
        .entry_price
        .expect("price-aware position has no entry price");
    let raw_pnl: i128 =
        calculate_linear_pnl_raw(position.notional_usdc, entry, mark_price, position.is_long)
            .parse()
            .expect("PnL exceeds signed range");
    let margin = i128::try_from(position.margin_usdc.0).expect("margin exceeds signed range");
    let notional = i128::try_from(position.notional_usdc.0).expect("notional exceeds signed range");
    let pnl = raw_pnl.clamp(-margin, notional);
    let payout = u128::try_from(margin + pnl).expect("settlement payout is negative");

    let vault_id = SHARED_VAULT_RECORD_ID.to_string();
    let vault_balance = balances.get(&vault_id).unwrap_or_default();
    let reserved = read_vault_reserved_payout();
    assert!(
        reserved >= position.notional_usdc.0,
        "vault reservation invariant failed"
    );
    if pnl > 0 {
        let profit = pnl as u128;
        assert!(vault_balance >= profit, "vault cannot fund realized profit");
        balances.insert(&vault_id, &(vault_balance - profit));
    } else if pnl < 0 {
        balances.insert(
            &vault_id,
            &vault_balance
                .checked_add(pnl.unsigned_abs())
                .expect("vault balance overflow"),
        );
    }

    let reserved_margin = read_agent_reserved_margin(&position.agent_record_id);
    assert!(
        reserved_margin >= position.margin_usdc.0,
        "agent reserved margin invariant failed"
    );
    let open_notional = read_agent_open_notional(&position.agent_record_id);
    assert!(
        open_notional >= position.notional_usdc.0,
        "agent open notional invariant failed"
    );
    let count = read_agent_position_count(&position.agent_record_id);
    assert!(count > 0, "agent position count invariant failed");
    let recipient = if config.enabled {
        &position.agent_record_id
    } else {
        &config.user_record_id
    };
    let recipient_balance = balances.get(recipient).unwrap_or_default();
    balances.insert(
        recipient,
        &recipient_balance
            .checked_add(payout)
            .expect("settlement payout overflow"),
    );
    write_vault_reserved_payout(reserved - position.notional_usdc.0);
    write_agent_reserved_margin(
        &position.agent_record_id,
        reserved_margin - position.margin_usdc.0,
    );
    write_agent_open_notional(
        &position.agent_record_id,
        open_notional - position.notional_usdc.0,
    );
    write_agent_position_count(&position.agent_record_id, count - 1);
    delete_agent_position(&position.agent_record_id, &position.position_id);
    U128(payout)
}

fn calculate_linear_pnl_raw(
    notional_usdc: U128,
    entry_price: U128,
    mark_price: U128,
    is_long: bool,
) -> String {
    assert!(notional_usdc.0 > 0, "position notional must be positive");
    assert!(entry_price.0 > 0, "entry price must be positive");
    assert!(mark_price.0 > 0, "mark price must be positive");

    let favorable_move = if is_long {
        mark_price.0 >= entry_price.0
    } else {
        mark_price.0 <= entry_price.0
    };
    let price_delta = mark_price.0.abs_diff(entry_price.0);
    let pnl = notional_usdc
        .0
        .checked_mul(price_delta)
        .expect("PnL calculation overflow")
        / entry_price.0;
    let magnitude = i128::try_from(pnl).expect("PnL exceeds signed range");
    if favorable_move {
        magnitude.to_string()
    } else {
        (-magnitude).to_string()
    }
}

fn assert_valid_record_id(account_id: &str) {
    assert!(!account_id.is_empty(), "account id must not be empty");
    assert!(account_id.len() <= 128, "account id is too long");
    assert!(
        account_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":_-.@".contains(&byte)),
        "account id contains unsupported characters"
    );
}

fn assert_valid_market_id(market_id: &str) {
    assert!(!market_id.is_empty(), "market id must not be empty");
    assert!(market_id.len() <= 32, "market id is too long");
    assert!(
        market_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte)),
        "market id contains unsupported characters"
    );
}

fn assert_valid_agent_config(
    user_record_id: &str,
    agent_record_id: &str,
    allowed_markets: &[String],
    max_position_usdc: U128,
    max_open_notional_usdc: U128,
    max_leverage_bps: u32,
    max_allocation_usdc: U128,
    expires_at_ms: u64,
) {
    assert_valid_record_id(user_record_id);
    assert_valid_record_id(agent_record_id);
    assert!(
        user_record_id != agent_record_id,
        "agent record must be distinct"
    );
    assert!(
        agent_record_id.starts_with(&format!("agent:{user_record_id}:")),
        "agent record must be namespaced to its user"
    );
    assert!(!allowed_markets.is_empty(), "agent needs an allowed market");
    assert!(allowed_markets.len() <= 32, "too many allowed markets");
    for (index, market) in allowed_markets.iter().enumerate() {
        assert!(
            !market.is_empty() && market.len() <= 32,
            "invalid market id"
        );
        assert!(
            market
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte)),
            "invalid market id"
        );
        assert!(
            !allowed_markets[..index].contains(market),
            "duplicate allowed market"
        );
    }
    assert!(
        max_position_usdc.0 > 0,
        "max position must be greater than zero"
    );
    assert!(
        max_open_notional_usdc.0 >= max_position_usdc.0,
        "max open notional must be at least the max position"
    );
    assert!(
        max_allocation_usdc.0 > 0,
        "max allocation must be greater than zero"
    );
    assert!(
        max_leverage_bps >= 10_000 && max_leverage_bps <= 50_000,
        "agent leverage must be between 1x and 5x"
    );
    let now_ms = env::block_timestamp() / 1_000_000;
    assert!(expires_at_ms > now_ms, "agent authorization expired");
    assert!(
        expires_at_ms <= now_ms.saturating_add(MAX_AGENT_TTL_MS),
        "agent authorization expiry is too far in the future"
    );
}

fn assert_valid_expiry(expires_at_ms: u64) {
    let now_ms = env::block_timestamp() / 1_000_000;
    assert!(expires_at_ms > now_ms, "authorization expired");
    assert!(
        expires_at_ms <= now_ms.saturating_add(MAX_AUTHORIZATION_TTL_MS),
        "authorization expiry is too far in the future"
    );
}

fn assert_valid_agent_action_expiry(expires_at_ms: u64) {
    let now_ms = env::block_timestamp() / 1_000_000;
    assert!(expires_at_ms > now_ms, "agent action expired");
    assert!(
        expires_at_ms <= now_ms.saturating_add(MAX_AGENT_ACTION_TTL_MS),
        "agent action expiry is too far in the future"
    );
}

fn owner_storage_key(account_id: &str) -> Vec<u8> {
    let mut key = OWNER_PREFIX.to_vec();
    key.extend_from_slice(account_id.as_bytes());
    key
}

fn owner_key_rotation_nonce_storage_key(account_id: &str) -> Vec<u8> {
    let mut key = OWNER_KEY_ROTATION_NONCE_PREFIX.to_vec();
    key.extend_from_slice(account_id.as_bytes());
    key
}

fn nonce_storage_key(account_id: &str) -> Vec<u8> {
    let mut key = WITHDRAWAL_NONCE_PREFIX.to_vec();
    key.extend_from_slice(account_id.as_bytes());
    key
}

fn agent_config_storage_key(account_id: &str) -> Vec<u8> {
    let mut key = AGENT_CONFIG_PREFIX.to_vec();
    key.extend_from_slice(account_id.as_bytes());
    key
}

fn user_agent_storage_key(account_id: &str) -> Vec<u8> {
    let mut key = USER_AGENT_PREFIX.to_vec();
    key.extend_from_slice(account_id.as_bytes());
    key
}

fn agent_admin_nonce_storage_key(account_id: &str) -> Vec<u8> {
    let mut key = AGENT_ADMIN_NONCE_PREFIX.to_vec();
    key.extend_from_slice(account_id.as_bytes());
    key
}

fn agent_action_nonce_storage_key(account_id: &str) -> Vec<u8> {
    let mut key = AGENT_ACTION_NONCE_PREFIX.to_vec();
    key.extend_from_slice(account_id.as_bytes());
    key
}

fn agent_open_notional_storage_key(account_id: &str) -> Vec<u8> {
    let mut key = AGENT_OPEN_NOTIONAL_PREFIX.to_vec();
    key.extend_from_slice(account_id.as_bytes());
    key
}

fn agent_position_count_storage_key(account_id: &str) -> Vec<u8> {
    let mut key = AGENT_POSITION_COUNT_PREFIX.to_vec();
    key.extend_from_slice(account_id.as_bytes());
    key
}

fn agent_reserved_margin_storage_key(account_id: &str) -> Vec<u8> {
    let mut key = AGENT_RESERVED_MARGIN_PREFIX.to_vec();
    key.extend_from_slice(account_id.as_bytes());
    key
}

fn market_price_storage_key(market_id: &str) -> Vec<u8> {
    let mut key = MARKET_PRICE_PREFIX.to_vec();
    key.extend_from_slice(market_id.as_bytes());
    key
}

fn storage_credit_storage_key(account_id: &str) -> Vec<u8> {
    let mut key = STORAGE_CREDIT_PREFIX.to_vec();
    key.extend_from_slice(account_id.as_bytes());
    key
}

fn read_storage_credit(account_id: &str) -> u128 {
    read_u128_storage(&storage_credit_storage_key(account_id))
}

fn write_storage_credit(account_id: &str, amount: u128) {
    write_u128_storage(&storage_credit_storage_key(account_id), amount);
}

fn agent_position_storage_key(agent_record_id: &str, position_id: &str) -> Vec<u8> {
    let mut key = AGENT_POSITION_PREFIX.to_vec();
    key.extend_from_slice(&(agent_record_id.len() as u32).to_le_bytes());
    key.extend_from_slice(agent_record_id.as_bytes());
    key.extend_from_slice(position_id.as_bytes());
    key
}

fn read_owner(account_id: &str) -> Option<OwnerKey> {
    env::storage_read(&owner_storage_key(account_id))
        .map(|bytes| near_sdk::serde_json::from_slice(&bytes).expect("invalid stored owner key"))
}

fn write_owner(account_id: &str, owner: &OwnerKey) {
    let value = near_sdk::serde_json::to_vec(owner).expect("serialize owner key");
    env::storage_write(&owner_storage_key(account_id), &value);
}

fn read_withdrawal_nonce(account_id: &str) -> u64 {
    env::storage_read(&nonce_storage_key(account_id))
        .map(|bytes| {
            u64::from_le_bytes(
                bytes
                    .as_slice()
                    .try_into()
                    .expect("invalid stored withdrawal nonce"),
            )
        })
        .unwrap_or_default()
}

fn write_withdrawal_nonce(account_id: &str, nonce: u64) {
    env::storage_write(&nonce_storage_key(account_id), &nonce.to_le_bytes());
}

fn read_owner_key_rotation_nonce(account_id: &str) -> u64 {
    read_u64_storage(&owner_key_rotation_nonce_storage_key(account_id))
}

fn write_owner_key_rotation_nonce(account_id: &str, nonce: u64) {
    write_u64_storage(&owner_key_rotation_nonce_storage_key(account_id), nonce);
}

fn read_agent_config(account_id: &str) -> Option<AgentConfig> {
    env::storage_read(&agent_config_storage_key(account_id))
        .map(|bytes| near_sdk::serde_json::from_slice(&bytes).expect("invalid stored agent config"))
}

fn write_agent_config(account_id: &str, config: &AgentConfig) {
    let value = near_sdk::serde_json::to_vec(config).expect("serialize agent config");
    env::storage_write(&agent_config_storage_key(account_id), &value);
}

fn read_agent_record(user_record_id: &str) -> Option<String> {
    env::storage_read(&user_agent_storage_key(user_record_id))
        .map(|bytes| String::from_utf8(bytes).expect("invalid stored agent record id"))
}

fn write_agent_record(user_record_id: &str, agent_record_id: &str) {
    env::storage_write(
        &user_agent_storage_key(user_record_id),
        agent_record_id.as_bytes(),
    );
}

fn read_agent_admin_nonce(user_record_id: &str) -> u64 {
    env::storage_read(&agent_admin_nonce_storage_key(user_record_id))
        .map(|bytes| {
            u64::from_le_bytes(
                bytes
                    .as_slice()
                    .try_into()
                    .expect("invalid stored agent admin nonce"),
            )
        })
        .unwrap_or_default()
}

fn write_agent_admin_nonce(user_record_id: &str, nonce: u64) {
    env::storage_write(
        &agent_admin_nonce_storage_key(user_record_id),
        &nonce.to_le_bytes(),
    );
}

fn read_u64_storage(key: &[u8]) -> u64 {
    env::storage_read(key)
        .map(|bytes| u64::from_le_bytes(bytes.as_slice().try_into().expect("invalid stored u64")))
        .unwrap_or_default()
}

fn write_u64_storage(key: &[u8], value: u64) {
    env::storage_write(key, &value.to_le_bytes());
}

fn read_u128_storage(key: &[u8]) -> u128 {
    env::storage_read(key)
        .map(|bytes| u128::from_le_bytes(bytes.as_slice().try_into().expect("invalid stored u128")))
        .unwrap_or_default()
}

fn write_u128_storage(key: &[u8], value: u128) {
    env::storage_write(key, &value.to_le_bytes());
}

fn read_agent_action_nonce(agent_record_id: &str) -> u64 {
    read_u64_storage(&agent_action_nonce_storage_key(agent_record_id))
}

fn write_agent_action_nonce(agent_record_id: &str, nonce: u64) {
    write_u64_storage(&agent_action_nonce_storage_key(agent_record_id), nonce);
}

fn read_agent_open_notional(agent_record_id: &str) -> u128 {
    read_u128_storage(&agent_open_notional_storage_key(agent_record_id))
}

fn write_agent_open_notional(agent_record_id: &str, notional: u128) {
    write_u128_storage(&agent_open_notional_storage_key(agent_record_id), notional);
}

fn read_agent_reserved_margin(agent_record_id: &str) -> u128 {
    read_u128_storage(&agent_reserved_margin_storage_key(agent_record_id))
}

fn write_agent_reserved_margin(agent_record_id: &str, margin: u128) {
    write_u128_storage(&agent_reserved_margin_storage_key(agent_record_id), margin);
}

fn read_agent_position_count(agent_record_id: &str) -> u64 {
    read_u64_storage(&agent_position_count_storage_key(agent_record_id))
}

fn write_agent_position_count(agent_record_id: &str, count: u64) {
    write_u64_storage(&agent_position_count_storage_key(agent_record_id), count);
}

fn read_agent_position(agent_record_id: &str, position_id: &str) -> Option<AgentPosition> {
    env::storage_read(&agent_position_storage_key(agent_record_id, position_id)).map(|bytes| {
        near_sdk::serde_json::from_slice(&bytes).expect("invalid stored agent position")
    })
}

fn read_market_price(market_id: &str) -> Option<MarketPrice> {
    env::storage_read(&market_price_storage_key(market_id))
        .map(|bytes| near_sdk::serde_json::from_slice(&bytes).expect("invalid stored market price"))
}

fn write_market_price(price: &MarketPrice) {
    let value = near_sdk::serde_json::to_vec(price).expect("serialize market price");
    env::storage_write(&market_price_storage_key(&price.market_id), &value);
}

fn require_fresh_market_price(market_id: &str) -> MarketPrice {
    let price = read_market_price(market_id).expect("market price is unavailable");
    let now_ms = env::block_timestamp() / 1_000_000;
    assert!(
        price.observed_at_ms <= now_ms
            && now_ms.saturating_sub(price.observed_at_ms) <= MAX_MARKET_PRICE_AGE_MS,
        "market price is stale"
    );
    price
}

fn write_agent_position(agent_record_id: &str, position_id: &str, position: &AgentPosition) {
    let value = near_sdk::serde_json::to_vec(position).expect("serialize agent position");
    env::storage_write(
        &agent_position_storage_key(agent_record_id, position_id),
        &value,
    );
}

fn delete_agent_position(agent_record_id: &str, position_id: &str) {
    env::storage_remove(&agent_position_storage_key(agent_record_id, position_id));
}

fn owner_key_id(owner: &OwnerKey) -> String {
    match owner {
        OwnerKey::NearEd25519 { public_key } => format!("near-ed25519:{}", hex(public_key)),
        OwnerKey::SolanaEd25519 { public_key } => format!("solana-ed25519:{}", hex(public_key)),
        OwnerKey::Ethereum { address } => format!("ethereum:{}", hex(address)),
    }
}

fn registration_message(account_id: &str, owner: &OwnerKey, expires_at_ms: u64) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:register_owner\naccount_id:{}\nowner:{}\nexpires_at_ms:{}",
        env::current_account_id(),
        account_id,
        owner_key_id(owner),
        expires_at_ms,
    )
}

fn owner_key_rotation_message(
    account_id: &str,
    current_owner: &OwnerKey,
    new_owner: &OwnerKey,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:rotate_owner_key\naccount_id:{}\ncurrent_owner:{}\nnew_owner:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(),
        account_id,
        owner_key_id(current_owner),
        owner_key_id(new_owner),
        nonce,
        expires_at_ms,
    )
}

fn withdrawal_message(
    account_id: &str,
    receiver_id: &AccountId,
    amount: U128,
    nonce: u64,
    expires_at_ms: u64,
    max_fee: U128,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:withdraw\naccount_id:{}\nreceiver:{}\namount:{}\nnonce:{}\nexpires_at_ms:{}\nmax_fee_usdc:{}",
        env::current_account_id(),
        account_id,
        receiver_id,
        amount.0,
        nonce,
        expires_at_ms,
        max_fee.0,
    )
}

fn storage_credit_withdrawal_message(
    account_id: &str,
    receiver_id: &AccountId,
    amount: U128,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:withdraw_storage_credit\naccount_id:{}\nreceiver:{}\namount_yocto_near:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(),
        account_id,
        receiver_id,
        amount.0,
        nonce,
        expires_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
fn agent_setup_message(
    user_record_id: &str,
    agent_record_id: &str,
    agent_key: &OwnerKey,
    allowed_markets: &[String],
    max_position_usdc: U128,
    max_open_notional_usdc: U128,
    max_leverage_bps: u32,
    max_allocation_usdc: U128,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    let markets = near_sdk::serde_json::to_string(allowed_markets).expect("serialize markets");
    format!(
        "afterbell:v1\ncontract:{}\naction:create_agent_record\nuser_record:{}\nagent_record:{}\nagent_key:{}\nallowed_markets:{}\nmax_position_usdc:{}\nmax_open_notional_usdc:{}\nmax_leverage_bps:{}\nmax_allocation_usdc:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(),
        user_record_id,
        agent_record_id,
        owner_key_id(agent_key),
        markets,
        max_position_usdc.0,
        max_open_notional_usdc.0,
        max_leverage_bps,
        max_allocation_usdc.0,
        nonce,
        expires_at_ms,
    )
}

fn agent_funding_message(
    user_record_id: &str,
    agent_record_id: &str,
    amount: U128,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:fund_agent_record\nuser_record:{}\nagent_record:{}\namount:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(),
        user_record_id,
        agent_record_id,
        amount.0,
        nonce,
        expires_at_ms,
    )
}

fn market_price_message(
    market_id: &str,
    price: U128,
    observed_at_ms: u64,
    sequence: u64,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:publish_market_price\nmarket_id:{}\nprice:{}\nobserved_at_ms:{}\nsequence:{}",
        env::current_account_id(), market_id, price.0, observed_at_ms, sequence,
    )
}

#[allow(clippy::too_many_arguments)]
fn market_close_message(
    signer_kind: &str,
    user_record_id: &str,
    position: &AgentPosition,
    reference_price: U128,
    sequence: u64,
    max_slippage_bps: u16,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!("afterbell:v1\ncontract:{}\naction:close_agent_market_position\nsigner_kind:{}\nuser_record:{}\nagent_record:{}\nposition_id:{}\nmarket_id:{}\nside:{}\nnotional_usdc:{}\nmargin_usdc:{}\nentry_price:{}\nreference_price:{}\nprice_sequence:{}\nmax_slippage_bps:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(), signer_kind, user_record_id, position.agent_record_id,
        position.position_id, position.market_id, if position.is_long {"long"} else {"short"},
        position.notional_usdc.0, position.margin_usdc.0, position.entry_price.unwrap().0,
        reference_price.0, sequence, max_slippage_bps, nonce, expires_at_ms)
}

#[allow(clippy::too_many_arguments)]
fn agent_market_open_message(
    agent_record_id: &str,
    position_id: &str,
    market_id: &str,
    is_long: bool,
    notional_usdc: U128,
    leverage_bps: u32,
    reference_price: U128,
    price_sequence: u64,
    max_slippage_bps: u16,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:open_agent_market_position\nagent_record:{}\nposition_id:{}\nmarket_id:{}\nside:{}\nnotional_usdc:{}\nleverage_bps:{}\nreference_price:{}\nprice_sequence:{}\nmax_slippage_bps:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(), agent_record_id, position_id, market_id,
        if is_long { "long" } else { "short" }, notional_usdc.0, leverage_bps,
        reference_price.0, price_sequence, max_slippage_bps, nonce, expires_at_ms,
    )
}

fn agent_revocation_message(
    user_record_id: &str,
    agent_record_id: &str,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:revoke_agent\nuser_record:{}\nagent_record:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(),
        user_record_id,
        agent_record_id,
        nonce,
        expires_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
fn agent_open_message(
    agent_record_id: &str,
    position_id: &str,
    market_id: &str,
    is_long: bool,
    notional_usdc: U128,
    leverage_bps: u32,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:agent_open_position\nagent_record:{}\nposition_id:{}\nmarket_id:{}\nside:{}\nnotional_usdc:{}\nleverage_bps:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(),
        agent_record_id,
        position_id,
        market_id,
        if is_long { "long" } else { "short" },
        notional_usdc.0,
        leverage_bps,
        nonce,
        expires_at_ms,
    )
}

fn agent_close_message(position: &AgentPosition, nonce: u64, expires_at_ms: u64) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:agent_close_position\nagent_record:{}\nposition_id:{}\nmarket_id:{}\nside:{}\nnotional_usdc:{}\nmargin_usdc:{}\nleverage_bps:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(),
        position.agent_record_id,
        position.position_id,
        position.market_id,
        if position.is_long { "long" } else { "short" },
        position.notional_usdc.0,
        position.margin_usdc.0,
        position.leverage_bps,
        nonce,
        expires_at_ms,
    )
}

fn owner_close_agent_message(
    user_record_id: &str,
    position: &AgentPosition,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:owner_close_agent_position\nuser_record:{}\nagent_record:{}\nposition_id:{}\nmarket_id:{}\nside:{}\nnotional_usdc:{}\nmargin_usdc:{}\nleverage_bps:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(),
        user_record_id,
        position.agent_record_id,
        position.position_id,
        position.market_id,
        if position.is_long { "long" } else { "short" },
        position.notional_usdc.0,
        position.margin_usdc.0,
        position.leverage_bps,
        nonce,
        expires_at_ms,
    )
}

fn verify_owner_signature(owner: &OwnerKey, message: &str, signature: &[u8], nonce: u64) -> bool {
    match owner {
        OwnerKey::NearEd25519 { public_key } => {
            let Ok(signature) = <&[u8; 64]>::try_from(signature) else {
                return false;
            };
            let nonce = nep413_nonce(nonce);
            let digest = nep413_message_hash(message, &env::current_account_id(), &nonce);
            env::ed25519_verify(signature, digest, public_key)
        }
        OwnerKey::SolanaEd25519 { public_key } => {
            let Ok(signature) = <&[u8; 64]>::try_from(signature) else {
                return false;
            };
            env::ed25519_verify(signature, message.as_bytes(), public_key)
        }
        OwnerKey::Ethereum { address } => verify_eip191_signature(message, signature, address),
    }
}

fn nep413_nonce(nonce: u64) -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&nonce.to_be_bytes());
    bytes
}

fn append_borsh_string(output: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    output.extend_from_slice(
        &u32::try_from(bytes.len())
            .expect("NEP-413 string is too large")
            .to_le_bytes(),
    );
    output.extend_from_slice(bytes);
}

/// NEP-413 hashes `u32(2^31 + 413) || Borsh(payload)` with SHA-256.
fn nep413_message_hash(message: &str, recipient: &AccountId, nonce: &[u8; 32]) -> [u8; 32] {
    let mut payload = NEP_413_MESSAGE_PREFIX.to_le_bytes().to_vec();
    append_borsh_string(&mut payload, message);
    payload.extend_from_slice(nonce);
    append_borsh_string(&mut payload, recipient.as_str());
    payload.push(0); // Borsh Option::None callback URL.
    env::sha256_array(&payload)
}

fn verify_eip191_signature(message: &str, signature: &[u8], owner_address: &[u8; 20]) -> bool {
    let Ok(signature) = <&[u8; 65]>::try_from(signature) else {
        return false;
    };
    let recovery_id = match signature[64] {
        0 | 1 => signature[64],
        27 | 28 => signature[64] - 27,
        _ => return false,
    };

    let mut personal_message =
        format!("\x19Ethereum Signed Message:\n{}", message.len()).into_bytes();
    personal_message.extend_from_slice(message.as_bytes());
    let digest = env::keccak256_array(&personal_message);
    let Some(public_key) = env::ecrecover(&digest, &signature[..64], recovery_id, true) else {
        return false;
    };
    let address_hash = env::keccak256_array(public_key);
    &address_hash[12..] == owner_address
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(DIGITS[(byte >> 4) as usize] as char);
        result.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    result
}

#[near]
impl FungibleTokenReceiver for SharedLedger {
    /// NEP-141 callback for a 1Click `customRecipientMsg` deposit.
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        if env::predecessor_account_id() != self.usdc_token {
            return PromiseOrValue::Value(amount);
        }

        let Ok(deposit) = near_sdk::serde_json::from_str::<DepositMessage>(&msg) else {
            return PromiseOrValue::Value(amount);
        };
        if deposit.account_id.is_empty()
            || deposit.account_id.len() > 128
            || deposit.deposit_id.is_empty()
            || deposit.deposit_id.len() > 128
            || amount.0 == 0
            || (deposit.account_id != SHARED_VAULT_RECORD_ID
                && read_owner(&deposit.account_id).is_none())
        {
            return PromiseOrValue::Value(amount);
        }

        let current_balance = self.balances.get(&deposit.account_id);
        let current = current_balance.unwrap_or_default();
        let Some(updated) = current.checked_add(amount.0) else {
            return PromiseOrValue::Value(amount);
        };

        let storage_before = env::storage_usage();
        self.balances.insert(&deposit.account_id, &updated);
        let storage_after = env::storage_usage();
        let bytes = storage_after.saturating_sub(storage_before);
        let cost = env::storage_byte_cost()
            .as_yoctonear()
            .checked_mul(u128::from(bytes))
            .expect("deposit storage cost overflow");
        let credit = read_storage_credit(&deposit.account_id);
        if credit < cost {
            if let Some(previous) = current_balance {
                self.balances.insert(&deposit.account_id, &previous);
            } else {
                self.balances.remove(&deposit.account_id);
            }
            return PromiseOrValue::Value(amount);
        }
        if cost > 0 {
            write_storage_credit(&deposit.account_id, credit - cost);
        }
        let event = near_sdk::serde_json::json!({
            "standard": "afterbell",
            "version": "1.0.0",
            "event": "deposit_credited",
            "data": [{
                "account_id": deposit.account_id,
                "deposit_id": deposit.deposit_id,
                "sender_id": sender_id,
                "amount": amount.0.to_string()
            }]
        });
        env::log_str(&format!("EVENT_JSON:{event}"));
        PromiseOrValue::Value(U128(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;
    use secp256k1::{Message, Secp256k1, SecretKey};

    const TOKEN: &str = "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1";
    const SENDER: &str = "sender.near";
    const USER: &str = "near:test-user";
    const OTHER_USER: &str = "near:other-test-user";
    const NOW_MS: u64 = 1_000_000;
    const EXPIRY_MS: u64 = NOW_MS + 120_000;
    const AMOUNT: U128 = U128(1_000_000);
    const TEST_PRICE_100_SIGNATURE: [u8; 64] = [
        0x73, 0x55, 0x5c, 0xfc, 0x38, 0x5d, 0xa2, 0x55, 0xcf, 0x30, 0x0b, 0x77, 0xc3, 0x3f, 0x92,
        0xba, 0x04, 0xc5, 0x67, 0xd1, 0x84, 0xcf, 0xba, 0x44, 0xcf, 0xe8, 0x26, 0x99, 0x7e, 0xb9,
        0xea, 0x38, 0x3f, 0x74, 0xa3, 0x55, 0x4e, 0x9f, 0xdb, 0x28, 0x3f, 0xa3, 0x0e, 0x44, 0xf9,
        0x3a, 0xdd, 0x1a, 0x7d, 0xed, 0x78, 0x3a, 0xd8, 0xeb, 0x65, 0x4e, 0x97, 0x76, 0xab, 0x2d,
        0x23, 0x59, 0xb0, 0x05,
    ];
    const TEST_PRICE_110_SIGNATURE: [u8; 64] = [
        0x1d, 0xfa, 0xf0, 0xfa, 0x62, 0x55, 0xcf, 0x7e, 0x6a, 0x91, 0x52, 0xfb, 0xcf, 0x51, 0x81,
        0x58, 0x2a, 0x5c, 0x31, 0x65, 0xfc, 0x94, 0x27, 0xf5, 0x92, 0x6f, 0xce, 0x03, 0xcb, 0x07,
        0x54, 0x10, 0x9a, 0xa8, 0x64, 0xbb, 0xf9, 0xba, 0x59, 0x60, 0xd9, 0x0c, 0x57, 0x02, 0xb8,
        0xfb, 0x84, 0xcf, 0xde, 0x69, 0x11, 0xf8, 0x46, 0xf1, 0xbb, 0x14, 0x02, 0xc4, 0xe9, 0xe7,
        0x3a, 0x9c, 0xe8, 0x0b,
    ];

    fn setup(predecessor: &str) -> SharedLedger {
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(predecessor.parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        SharedLedger::new(TOKEN.parse().unwrap())
    }

    fn deposit_msg(id: &str, account_id: &str) -> String {
        format!(r#"{{"account_id":"{account_id}","deposit_id":"{id}"}}"#)
    }

    fn assert_unused(result: PromiseOrValue<U128>, expected: u128) {
        match result {
            PromiseOrValue::Value(value) => assert_eq!(value, U128(expected)),
            PromiseOrValue::Promise(_) => panic!("expected immediate callback result"),
        }
    }

    fn fund_test_storage_credit(account_id: &str) {
        if env::storage_read(&storage_credit_storage_key(account_id)).is_none() {
            write_storage_credit(account_id, u128::MAX);
        }
    }

    fn fund_record(contract: &mut SharedLedger, account_id: &str, amount: u128, deposit_id: &str) {
        fund_test_storage_credit(account_id);
        if account_id != SHARED_VAULT_RECORD_ID
            && contract.owner_of(account_id.to_string()).is_none()
        {
            let seed = env::sha256(account_id.as_bytes());
            let key = SigningKey::from_bytes(seed.as_slice().try_into().unwrap());
            register_solana_owner_for(contract, account_id, &key);
        }
        let result = contract.ft_on_transfer(
            SENDER.parse().unwrap(),
            U128(amount),
            deposit_msg(deposit_id, account_id),
        );
        assert_unused(result, 0);
    }

    fn register_solana_owner(contract: &mut SharedLedger, key: &SigningKey) {
        let owner = OwnerKey::SolanaEd25519 {
            public_key: key.verifying_key().to_bytes(),
        };
        let message = registration_message(USER, &owner, EXPIRY_MS);
        let signature = key.sign(message.as_bytes());
        fund_test_storage_credit(USER);
        contract.register_owner(
            USER.to_string(),
            owner,
            EXPIRY_MS,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    fn register_solana_owner_for(contract: &mut SharedLedger, account_id: &str, key: &SigningKey) {
        let owner = OwnerKey::SolanaEd25519 {
            public_key: key.verifying_key().to_bytes(),
        };
        let message = registration_message(account_id, &owner, EXPIRY_MS);
        let signature = key.sign(message.as_bytes());
        fund_test_storage_credit(account_id);
        contract.register_owner(
            account_id.to_string(),
            owner,
            EXPIRY_MS,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    fn signed_withdrawal(
        contract: &mut SharedLedger,
        key: &SigningKey,
        receiver: &str,
        nonce: u64,
        expires_at_ms: u64,
        max_fee: u128,
    ) -> Promise {
        let receiver: AccountId = receiver.parse().unwrap();
        let message =
            withdrawal_message(USER, &receiver, AMOUNT, nonce, expires_at_ms, U128(max_fee));
        let signature = key.sign(message.as_bytes());
        contract.withdraw(
            USER.to_string(),
            receiver,
            AMOUNT,
            nonce,
            expires_at_ms,
            U128(max_fee),
            Base64VecU8::from(signature.to_bytes().to_vec()),
        )
    }

    #[test]
    fn accepts_usdc_and_credits_the_requested_logical_record() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        register_solana_owner(&mut contract, &key);
        let result = contract.ft_on_transfer(
            SENDER.parse().unwrap(),
            U128(2_500_000),
            deposit_msg("quote-1", USER),
        );

        assert_unused(result, 0);
        assert_eq!(contract.balance_of(USER.to_string()), U128(2_500_000));
    }

    #[test]
    fn rejects_unapproved_tokens_by_returning_the_full_amount() {
        let mut contract = setup("attacker.near");
        let result = contract.ft_on_transfer(
            SENDER.parse().unwrap(),
            U128(500),
            deposit_msg("quote-1", USER),
        );

        assert_unused(result, 500);
        assert_eq!(contract.balance_of(USER.to_string()), U128(0));
    }

    #[test]
    fn malformed_messages_are_refunded_and_each_real_transfer_is_credited() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        register_solana_owner(&mut contract, &key);
        assert_unused(
            contract.ft_on_transfer(
                SENDER.parse().unwrap(),
                U128(500),
                deposit_msg("quote-1", USER),
            ),
            0,
        );
        assert_unused(
            contract.ft_on_transfer(
                SENDER.parse().unwrap(),
                U128(500),
                deposit_msg("quote-1", USER),
            ),
            0,
        );
        assert_unused(
            contract.ft_on_transfer(SENDER.parse().unwrap(), U128(200), "not-json".to_string()),
            200,
        );
        assert_eq!(contract.balance_of(USER.to_string()), U128(1_000));
    }

    #[test]
    fn rejects_zero_value_deposits_without_creating_a_balance() {
        let mut contract = setup(TOKEN);
        assert_unused(
            contract.ft_on_transfer(
                SENDER.parse().unwrap(),
                U128(0),
                deposit_msg("quote-1", USER),
            ),
            0,
        );
        assert_eq!(contract.balance_of(USER.to_string()), U128(0));
    }

    #[test]
    fn solana_wallet_registers_and_authorizes_shared_record_withdrawal() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        let owner = OwnerKey::SolanaEd25519 {
            public_key: key.verifying_key().to_bytes(),
        };
        register_solana_owner(&mut contract, &key);
        fund_record(&mut contract, USER, 2_500_000, "deposit-solana");

        let _promise = signed_withdrawal(&mut contract, &key, "receiver.near", 1, EXPIRY_MS, 0);

        assert_eq!(contract.balance_of(USER.to_string()), U128(1_500_000));
        assert_eq!(contract.withdrawal_nonce(USER.to_string()), 1);
        assert_eq!(contract.owner_of(USER.to_string()), Some(owner));
    }

    #[test]
    #[should_panic(expected = "invalid owner registration signature")]
    fn rejects_owner_registration_signed_by_a_different_key() {
        let mut contract = setup(TOKEN);
        let owner_key = SigningKey::from_bytes(&[7; 32]);
        let attacker_key = SigningKey::from_bytes(&[8; 32]);
        let owner = OwnerKey::SolanaEd25519 {
            public_key: owner_key.verifying_key().to_bytes(),
        };
        let message = registration_message(USER, &owner, EXPIRY_MS);
        let signature = attacker_key.sign(message.as_bytes());
        contract.register_owner(
            USER.to_string(),
            owner,
            EXPIRY_MS,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "owner already registered")]
    fn prevents_overwriting_a_registered_owner_key() {
        let mut contract = setup(TOKEN);
        let original_key = SigningKey::from_bytes(&[7; 32]);
        let replacement_key = SigningKey::from_bytes(&[8; 32]);
        register_solana_owner(&mut contract, &original_key);

        let replacement_owner = OwnerKey::SolanaEd25519 {
            public_key: replacement_key.verifying_key().to_bytes(),
        };
        let message = registration_message(USER, &replacement_owner, EXPIRY_MS);
        let signature = replacement_key.sign(message.as_bytes());
        contract.register_owner(
            USER.to_string(),
            replacement_owner,
            EXPIRY_MS,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    fn owner_key_rotation_requires_both_keys_and_supports_cross_chain_rotation() {
        let mut contract = setup(TOKEN);
        let current_key = SigningKey::from_bytes(&[81; 32]);
        register_solana_owner(&mut contract, &current_key);
        fund_record(&mut contract, USER, 2_500_000, "deposit-key-rotation");

        let new_secret = SecretKey::from_slice(&[82; 32]).unwrap();
        let new_public = secp256k1::PublicKey::from_secret_key(&Secp256k1::new(), &new_secret);
        let serialized = new_public.serialize_uncompressed();
        let hash = env::keccak256_array(&serialized[1..]);
        let mut address = [0; 20];
        address.copy_from_slice(&hash[12..]);
        let new_owner = OwnerKey::Ethereum { address };
        let nonce = 1;
        let expiry = EXPIRY_MS;
        let message = owner_key_rotation_message(
            USER,
            &contract.owner_of(USER.to_string()).unwrap(),
            &new_owner,
            nonce,
            expiry,
        );
        let current_signature = current_key.sign(message.as_bytes());
        let new_signature = Secp256k1::new().sign_ecdsa_recoverable(
            &Message::from_slice(&eip191_digest(&message)).unwrap(),
            &new_secret,
        );
        let (recovery_id, compact) = new_signature.serialize_compact();
        let mut new_signature_bytes = compact.to_vec();
        new_signature_bytes.push(recovery_id.to_i32() as u8);

        contract.rotate_owner_key(
            USER.to_string(),
            new_owner.clone(),
            nonce,
            expiry,
            Base64VecU8::from(current_signature.to_bytes().to_vec()),
            Base64VecU8::from(new_signature_bytes),
        );

        assert_eq!(contract.owner_of(USER.to_string()), Some(new_owner));
        assert_eq!(contract.owner_key_rotation_nonce(USER.to_string()), 1);

        let receiver: AccountId = "receiver.near".parse().unwrap();
        let withdrawal = withdrawal_message(USER, &receiver, AMOUNT, 1, expiry, U128(0));
        let new_withdrawal = Secp256k1::new().sign_ecdsa_recoverable(
            &Message::from_slice(&eip191_digest(&withdrawal)).unwrap(),
            &new_secret,
        );
        let (recovery_id, compact) = new_withdrawal.serialize_compact();
        let mut withdrawal_bytes = compact.to_vec();
        withdrawal_bytes.push(recovery_id.to_i32() as u8);
        let _promise = contract.withdraw(
            USER.to_string(),
            receiver,
            AMOUNT,
            1,
            expiry,
            U128(0),
            Base64VecU8::from(withdrawal_bytes),
        );
        assert_eq!(contract.balance_of(USER.to_string()), U128(1_500_000));
    }

    #[test]
    #[should_panic(expected = "invalid new owner rotation signature")]
    fn owner_key_rotation_rejects_missing_proof_of_new_key() {
        let mut contract = setup(TOKEN);
        let current_key = SigningKey::from_bytes(&[83; 32]);
        let wrong_new_key = SigningKey::from_bytes(&[84; 32]);
        register_solana_owner(&mut contract, &current_key);
        let new_owner = OwnerKey::SolanaEd25519 {
            public_key: SigningKey::from_bytes(&[85; 32]).verifying_key().to_bytes(),
        };
        let nonce = 1;
        let expiry = EXPIRY_MS;
        let message = owner_key_rotation_message(
            USER,
            &contract.owner_of(USER.to_string()).unwrap(),
            &new_owner,
            nonce,
            expiry,
        );
        let current_signature = current_key.sign(message.as_bytes());
        let incorrect_new_signature = wrong_new_key.sign(message.as_bytes());

        contract.rotate_owner_key(
            USER.to_string(),
            new_owner,
            nonce,
            expiry,
            Base64VecU8::from(current_signature.to_bytes().to_vec()),
            Base64VecU8::from(incorrect_new_signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "invalid owner key rotation nonce")]
    fn owner_key_rotation_nonce_prevents_replay() {
        let mut contract = setup(TOKEN);
        let first_key = SigningKey::from_bytes(&[88; 32]);
        let second_key = SigningKey::from_bytes(&[89; 32]);
        let third_key = SigningKey::from_bytes(&[90; 32]);
        register_solana_owner(&mut contract, &first_key);
        let first_owner = OwnerKey::SolanaEd25519 {
            public_key: first_key.verifying_key().to_bytes(),
        };
        let second_owner = OwnerKey::SolanaEd25519 {
            public_key: second_key.verifying_key().to_bytes(),
        };
        let third_owner = OwnerKey::SolanaEd25519 {
            public_key: third_key.verifying_key().to_bytes(),
        };
        let expiry = EXPIRY_MS;
        let first_message =
            owner_key_rotation_message(USER, &first_owner, &second_owner, 1, expiry);
        contract.rotate_owner_key(
            USER.to_string(),
            second_owner.clone(),
            1,
            expiry,
            Base64VecU8::from(first_key.sign(first_message.as_bytes()).to_bytes().to_vec()),
            Base64VecU8::from(
                second_key
                    .sign(first_message.as_bytes())
                    .to_bytes()
                    .to_vec(),
            ),
        );

        let stale_message =
            owner_key_rotation_message(USER, &second_owner, &third_owner, 1, expiry);
        contract.rotate_owner_key(
            USER.to_string(),
            third_owner,
            1,
            expiry,
            Base64VecU8::from(
                second_key
                    .sign(stale_message.as_bytes())
                    .to_bytes()
                    .to_vec(),
            ),
            Base64VecU8::from(third_key.sign(stale_message.as_bytes()).to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "invalid withdrawal signature")]
    fn rotated_out_owner_key_cannot_authorize_withdrawal() {
        let mut contract = setup(TOKEN);
        let old_key = SigningKey::from_bytes(&[86; 32]);
        let new_key = SigningKey::from_bytes(&[87; 32]);
        register_solana_owner(&mut contract, &old_key);
        fund_record(&mut contract, USER, 2_500_000, "deposit-retired-owner-key");
        let old_owner = OwnerKey::SolanaEd25519 {
            public_key: old_key.verifying_key().to_bytes(),
        };
        let new_owner = OwnerKey::SolanaEd25519 {
            public_key: new_key.verifying_key().to_bytes(),
        };
        let expiry = EXPIRY_MS;
        let rotation = owner_key_rotation_message(USER, &old_owner, &new_owner, 1, expiry);
        let old_signature = old_key.sign(rotation.as_bytes());
        let new_signature = new_key.sign(rotation.as_bytes());
        contract.rotate_owner_key(
            USER.to_string(),
            new_owner,
            1,
            expiry,
            Base64VecU8::from(old_signature.to_bytes().to_vec()),
            Base64VecU8::from(new_signature.to_bytes().to_vec()),
        );

        let _promise = signed_withdrawal(&mut contract, &old_key, "receiver.near", 1, expiry, 0);
    }

    #[test]
    fn every_accepted_deposit_adds_persistent_storage() {
        let mut contract = setup(TOKEN);
        let before_first = env::storage_usage();
        fund_record(&mut contract, "near:storage-user-1", 1, "storage-deposit-1");
        let after_first = env::storage_usage();
        assert!(after_first > before_first);
        assert!(contract
            .owner_of("near:storage-user-1".to_string())
            .is_some());

        fund_record(&mut contract, "near:storage-user-1", 1, "storage-deposit-2");
        let after_second = env::storage_usage();
        assert_eq!(after_second, after_first);

        fund_record(&mut contract, "near:storage-user-2", 1, "storage-deposit-3");
        assert!(env::storage_usage() > after_second);
    }

    #[test]
    fn rejects_deposits_to_unregistered_records_without_creating_storage() {
        let mut contract = setup(TOKEN);
        let before = env::storage_usage();
        let result = contract.ft_on_transfer(
            SENDER.parse().unwrap(),
            U128(500),
            deposit_msg("unregistered", "near:unregistered"),
        );
        assert_unused(result, 500);
        assert_eq!(
            contract.balance_of("near:unregistered".to_string()),
            U128(0)
        );
        assert_eq!(env::storage_usage(), before);
    }

    #[test]
    fn refunds_deposit_when_record_storage_credit_cannot_cover_new_balance() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        register_solana_owner(&mut contract, &key);
        write_storage_credit(USER, 0);
        let before = env::storage_usage();
        let result = contract.ft_on_transfer(
            SENDER.parse().unwrap(),
            U128(500),
            deposit_msg("underfunded-storage", USER),
        );
        assert_unused(result, 500);
        assert_eq!(contract.balance_of(USER.to_string()), U128(0));
        assert_eq!(env::storage_usage(), before);
    }

    #[test]
    fn storage_deposit_covers_its_credit_entry_and_leaves_record_credit() {
        let context = VMContextBuilder::new()
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(SENDER.parse().unwrap())
            .attached_deposit(NearToken::from_near(1))
            .build();
        testing_env!(context);
        let mut contract = SharedLedger::new(TOKEN.parse().unwrap());
        let available = contract.storage_deposit_for(USER.to_string());
        assert!(available.0 > 0);
        assert!(available.0 < NearToken::from_near(1).as_yoctonear());
        assert_eq!(contract.storage_credit_of(USER.to_string()), available);
    }

    #[test]
    fn storage_growth_is_charged_and_released_bytes_return_to_credit() {
        let _contract = setup(TOKEN);
        let starting_credit = 10_000_000_000_000_000_000_000_000u128;
        write_storage_credit(USER, starting_credit);
        let before_growth = env::storage_usage();
        assert!(!env::storage_write(b"a1:test:storage-fee", &[1; 12]));
        let growth = env::storage_usage() - before_growth;
        settle_storage_credit(USER, before_growth);
        let after_charge = read_storage_credit(USER);
        assert_eq!(
            starting_credit - after_charge,
            env::storage_byte_cost().as_yoctonear() * u128::from(growth)
        );

        let before_release = env::storage_usage();
        assert!(env::storage_remove(b"a1:test:storage-fee"));
        settle_storage_credit(USER, before_release);
        assert_eq!(read_storage_credit(USER), starting_credit);
    }

    #[test]
    fn legacy_record_can_release_storage_without_prior_credit() {
        let _contract = setup(TOKEN);
        let fixture_key = b"a1:test:legacy-storage-release";
        assert!(!env::storage_write(fixture_key, &[7; 256]));
        let before_release = env::storage_usage();
        assert!(env::storage_remove(fixture_key));

        settle_storage_credit(USER, before_release);

        let credit_key = storage_credit_storage_key(USER);
        let after_release = env::storage_usage();
        let credit = read_storage_credit(USER);
        assert!(credit > 0);
        assert!(after_release < before_release);
        assert_eq!(
            credit,
            env::storage_byte_cost().as_yoctonear() * u128::from(before_release - after_release)
        );
        assert!(env::storage_read(&credit_key).is_some());
    }

    #[test]
    fn tiny_legacy_storage_release_does_not_create_unfunded_credit_entry() {
        let _contract = setup(TOKEN);
        let fixture_key = b"x";
        assert!(!env::storage_write(fixture_key, &[1]));
        let before_release = env::storage_usage();
        assert!(env::storage_remove(fixture_key));

        settle_storage_credit(USER, before_release);

        assert_eq!(read_storage_credit(USER), 0);
        assert!(env::storage_read(&storage_credit_storage_key(USER)).is_none());
    }

    #[test]
    fn owner_can_withdraw_only_signed_unused_near_storage_credit() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        register_solana_owner(&mut contract, &key);
        let available_before = contract.storage_credit_of(USER.to_string()).0;
        let receiver: AccountId = "receiver.near".parse().unwrap();
        let amount = U128(1_000);
        let message = storage_credit_withdrawal_message(USER, &receiver, amount, 1, EXPIRY_MS);
        let signature = key.sign(message.as_bytes());
        let _promise = contract.withdraw_storage_credit(
            USER.to_string(),
            receiver,
            amount,
            1,
            EXPIRY_MS,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
        let available_after = contract.storage_credit_of(USER.to_string()).0;
        assert!(available_after < available_before - amount.0);
        assert!(!contract.on_storage_credit_withdraw_complete(
            USER.to_string(),
            amount,
            Err(PromiseError::Failed),
        ));
        assert_eq!(
            contract.storage_credit_of(USER.to_string()).0,
            available_after + amount.0
        );
        assert_eq!(contract.withdrawal_nonce(USER.to_string()), 1);
    }

    #[test]
    #[should_panic(expected = "invalid withdrawal signature")]
    fn rejects_signature_bound_to_another_recipient() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        register_solana_owner(&mut contract, &key);
        fund_record(&mut contract, USER, 2_500_000, "deposit-tamper");

        let wrong_message = withdrawal_message(
            USER,
            &"attacker.near".parse().unwrap(),
            AMOUNT,
            1,
            EXPIRY_MS,
            U128(0),
        );
        let signature = key.sign(wrong_message.as_bytes());
        let _promise = contract.withdraw(
            USER.to_string(),
            "receiver.near".parse().unwrap(),
            AMOUNT,
            1,
            EXPIRY_MS,
            U128(0),
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "invalid withdrawal signature")]
    fn one_logical_record_cannot_withdraw_another_records_balance() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[7; 32]);
        let other_user_key = SigningKey::from_bytes(&[8; 32]);
        register_solana_owner_for(&mut contract, USER, &user_key);
        register_solana_owner_for(&mut contract, OTHER_USER, &other_user_key);
        fund_record(&mut contract, USER, 2_500_000, "deposit-user-record");
        fund_record(
            &mut contract,
            OTHER_USER,
            2_500_000,
            "deposit-other-user-record",
        );

        let receiver: AccountId = "receiver.near".parse().unwrap();
        let message = withdrawal_message(USER, &receiver, AMOUNT, 1, EXPIRY_MS, U128(0));
        let signature = user_key.sign(message.as_bytes());
        let _promise = contract.withdraw(
            OTHER_USER.to_string(),
            receiver,
            AMOUNT,
            1,
            EXPIRY_MS,
            U128(0),
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "invalid withdrawal nonce")]
    fn rejects_replaying_a_valid_withdrawal_signature() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        register_solana_owner(&mut contract, &key);
        fund_record(&mut contract, USER, 2_500_000, "deposit-replay");

        let _first = signed_withdrawal(&mut contract, &key, "receiver.near", 1, EXPIRY_MS, 0);
        let _replay = signed_withdrawal(&mut contract, &key, "receiver.near", 1, EXPIRY_MS, 0);
    }

    #[test]
    #[should_panic(expected = "authorization expired")]
    fn rejects_expired_withdrawal_authorization() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        register_solana_owner(&mut contract, &key);
        let _promise = signed_withdrawal(&mut contract, &key, "receiver.near", 1, NOW_MS, 0);
    }

    #[test]
    #[should_panic(expected = "authorization expiry is too far in the future")]
    fn rejects_authorization_with_excessive_ttl() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        register_solana_owner(&mut contract, &key);
        let _promise = signed_withdrawal(
            &mut contract,
            &key,
            "receiver.near",
            1,
            NOW_MS + MAX_AUTHORIZATION_TTL_MS + 1,
            0,
        );
    }

    #[test]
    #[should_panic(expected = "withdrawal fees are not supported yet")]
    fn rejects_nonzero_fee_cap() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        register_solana_owner(&mut contract, &key);
        let _promise = signed_withdrawal(&mut contract, &key, "receiver.near", 1, EXPIRY_MS, 1);
    }

    #[test]
    #[should_panic(expected = "insufficient free balance")]
    fn rejects_withdrawal_above_the_logical_record_balance() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        register_solana_owner(&mut contract, &key);
        let _promise = signed_withdrawal(&mut contract, &key, "receiver.near", 1, EXPIRY_MS, 0);
    }

    #[test]
    fn failed_token_transfer_restores_record_balance_but_spends_nonce() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[7; 32]);
        register_solana_owner(&mut contract, &key);
        fund_record(&mut contract, USER, 2_500_000, "deposit-failure");
        let _promise = signed_withdrawal(&mut contract, &key, "receiver.near", 1, EXPIRY_MS, 0);

        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id("a1.near".parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        assert!(!contract.on_withdraw_complete(
            USER.to_string(),
            AMOUNT,
            Err(PromiseError::Failed),
        ));

        assert_eq!(contract.balance_of(USER.to_string()), U128(2_500_000));
        assert_eq!(contract.withdrawal_nonce(USER.to_string()), 1);
    }

    #[test]
    fn ethereum_eip191_signature_authorizes_shared_record_withdrawal() {
        let mut contract = setup(TOKEN);
        let secret = SecretKey::from_slice(&[9; 32]).unwrap();
        let public_key = secp256k1::PublicKey::from_secret_key(&Secp256k1::new(), &secret);
        let serialized = public_key.serialize_uncompressed();
        let hash = env::keccak256_array(&serialized[1..]);
        let mut address = [0; 20];
        address.copy_from_slice(&hash[12..]);
        let owner = OwnerKey::Ethereum { address };
        let register_msg = registration_message(USER, &owner, EXPIRY_MS);
        let register_digest = eip191_digest(&register_msg);
        let register_signature = Secp256k1::new()
            .sign_ecdsa_recoverable(&Message::from_slice(&register_digest).unwrap(), &secret);
        let (recovery_id, compact) = register_signature.serialize_compact();
        let mut register_bytes = compact.to_vec();
        register_bytes.push(recovery_id.to_i32() as u8);
        fund_test_storage_credit(USER);
        contract.register_owner(
            USER.to_string(),
            owner,
            EXPIRY_MS,
            Base64VecU8::from(register_bytes),
        );
        fund_record(&mut contract, USER, 2_500_000, "deposit-ethereum");

        let receiver: AccountId = "receiver.near".parse().unwrap();
        let message = withdrawal_message(USER, &receiver, AMOUNT, 1, EXPIRY_MS, U128(0));
        let signature = Secp256k1::new().sign_ecdsa_recoverable(
            &Message::from_slice(&eip191_digest(&message)).unwrap(),
            &secret,
        );
        let (recovery_id, compact) = signature.serialize_compact();
        let mut signature_bytes = compact.to_vec();
        signature_bytes.push(recovery_id.to_i32() as u8);
        let _promise = contract.withdraw(
            USER.to_string(),
            receiver,
            AMOUNT,
            1,
            EXPIRY_MS,
            U128(0),
            Base64VecU8::from(signature_bytes),
        );

        assert_eq!(contract.balance_of(USER.to_string()), U128(1_500_000));
        assert_eq!(contract.withdrawal_nonce(USER.to_string()), 1);
    }

    #[test]
    fn near_nep413_signature_authorizes_shared_record_withdrawal() {
        let mut contract = setup(TOKEN);
        let key = SigningKey::from_bytes(&[11; 32]);
        let owner = OwnerKey::NearEd25519 {
            public_key: key.verifying_key().to_bytes(),
        };
        let registration = registration_message(USER, &owner, EXPIRY_MS);
        let registration_hash =
            nep413_message_hash(&registration, &"a1.near".parse().unwrap(), &nep413_nonce(0));
        let registration_signature = key.sign(&registration_hash);
        fund_test_storage_credit(USER);
        contract.register_owner(
            USER.to_string(),
            owner,
            EXPIRY_MS,
            Base64VecU8::from(registration_signature.to_bytes().to_vec()),
        );
        fund_record(&mut contract, USER, 2_500_000, "deposit-near");

        let receiver: AccountId = "receiver.near".parse().unwrap();
        let message = withdrawal_message(USER, &receiver, AMOUNT, 1, EXPIRY_MS, U128(0));
        let withdrawal_hash =
            nep413_message_hash(&message, &"a1.near".parse().unwrap(), &nep413_nonce(1));
        let signature = key.sign(&withdrawal_hash);
        let _promise = contract.withdraw(
            USER.to_string(),
            receiver,
            AMOUNT,
            1,
            EXPIRY_MS,
            U128(0),
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );

        assert_eq!(contract.balance_of(USER.to_string()), U128(1_500_000));
        assert_eq!(contract.withdrawal_nonce(USER.to_string()), 1);
    }

    fn setup_agent(
        contract: &mut SharedLedger,
        user_key: &SigningKey,
        agent_key: &SigningKey,
    ) -> String {
        register_solana_owner_for(contract, USER, user_key);
        let agent_id = format!("agent:{USER}:primary");
        let agent_owner = OwnerKey::SolanaEd25519 {
            public_key: agent_key.verifying_key().to_bytes(),
        };
        let markets = vec!["NVDA".to_string(), "TSLA".to_string()];
        let expiry = NOW_MS + 24 * 60 * 60 * 1000;
        let message = agent_setup_message(
            USER,
            &agent_id,
            &agent_owner,
            &markets,
            U128(2_000_000),
            U128(3_000_000),
            20_000,
            U128(2_500_000),
            1,
            expiry,
        );
        let signature = user_key.sign(message.as_bytes());
        contract.create_agent_record(
            USER.to_string(),
            agent_id.clone(),
            agent_owner,
            markets,
            U128(2_000_000),
            U128(3_000_000),
            20_000,
            U128(2_500_000),
            expiry,
            1,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
        agent_id
    }

    fn fund_test_agent(
        contract: &mut SharedLedger,
        user_key: &SigningKey,
        agent_id: &str,
        amount: u128,
    ) {
        let expiry = EXPIRY_MS;
        let message = agent_funding_message(USER, agent_id, U128(amount), 2, expiry);
        let signature = user_key.sign(message.as_bytes());
        contract.fund_agent_record(
            USER.to_string(),
            U128(amount),
            2,
            expiry,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    fn publish_test_price(contract: &mut SharedLedger, price: U128, sequence: u64) {
        let signature = match (price.0, sequence) {
            (10_000_000_000, 1) => TEST_PRICE_100_SIGNATURE,
            (11_000_000_000, 2) => TEST_PRICE_110_SIGNATURE,
            _ => panic!("no signed test price fixture"),
        };
        fund_test_storage_credit("protocol:market_prices");
        contract.publish_market_price(
            "NVDA".to_string(),
            price,
            NOW_MS,
            sequence,
            Base64VecU8::from(signature.to_vec()),
        );
    }

    #[test]
    fn signed_price_updates_are_monotonic_and_feed_position_pnl() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[41; 32]);
        let agent_key = SigningKey::from_bytes(&[42; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(&mut contract, USER, 2_000_000, "deposit-price-pnl");
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            2_000_000,
            "deposit-test-vault",
        );
        fund_test_agent(&mut contract, &user_key, &agent_id, 1_000_000);
        publish_test_price(&mut contract, U128(10_000_000_000), 1);

        let open_expiry = NOW_MS + 30_000;
        let open_message = agent_market_open_message(
            &agent_id,
            "priced-position",
            "NVDA",
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            1,
            open_expiry,
        );
        let open_signature = agent_key.sign(open_message.as_bytes());
        assert_eq!(
            contract.open_agent_market_position(
                agent_id.clone(),
                "priced-position".to_string(),
                "NVDA".to_string(),
                true,
                U128(500_000),
                20_000,
                U128(10_000_000_000),
                1,
                100,
                1,
                open_expiry,
                Base64VecU8::from(open_signature.to_bytes().to_vec()),
            ),
            U128(250_000)
        );
        assert_eq!(
            contract
                .agent_position_pnl(agent_id.clone(), "priced-position".to_string())
                .unwrap()
                .unrealized_pnl_usdc,
            "0"
        );

        publish_test_price(&mut contract, U128(11_000_000_000), 2);
        let pnl = contract
            .agent_position_pnl(agent_id.clone(), "priced-position".to_string())
            .unwrap();
        assert_eq!(pnl.entry_price, U128(10_000_000_000));
        assert_eq!(pnl.mark_price, U128(11_000_000_000));
        assert_eq!(pnl.unrealized_pnl_usdc, "50000");
        assert_eq!(
            contract.agent_reserved_margin(agent_id.clone()),
            U128(250_000)
        );
        assert_eq!(contract.vault_balance(), U128(2_000_000));
        assert_eq!(contract.vault_reserved_payout(), U128(500_000));

        let close_expiry = NOW_MS + 30_000;
        let close_message = market_close_message(
            "owner",
            USER,
            &read_agent_position(&agent_id, "priced-position").unwrap(),
            U128(11_000_000_000),
            2,
            100,
            3,
            close_expiry,
        );
        let close_signature = user_key.sign(close_message.as_bytes());
        assert_eq!(
            contract.owner_close_agent_market_position(
                USER.to_string(),
                "priced-position".to_string(),
                U128(11_000_000_000),
                2,
                100,
                3,
                close_expiry,
                Base64VecU8::from(close_signature.to_bytes().to_vec()),
            ),
            U128(300_000)
        );
        assert_eq!(contract.balance_of(agent_id.clone()), U128(1_050_000));
        assert_eq!(contract.vault_balance(), U128(1_950_000));
        assert_eq!(contract.vault_reserved_payout(), U128(0));
        assert!(contract
            .agent_position(agent_id, "priced-position".to_string())
            .is_none());
    }

    #[test]
    #[should_panic(expected = "invalid market price signature")]
    fn rejects_market_price_signed_for_another_market() {
        let mut contract = setup(TOKEN);
        contract.publish_market_price(
            "TSLA".to_string(),
            U128(10_000_000_000),
            NOW_MS,
            1,
            Base64VecU8::from(TEST_PRICE_100_SIGNATURE.to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "invalid market price signature")]
    fn rejects_market_price_signed_for_another_contract() {
        let mut contract = setup(TOKEN);
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("other.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        contract.publish_market_price(
            "NVDA".to_string(),
            U128(10_000_000_000),
            NOW_MS,
            1,
            Base64VecU8::from(TEST_PRICE_100_SIGNATURE.to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "market price signature length")]
    fn rejects_malformed_market_price_signature() {
        let mut contract = setup(TOKEN);
        contract.publish_market_price(
            "NVDA".to_string(),
            U128(10_000_000_000),
            NOW_MS,
            1,
            Base64VecU8::from(vec![1, 2, 3]),
        );
    }

    #[test]
    #[should_panic(expected = "market price is stale")]
    fn rejects_stale_market_price_before_signature_check() {
        let mut contract = setup(TOKEN);
        contract.publish_market_price(
            "NVDA".to_string(),
            U128(10_000_000_000),
            NOW_MS - MAX_MARKET_PRICE_AGE_MS - 1,
            1,
            Base64VecU8::from(vec![0; 64]),
        );
    }

    #[test]
    #[should_panic(expected = "market price timestamp is in the future")]
    fn rejects_future_market_price_before_signature_check() {
        let mut contract = setup(TOKEN);
        contract.publish_market_price(
            "NVDA".to_string(),
            U128(10_000_000_000),
            NOW_MS + 1,
            1,
            Base64VecU8::from(vec![0; 64]),
        );
    }

    #[test]
    #[should_panic(expected = "market price sequence must increase")]
    fn rejects_replayed_market_price_sequence() {
        let mut contract = setup(TOKEN);
        publish_test_price(&mut contract, U128(10_000_000_000), 1);
        publish_test_price(&mut contract, U128(10_000_000_000), 1);
    }

    #[test]
    #[should_panic(expected = "market price exceeded signed slippage")]
    fn rejects_agent_open_when_mark_exceeds_signed_slippage() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[43; 32]);
        let agent_key = SigningKey::from_bytes(&[44; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(&mut contract, USER, 2_000_000, "deposit-price-slippage");
        fund_test_agent(&mut contract, &user_key, &agent_id, 1_000_000);
        publish_test_price(&mut contract, U128(10_000_000_000), 1);
        publish_test_price(&mut contract, U128(11_000_000_000), 2);

        contract.open_agent_market_position(
            agent_id,
            "slippage-position".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            1,
            NOW_MS + 30_000,
            Base64VecU8::from(vec![0; 64]),
        );
    }

    #[test]
    #[should_panic(expected = "insufficient unreserved vault liquidity")]
    fn price_aware_open_requires_funded_vault_capacity() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[51; 32]);
        let agent_key = SigningKey::from_bytes(&[52; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(&mut contract, USER, 2_000_000, "deposit-no-vault");
        fund_test_agent(&mut contract, &user_key, &agent_id, 1_000_000);
        publish_test_price(&mut contract, U128(10_000_000_000), 1);
        let expiry = NOW_MS + 30_000;
        let message = agent_market_open_message(
            &agent_id,
            "no-vault-position",
            "NVDA",
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            1,
            expiry,
        );
        let signature = agent_key.sign(message.as_bytes());
        contract.open_agent_market_position(
            agent_id,
            "no-vault-position".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            1,
            expiry,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "protocol records cannot have an owner")]
    fn protocol_vault_record_cannot_be_claimed_by_a_wallet() {
        let mut contract = setup(TOKEN);
        contract.register_owner(
            SHARED_VAULT_RECORD_ID.to_string(),
            OwnerKey::NearEd25519 {
                public_key: [0; 32],
            },
            EXPIRY_MS,
            Base64VecU8::from(vec![0; 64]),
        );
    }

    #[test]
    #[should_panic(expected = "invalid agent action signature")]
    fn rejects_price_aware_open_signed_by_another_agent() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[45; 32]);
        let agent_key = SigningKey::from_bytes(&[46; 32]);
        let attacker_key = SigningKey::from_bytes(&[47; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(&mut contract, USER, 2_000_000, "deposit-price-owner");
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            2_000_000,
            "deposit-test-vault-owner",
        );
        fund_test_agent(&mut contract, &user_key, &agent_id, 1_000_000);
        publish_test_price(&mut contract, U128(10_000_000_000), 1);
        let expiry = NOW_MS + 30_000;
        let message = agent_market_open_message(
            &agent_id,
            "wrong-agent-position",
            "NVDA",
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            1,
            expiry,
        );
        let signature = attacker_key.sign(message.as_bytes());
        contract.open_agent_market_position(
            agent_id,
            "wrong-agent-position".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            1,
            expiry,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "invalid agent funding signature")]
    fn owner_cannot_change_agent_allocation_after_signing() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[61; 32]);
        let agent_key = SigningKey::from_bytes(&[62; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(
            &mut contract,
            USER,
            2_000_000,
            "deposit-agent-allocation-tamper",
        );
        let expiry = NOW_MS + 30_000;
        let signed_message = agent_funding_message(USER, &agent_id, U128(500_000), 2, expiry);
        let signature = user_key.sign(signed_message.as_bytes());

        contract.fund_agent_record(
            USER.to_string(),
            U128(600_000),
            2,
            expiry,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "invalid agent admin nonce")]
    fn agent_allocation_signature_cannot_be_replayed() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[63; 32]);
        let agent_key = SigningKey::from_bytes(&[64; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(
            &mut contract,
            USER,
            2_000_000,
            "deposit-agent-allocation-replay",
        );
        fund_test_agent(&mut contract, &user_key, &agent_id, 500_000);
        let expiry = NOW_MS + 30_000;
        let message = agent_funding_message(USER, &agent_id, U128(500_000), 3, expiry);
        let signature = user_key.sign(message.as_bytes());
        contract.fund_agent_record(
            USER.to_string(),
            U128(500_000),
            3,
            expiry,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );

        contract.fund_agent_record(
            USER.to_string(),
            U128(500_000),
            3,
            expiry,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "invalid agent revocation signature")]
    fn another_wallet_cannot_revoke_an_agent() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[65; 32]);
        let agent_key = SigningKey::from_bytes(&[66; 32]);
        let attacker_key = SigningKey::from_bytes(&[67; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        let expiry = NOW_MS + 30_000;
        let message = agent_revocation_message(USER, &agent_id, 2, expiry);
        let signature = attacker_key.sign(message.as_bytes());

        contract.revoke_agent(
            USER.to_string(),
            2,
            expiry,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "invalid agent action signature")]
    fn agent_cannot_increase_position_size_after_signing() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[68; 32]);
        let agent_key = SigningKey::from_bytes(&[69; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(&mut contract, USER, 2_000_000, "deposit-market-size-tamper");
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            2_000_000,
            "deposit-market-size-tamper-vault",
        );
        fund_test_agent(&mut contract, &user_key, &agent_id, 1_000_000);
        publish_test_price(&mut contract, U128(10_000_000_000), 1);
        let expiry = NOW_MS + 30_000;
        let message = agent_market_open_message(
            &agent_id,
            "tampered-size-position",
            "NVDA",
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            1,
            expiry,
        );
        let signature = agent_key.sign(message.as_bytes());

        contract.open_agent_market_position(
            agent_id,
            "tampered-size-position".to_string(),
            "NVDA".to_string(),
            true,
            U128(600_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            1,
            expiry,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    fn failed_second_open_does_not_consume_reserved_vault_capacity_or_nonce() {
        use std::panic::{catch_unwind, AssertUnwindSafe};

        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[70; 32]);
        let agent_key = SigningKey::from_bytes(&[71; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(&mut contract, USER, 2_000_000, "deposit-vault-reserve-edge");
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            500_000,
            "deposit-vault-reserve-edge-vault",
        );
        fund_test_agent(&mut contract, &user_key, &agent_id, 1_000_000);
        publish_test_price(&mut contract, U128(10_000_000_000), 1);
        let expiry = NOW_MS + 30_000;
        let first_message = agent_market_open_message(
            &agent_id,
            "first-reserved-position",
            "NVDA",
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            1,
            expiry,
        );
        let first_signature = agent_key.sign(first_message.as_bytes());
        contract.open_agent_market_position(
            agent_id.clone(),
            "first-reserved-position".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            1,
            expiry,
            Base64VecU8::from(first_signature.to_bytes().to_vec()),
        );

        let second_message = agent_market_open_message(
            &agent_id,
            "second-reserved-position",
            "NVDA",
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            2,
            expiry,
        );
        let second_signature = agent_key.sign(second_message.as_bytes());
        let second_open = catch_unwind(AssertUnwindSafe(|| {
            contract.open_agent_market_position(
                agent_id.clone(),
                "second-reserved-position".to_string(),
                "NVDA".to_string(),
                true,
                U128(500_000),
                20_000,
                U128(10_000_000_000),
                1,
                100,
                2,
                expiry,
                Base64VecU8::from(second_signature.to_bytes().to_vec()),
            )
        }));

        assert!(second_open.is_err());
        assert_eq!(contract.vault_reserved_payout(), U128(500_000));
        assert_eq!(contract.vault_available_payout(), U128(0));
        assert_eq!(contract.agent_action_nonce(agent_id.clone()), 1);
        assert_eq!(
            contract.agent_open_notional(agent_id.clone()),
            U128(500_000)
        );
        assert_eq!(
            contract.agent_reserved_margin(agent_id.clone()),
            U128(250_000)
        );
        assert!(contract
            .agent_position(agent_id.clone(), "first-reserved-position".to_string())
            .is_some());
        assert!(contract
            .agent_position(agent_id, "second-reserved-position".to_string())
            .is_none());
    }

    #[test]
    #[should_panic(expected = "agent authorization expired")]
    fn expired_agent_cannot_open_a_position() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[72; 32]);
        let agent_key = SigningKey::from_bytes(&[73; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(&mut contract, USER, 2_000_000, "deposit-expired-agent-open");
        fund_test_agent(&mut contract, &user_key, &agent_id, 1_000_000);
        let config = contract.agent_config(agent_id.clone()).unwrap();

        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id("a1.near".parse().unwrap())
            .block_timestamp((config.expires_at_ms + 1) * 1_000_000);
        testing_env!(context.build());

        contract.open_agent_position(
            agent_id,
            "expired-position".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            1,
            NOW_MS + 30_000,
            Base64VecU8::from(vec![0; 64]),
        );
    }

    #[test]
    #[should_panic(expected = "invalid agent setup signature")]
    fn owner_signature_does_not_authorize_a_tampered_agent_policy() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[74; 32]);
        let agent_key = SigningKey::from_bytes(&[75; 32]);
        register_solana_owner_for(&mut contract, USER, &user_key);

        let agent_id = format!("agent:{USER}:policy-tamper");
        let agent_owner = OwnerKey::SolanaEd25519 {
            public_key: agent_key.verifying_key().to_bytes(),
        };
        let markets = vec!["NVDA".to_string()];
        let expiry = NOW_MS + 24 * 60 * 60 * 1000;
        let signed_message = agent_setup_message(
            USER,
            &agent_id,
            &agent_owner,
            &markets,
            U128(2_000_000),
            U128(3_000_000),
            20_000,
            U128(2_500_000),
            1,
            expiry,
        );
        let signature = user_key.sign(signed_message.as_bytes());

        contract.create_agent_record(
            USER.to_string(),
            agent_id,
            agent_owner,
            markets,
            U128(1_000_000),
            U128(3_000_000),
            20_000,
            U128(2_500_000),
            expiry,
            1,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "user already has an active agent")]
    fn active_agent_policy_cannot_be_replaced_in_place() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[76; 32]);
        let first_agent_key = SigningKey::from_bytes(&[77; 32]);
        let replacement_key = SigningKey::from_bytes(&[78; 32]);
        setup_agent(&mut contract, &user_key, &first_agent_key);

        let replacement_id = format!("agent:{USER}:replacement");
        let replacement_owner = OwnerKey::SolanaEd25519 {
            public_key: replacement_key.verifying_key().to_bytes(),
        };
        let markets = vec!["NVDA".to_string()];
        let expiry = NOW_MS + 24 * 60 * 60 * 1000;
        let message = agent_setup_message(
            USER,
            &replacement_id,
            &replacement_owner,
            &markets,
            U128(2_000_000),
            U128(3_000_000),
            20_000,
            U128(2_500_000),
            2,
            expiry,
        );
        let signature = user_key.sign(message.as_bytes());

        contract.create_agent_record(
            USER.to_string(),
            replacement_id,
            replacement_owner,
            markets,
            U128(2_000_000),
            U128(3_000_000),
            20_000,
            U128(2_500_000),
            expiry,
            2,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "agent is disabled")]
    fn revoked_agent_cannot_receive_a_new_allocation() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[79; 32]);
        let agent_key = SigningKey::from_bytes(&[80; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(
            &mut contract,
            USER,
            2_000_000,
            "deposit-revoked-agent-funding",
        );

        let revoke_expiry = EXPIRY_MS;
        let revoke_message = agent_revocation_message(USER, &agent_id, 2, revoke_expiry);
        let revoke_signature = user_key.sign(revoke_message.as_bytes());
        contract.revoke_agent(
            USER.to_string(),
            2,
            revoke_expiry,
            Base64VecU8::from(revoke_signature.to_bytes().to_vec()),
        );

        let funding_expiry = NOW_MS + 30_000;
        let funding_message =
            agent_funding_message(USER, &agent_id, U128(500_000), 3, funding_expiry);
        let funding_signature = user_key.sign(funding_message.as_bytes());
        contract.fund_agent_record(
            USER.to_string(),
            U128(500_000),
            3,
            funding_expiry,
            Base64VecU8::from(funding_signature.to_bytes().to_vec()),
        );
    }

    #[test]
    fn old_agent_position_storage_defaults_entry_price_to_none() {
        let legacy_position = r#"{
            "position_id":"legacy-position",
            "agent_record_id":"agent:near:test-user:primary",
            "market_id":"NVDA",
            "is_long":true,
            "notional_usdc":"1000000",
            "margin_usdc":"500000",
            "leverage_bps":20000
        }"#;
        let position: AgentPosition = near_sdk::serde_json::from_str(legacy_position).unwrap();
        assert_eq!(position.entry_price, None);
    }

    #[test]
    fn agent_open_reserves_margin_and_owner_can_reclaim_after_revocation_and_close() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[21; 32]);
        let agent_key = SigningKey::from_bytes(&[22; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(&mut contract, USER, 10_000_000, "deposit-agent-user");
        fund_test_agent(&mut contract, &user_key, &agent_id, 2_500_000);

        let action_expiry = NOW_MS + 30_000;
        let open_message = agent_open_message(
            &agent_id,
            "position-1",
            "NVDA",
            true,
            U128(2_000_000),
            20_000,
            1,
            action_expiry,
        );
        let open_signature = agent_key.sign(open_message.as_bytes());
        assert_eq!(
            contract.open_agent_position(
                agent_id.clone(),
                "position-1".to_string(),
                "NVDA".to_string(),
                true,
                U128(2_000_000),
                20_000,
                1,
                action_expiry,
                Base64VecU8::from(open_signature.to_bytes().to_vec()),
            ),
            U128(1_000_000)
        );
        assert_eq!(contract.balance_of(agent_id.clone()), U128(1_500_000));
        assert_eq!(
            contract.agent_reserved_margin(agent_id.clone()),
            U128(1_000_000)
        );
        assert_eq!(
            contract.agent_open_notional(agent_id.clone()),
            U128(2_000_000)
        );

        let revoke_message = agent_revocation_message(USER, &agent_id, 3, EXPIRY_MS);
        let revoke_signature = user_key.sign(revoke_message.as_bytes());
        assert_eq!(
            contract.revoke_agent(
                USER.to_string(),
                3,
                EXPIRY_MS,
                Base64VecU8::from(revoke_signature.to_bytes().to_vec()),
            ),
            U128(1_500_000)
        );
        assert_eq!(contract.balance_of(agent_id.clone()), U128(0));
        assert_eq!(contract.balance_of(USER.to_string()), U128(9_000_000));

        let close_expiry = NOW_MS + 30_000;
        let close_message = owner_close_agent_message(
            USER,
            &read_agent_position(&agent_id, "position-1").unwrap(),
            4,
            close_expiry,
        );
        let close_signature = user_key.sign(close_message.as_bytes());
        assert_eq!(
            contract.owner_close_agent_position(
                USER.to_string(),
                "position-1".to_string(),
                4,
                close_expiry,
                Base64VecU8::from(close_signature.to_bytes().to_vec()),
            ),
            U128(1_000_000)
        );
        assert_eq!(contract.balance_of(USER.to_string()), U128(10_000_000));
        assert_eq!(contract.agent_reserved_margin(agent_id.clone()), U128(0));
        assert_eq!(contract.agent_open_notional(agent_id.clone()), U128(0));
        assert_eq!(
            contract.agent_position(agent_id, "position-1".to_string()),
            None
        );
    }

    #[test]
    #[should_panic(expected = "max open notional limit exceeded")]
    fn agent_cannot_exceed_aggregate_open_notional() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[23; 32]);
        let agent_key = SigningKey::from_bytes(&[24; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(&mut contract, USER, 10_000_000, "deposit-agent-cap");
        fund_test_agent(&mut contract, &user_key, &agent_id, 2_500_000);
        let first_expiry = NOW_MS + 30_000;
        let first = agent_open_message(
            &agent_id,
            "position-1",
            "NVDA",
            true,
            U128(2_000_000),
            20_000,
            1,
            first_expiry,
        );
        let first_signature = agent_key.sign(first.as_bytes());
        contract.open_agent_position(
            agent_id.clone(),
            "position-1".to_string(),
            "NVDA".to_string(),
            true,
            U128(2_000_000),
            20_000,
            1,
            first_expiry,
            Base64VecU8::from(first_signature.to_bytes().to_vec()),
        );
        let second_expiry = NOW_MS + 40_000;
        let second = agent_open_message(
            &agent_id,
            "position-2",
            "TSLA",
            false,
            U128(2_000_000),
            20_000,
            2,
            second_expiry,
        );
        let second_signature = agent_key.sign(second.as_bytes());
        contract.open_agent_position(
            agent_id,
            "position-2".to_string(),
            "TSLA".to_string(),
            false,
            U128(2_000_000),
            20_000,
            2,
            second_expiry,
            Base64VecU8::from(second_signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "market is not allowed for this agent")]
    fn agent_cannot_open_a_position_in_an_unapproved_market() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[25; 32]);
        let agent_key = SigningKey::from_bytes(&[26; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        fund_record(&mut contract, USER, 10_000_000, "deposit-agent-market");
        fund_test_agent(&mut contract, &user_key, &agent_id, 2_500_000);
        let expiry = NOW_MS + 30_000;
        let message = agent_open_message(
            &agent_id,
            "position-1",
            "AAPL",
            true,
            U128(1_000_000),
            10_000,
            1,
            expiry,
        );
        let signature = agent_key.sign(message.as_bytes());
        contract.open_agent_position(
            agent_id,
            "position-1".to_string(),
            "AAPL".to_string(),
            true,
            U128(1_000_000),
            10_000,
            1,
            expiry,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "agent is disabled")]
    fn revoked_agent_cannot_open_new_positions() {
        let mut contract = setup(TOKEN);
        let user_key = SigningKey::from_bytes(&[27; 32]);
        let agent_key = SigningKey::from_bytes(&[28; 32]);
        let agent_id = setup_agent(&mut contract, &user_key, &agent_key);
        let revoke_message = agent_revocation_message(USER, &agent_id, 2, EXPIRY_MS);
        let revoke_signature = user_key.sign(revoke_message.as_bytes());
        contract.revoke_agent(
            USER.to_string(),
            2,
            EXPIRY_MS,
            Base64VecU8::from(revoke_signature.to_bytes().to_vec()),
        );
        let expiry = NOW_MS + 30_000;
        let message = agent_open_message(
            &agent_id,
            "position-1",
            "NVDA",
            true,
            U128(1_000_000),
            10_000,
            1,
            expiry,
        );
        let signature = agent_key.sign(message.as_bytes());
        contract.open_agent_position(
            agent_id,
            "position-1".to_string(),
            "NVDA".to_string(),
            true,
            U128(1_000_000),
            10_000,
            1,
            expiry,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    fn eip191_digest(message: &str) -> [u8; 32] {
        let mut personal_message =
            format!("\x19Ethereum Signed Message:\n{}", message.len()).into_bytes();
        personal_message.extend_from_slice(message.as_bytes());
        env::keccak256_array(&personal_message)
    }
}
