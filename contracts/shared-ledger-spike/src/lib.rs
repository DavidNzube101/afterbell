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
const PERP_ORACLE_RECOVERY_DEVIATION_BPS: u128 = 50;
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
const AGENT_DAILY_LOSS_PREFIX: &[u8] = b"afterbell:a1:agent_daily_loss:";
const MARKET_PRICE_PREFIX: &[u8] = b"afterbell:a1:market_price:";
const VAULT_RESERVED_PREFIX: &[u8] = b"afterbell:a1:vault_reserved_payout";
const STORAGE_CREDIT_PREFIX: &[u8] = b"afterbell:a1:storage_credit:";
const PERP_MARKET_PREFIX: &[u8] = b"afterbell:a1:perp:market:";
const PERP_PENDING_MARKET_PREFIX: &[u8] = b"afterbell:a1:perp:pending_market:";
const PERP_PENDING_MARKET_AT_PREFIX: &[u8] = b"afterbell:a1:perp:pending_market_at:";
const PERP_MARKET_IDS_KEY: &[u8] = b"afterbell:a1:perp:market_ids";
const PERP_POSITION_PREFIX: &[u8] = b"afterbell:a1:perp:position:";
const PERP_EXPOSURE_PREFIX: &[u8] = b"afterbell:a1:perp:exposure:";
const PERP_QUOTE_SIGNER_KEY: &[u8] = b"afterbell:a1:perp:quote_signer";
const PERP_PENDING_QUOTE_SIGNER_KEY: &[u8] = b"afterbell:a1:perp:pending_quote_signer";
const PERP_PENDING_QUOTE_SIGNER_AT_KEY: &[u8] = b"afterbell:a1:perp:pending_quote_signer_at";
const PERP_QUOTE_NONCE_PREFIX: &[u8] = b"afterbell:a1:perp:quote_nonce:";
const PERP_POSITION_COUNT_PREFIX: &[u8] = b"afterbell:a1:perp:position_count:";
const PERP_OPEN_NOTIONAL_PREFIX: &[u8] = b"afterbell:a1:perp:open_notional:";
const PERP_TOTAL_MARGIN_KEY: &[u8] = b"afterbell:a1:perp:total_margin";
const PERP_TOTAL_OI_KEY: &[u8] = b"afterbell:a1:perp:total_oi";
const PERP_GLOBAL_NET_KEY: &[u8] = b"afterbell:a1:perp:global_net";
const PERP_BAD_DEBT_KEY: &[u8] = b"afterbell:a1:perp:bad_debt";
const PERP_INSURANCE_KEY: &[u8] = b"afterbell:a1:perp:insurance";
const PERP_PAUSED_KEY: &[u8] = b"afterbell:a1:perp:paused";
const PERP_ORACLE_SIGNER_KEY: &[u8] = b"afterbell:a1:perp:oracle_signer";
const PERP_PENDING_ORACLE_SIGNER_KEY: &[u8] = b"afterbell:a1:perp:pending_oracle_signer";
const PERP_PENDING_ORACLE_SIGNER_AT_KEY: &[u8] = b"afterbell:a1:perp:pending_oracle_signer_at";
const PERP_BETA_CAP_KEY: &[u8] = b"afterbell:a1:perp:beta_cap";
const PERP_ORACLE_HEALTH_PREFIX: &[u8] = b"afterbell:a1:perp:oracle_health:";
const PERP_ORACLE_PENDING_JUMP_PREFIX: &[u8] = b"afterbell:a1:perp:oracle_pending_jump:";
const PERP_DEFERRED_CLAIM_PREFIX: &[u8] = b"afterbell:a1:perp:deferred:";
const PERP_DEFERRED_TOTAL_KEY: &[u8] = b"afterbell:a1:perp:deferred_total";
const LP_SHARE_PREFIX: &[u8] = b"afterbell:a1:lp:shares:";
const LP_TOTAL_SHARES_KEY: &[u8] = b"afterbell:a1:lp:total_shares";
const LP_WITHDRAW_PREFIX: &[u8] = b"afterbell:a1:lp:withdraw:";
const LP_WITHDRAWAL_COOLDOWN_MS: u64 = 24 * 60 * 60 * 1000;
const PERP_QUOTE_SIGNER_DELAY_MS: u64 = 60 * 60 * 1000;
const PERP_MARKET_RISK_DELAY_MS: u64 = 20 * 60 * 1000;
const PERP_ORACLE_SIGNER_DELAY_MS: u64 = 20 * 60 * 1000;
const PERP_ORACLE_JUMP_CONFIRM_DELAY_MS: u64 = 5 * 60 * 1000;
const MAX_PERP_MARKETS: usize = 64;
const MAX_PERP_POSITIONS_PER_RECORD: u64 = 64;
const MAX_PERP_POSITION_USDC: u128 = 5_000_000_000_000;
const PERP_BASE_UNITS_SCALE: u128 = 1_000_000_000_000_000_000;
const MAX_PERP_LEVERAGE_BPS: u32 = 50_000;
const MAX_PERP_FEE_BPS: u16 = 100;
const MAX_PERP_SLIPPAGE_BPS: u16 = 1_000;
const PERP_BASE_FUNDING_BPS_HOURLY: u16 = 1;
const PERP_MAX_FUNDING_BPS_HOURLY: u16 = 25;
const PERP_MAX_POSITION_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const PERP_LIQUIDATION_PENALTY_BPS: u16 = 500;
const PERP_LIQUIDATOR_REWARD_BPS: u16 = 250;
const PERP_MAX_GLOBAL_NET_BPS: u16 = 1_000;
const PERP_MAX_TOTAL_OI_BPS: u16 = 5_000;
const PERP_MAX_MARKET_OI_BPS: u16 = 1_000;
const PERP_MAX_MARKET_SKEW_BPS: u16 = 300;
const PERP_MAX_TRADER_OI_BPS: u16 = 50;
const PERP_DEFAULT_BETA_CAP_USDC: u128 = 100_000_000_000;
const PROTOCOL_ADMIN_KEY: &[u8] = b"afterbell:a1:protocol_admin";

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
    /// Maximum realized trading loss allowed per UTC day; new agent opens stop at the cap.
    #[serde(default)]
    pub max_daily_loss_usdc: U128,
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

/// Risk controls for a listed perpetual market. Amount limits are raw USDC.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerpMarketConfig {
    pub market_id: String,
    pub enabled: bool,
    pub close_only: bool,
    pub max_leverage_bps: u32,
    pub maintenance_margin_bps: u16,
    pub max_execution_deviation_bps: u16,
    pub max_oracle_confidence_bps: u16,
    pub max_position_usdc: U128,
    pub max_market_oi_bps: u16,
    pub max_market_skew_bps: u16,
}

/// Isolated position held by one user or agent logical record.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerpPosition {
    pub account_id: String,
    pub position_id: String,
    pub market_id: String,
    pub is_long: bool,
    pub notional_usdc: U128,
    pub margin_usdc: U128,
    pub entry_price: U128,
    pub opened_at_ms: u64,
    pub funding_index: U128,
    #[serde(default)]
    pub funding_remainder: i128,
}

#[near(serializers = [json])]
pub struct PerpPositionView {
    pub account_id: String,
    pub position_id: String,
    pub market_id: String,
    pub is_long: bool,
    pub notional_usdc: U128,
    pub margin_usdc: U128,
    pub entry_price: U128,
    pub opened_at_ms: u64,
    /// Signed fixed-point funding index, encoded as a decimal string.
    pub funding_index: String,
    pub funding_remainder: String,
}

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PerpExposure {
    pub long_notional_usdc: U128,
    pub short_notional_usdc: U128,
    // Legacy arithmetic entry values remain readable for state compatibility.
    #[serde(default)]
    pub long_entry_value: U128,
    #[serde(default)]
    pub short_entry_value: U128,
    #[serde(default)]
    pub long_base_units: U128,
    #[serde(default)]
    pub short_base_units: U128,
    pub long_funding_index: U128,
    pub short_funding_index: U128,
    pub funding_updated_at_ms: u64,
}

#[near(serializers = [json])]
pub struct PerpExposureView {
    pub long_notional_usdc: U128,
    pub short_notional_usdc: U128,
    pub long_entry_value: U128,
    pub short_entry_value: U128,
    pub long_base_units: U128,
    pub short_base_units: U128,
    pub long_funding_index: String,
    pub short_funding_index: String,
    pub funding_updated_at_ms: u64,
}

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentDailyLoss {
    pub utc_day: u64,
    pub realized_loss_usdc: U128,
}

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LpWithdrawalRequest {
    pub shares: U128,
    #[serde(default)]
    pub min_amount_usdc: U128,
    pub requested_at_ms: u64,
}

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleAttestation {
    pub market_id: String,
    pub price: U128,
    pub spot_price: U128,
    pub confidence_bps: u16,
    pub observed_at_ms: u64,
    pub sequence: u64,
    pub source_round: String,
    pub expires_at_ms: u64,
}

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct OracleHealth {
    pub circuit_broken: bool,
    pub confidence_bps: u16,
    pub spot_twap_deviation_bps: u16,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
struct PendingOracleJump {
    price: MarketPrice,
    staged_at_ms: u64,
    source_round: String,
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
    pub fn new(usdc_token: AccountId, protocol_admin: AccountId) -> Self {
        write_json_storage(PROTOCOL_ADMIN_KEY, &protocol_admin);
        Self {
            usdc_token,
            balances: LookupMap::new(b"b"),
            processed_deposits: UnorderedSet::new(b"d"),
        }
    }

    pub fn protocol_admin_account(&self) -> AccountId {
        read_json_storage(PROTOCOL_ADMIN_KEY).expect("protocol admin is not initialized")
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

    pub fn agent_daily_loss(&self, agent_record_id: String) -> U128 {
        U128(read_agent_daily_loss(&agent_record_id).realized_loss_usdc.0)
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

    pub fn perp_paused(&self) -> bool {
        read_u64_storage(PERP_PAUSED_KEY) != 0
    }

    pub fn perp_market_ids(&self) -> Vec<String> {
        read_perp_market_ids()
    }

    pub fn perp_market(&self, market_id: String) -> Option<PerpMarketConfig> {
        read_perp_market(&market_id)
    }

    pub fn perp_quote_signer(&self) -> Option<OwnerKey> {
        read_perp_quote_signer()
    }

    pub fn set_perp_quote_signer(&mut self, signer: OwnerKey) {
        assert_protocol_admin();
        if read_perp_quote_signer().is_none() {
            write_perp_quote_signer(&signer);
        } else {
            write_json_storage(PERP_PENDING_QUOTE_SIGNER_KEY, &signer);
            write_u64_storage(
                PERP_PENDING_QUOTE_SIGNER_AT_KEY,
                env::block_timestamp() / 1_000_000 + PERP_QUOTE_SIGNER_DELAY_MS,
            );
        }
    }

    pub fn apply_perp_quote_signer(&mut self) {
        assert_protocol_admin();
        let ready_at = read_u64_storage(PERP_PENDING_QUOTE_SIGNER_AT_KEY);
        assert!(ready_at > 0, "no pending quote signer");
        assert!(
            env::block_timestamp() / 1_000_000 >= ready_at,
            "config delay active"
        );
        let signer: OwnerKey =
            read_json_storage(PERP_PENDING_QUOTE_SIGNER_KEY).expect("pending quote signer missing");
        write_perp_quote_signer(&signer);
        env::storage_remove(PERP_PENDING_QUOTE_SIGNER_KEY);
        env::storage_remove(PERP_PENDING_QUOTE_SIGNER_AT_KEY);
    }

    pub fn set_perp_beta_cap(&mut self, cap: U128) {
        assert_protocol_admin();
        assert!(cap.0 > 0, "beta cap must be positive");
        assert!(
            cap.0
                >= self
                    .balances
                    .get(&SHARED_VAULT_RECORD_ID.to_string())
                    .unwrap_or_default(),
            "cap below current vault cash"
        );
        write_u128_storage(PERP_BETA_CAP_KEY, cap.0);
    }

    pub fn perp_beta_cap(&self) -> U128 {
        U128(if read_u128_storage(PERP_BETA_CAP_KEY) == 0 {
            PERP_DEFAULT_BETA_CAP_USDC
        } else {
            read_u128_storage(PERP_BETA_CAP_KEY)
        })
    }

    pub fn perp_position(
        &self,
        account_id: String,
        position_id: String,
    ) -> Option<PerpPositionView> {
        read_perp_position(&account_id, &position_id).map(|position| PerpPositionView {
            account_id: position.account_id,
            position_id: position.position_id,
            market_id: position.market_id,
            is_long: position.is_long,
            notional_usdc: position.notional_usdc,
            margin_usdc: position.margin_usdc,
            entry_price: position.entry_price,
            opened_at_ms: position.opened_at_ms,
            funding_index: (position.funding_index.0 as i128).to_string(),
            funding_remainder: position.funding_remainder.to_string(),
        })
    }

    pub fn perp_exposure(&self, market_id: String) -> PerpExposureView {
        let exposure = read_perp_exposure(&market_id);
        PerpExposureView {
            long_notional_usdc: exposure.long_notional_usdc,
            short_notional_usdc: exposure.short_notional_usdc,
            long_entry_value: exposure.long_entry_value,
            short_entry_value: exposure.short_entry_value,
            long_base_units: exposure.long_base_units,
            short_base_units: exposure.short_base_units,
            long_funding_index: (exposure.long_funding_index.0 as i128).to_string(),
            short_funding_index: (exposure.short_funding_index.0 as i128).to_string(),
            funding_updated_at_ms: exposure.funding_updated_at_ms,
        }
    }

    pub fn perp_funding_rate_bps_per_hour(&self, market_id: String) -> u16 {
        let exposure = read_perp_exposure(&market_id);
        let skew = exposure
            .long_notional_usdc
            .0
            .abs_diff(exposure.short_notional_usdc.0);
        crate::engine_math::funding_rate_bps_per_hour(
            skew,
            self.balances
                .get(&SHARED_VAULT_RECORD_ID.to_string())
                .unwrap_or_default(),
            PERP_BASE_FUNDING_BPS_HOURLY,
            PERP_MAX_FUNDING_BPS_HOURLY,
        )
    }

    pub fn oracle_health(&self, market_id: String) -> OracleHealth {
        read_oracle_health(&market_id)
    }

    pub fn oracle_attestation_message(&self, attestation: OracleAttestation) -> String {
        build_oracle_attestation_message(&attestation)
    }

    pub fn oracle_signer(&self) -> Option<[u8; 20]> {
        read_json_storage(PERP_ORACLE_SIGNER_KEY)
    }

    /// Configure the single EOA that signs oracle attestations. Initial setup
    /// is immediate; rotations wait for the protocol configuration delay.
    pub fn configure_oracle_signer(&mut self, signer: [u8; 20]) {
        assert_protocol_admin();
        assert!(signer != [0; 20], "oracle signer cannot be zero address");
        if read_json_storage::<[u8; 20]>(PERP_ORACLE_SIGNER_KEY).is_none() {
            write_json_storage(PERP_ORACLE_SIGNER_KEY, &signer);
        } else {
            write_json_storage(PERP_PENDING_ORACLE_SIGNER_KEY, &signer);
            write_u64_storage(
                PERP_PENDING_ORACLE_SIGNER_AT_KEY,
                env::block_timestamp() / 1_000_000 + PERP_ORACLE_SIGNER_DELAY_MS,
            );
        }
    }

    pub fn apply_oracle_signer(&mut self) {
        assert_protocol_admin();
        let ready_at = read_u64_storage(PERP_PENDING_ORACLE_SIGNER_AT_KEY);
        assert!(ready_at > 0, "no pending oracle signer");
        assert!(
            env::block_timestamp() / 1_000_000 >= ready_at,
            "config delay active"
        );
        let signer: [u8; 20] = read_json_storage(PERP_PENDING_ORACLE_SIGNER_KEY)
            .expect("pending oracle signer missing");
        write_json_storage(PERP_ORACLE_SIGNER_KEY, &signer);
        env::storage_remove(PERP_PENDING_ORACLE_SIGNER_KEY);
        env::storage_remove(PERP_PENDING_ORACLE_SIGNER_AT_KEY);
    }

    /// Accept an oracle attestation signed by the configured Ethereum EOA.
    /// The legacy fixed test publisher remains testnet-only for compatibility.
    pub fn publish_market_price_attestation(
        &mut self,
        attestation: OracleAttestation,
        signature: Base64VecU8,
    ) {
        assert_valid_market_id(&attestation.market_id);
        let market = read_perp_market(&attestation.market_id).expect("oracle market is not listed");
        assert!(
            attestation.price.0 > 0 && attestation.spot_price.0 > 0,
            "price must be positive"
        );
        assert!(
            attestation.confidence_bps <= market.max_oracle_confidence_bps,
            "oracle confidence too high"
        );
        assert!(
            attestation.source_round.len() > 0 && attestation.source_round.len() <= 128,
            "invalid source round"
        );
        let now = env::block_timestamp() / 1_000_000;
        assert!(
            attestation.observed_at_ms <= now
                && now.saturating_sub(attestation.observed_at_ms) <= MAX_MARKET_PRICE_AGE_MS,
            "price is stale or future dated"
        );
        assert!(
            attestation.expires_at_ms >= now
                && attestation.expires_at_ms <= now.saturating_add(MAX_AGENT_ACTION_TTL_MS),
            "attestation expired or too long-lived"
        );
        let old = read_market_price(&attestation.market_id);
        assert!(
            old.as_ref()
                .is_none_or(|v| attestation.sequence > v.sequence),
            "oracle sequence must increase"
        );
        let signer: [u8; 20] =
            read_json_storage(PERP_ORACLE_SIGNER_KEY).expect("oracle signer is not configured");
        let message = build_oracle_attestation_message(&attestation);
        assert!(
            verify_eip191_signature(&message, signature.as_ref(), &signer),
            "oracle EOA signature invalid"
        );
        let deviation = attestation
            .spot_price
            .0
            .abs_diff(attestation.price.0)
            .saturating_mul(10_000)
            / attestation.price.0;
        assert!(deviation <= 50, "spot and TWAP diverged beyond hard bound");
        let mut health = OracleHealth {
            circuit_broken: false,
            confidence_bps: attestation.confidence_bps,
            spot_twap_deviation_bps: deviation.min(u128::from(u16::MAX)) as u16,
        };
        let next_price = MarketPrice {
            market_id: attestation.market_id.clone(),
            price: attestation.price,
            observed_at_ms: attestation.observed_at_ms,
            sequence: attestation.sequence,
        };
        let storage_before = env::storage_usage();

        // A >20% move is held for a second signed observation. The confirming
        // attestation must agree with the staged candidate before the mark
        // moves, while still allowing a genuinely shifted market to recover.
        let pending_jump_key = raw_key(PERP_ORACLE_PENDING_JUMP_PREFIX, &[&attestation.market_id]);
        if let Some(pending) = read_json_storage::<PendingOracleJump>(&pending_jump_key) {
            let confirmation_deviation = pending
                .price
                .price
                .0
                .abs_diff(attestation.price.0)
                .saturating_mul(10_000)
                / pending.price.price.0;
            if confirmation_deviation <= PERP_ORACLE_RECOVERY_DEVIATION_BPS {
                if attestation.source_round != pending.source_round
                    && now
                        >= pending
                            .staged_at_ms
                            .saturating_add(PERP_ORACLE_JUMP_CONFIRM_DELAY_MS)
                {
                    write_market_price(&next_price);
                    write_oracle_health(&attestation.market_id, &health);
                    env::storage_remove(&pending_jump_key);
                    settle_storage_credit("protocol:market_prices", storage_before);
                    return;
                }
                settle_storage_credit("protocol:market_prices", storage_before);
                return;
            }
        }
        if let Some(previous) = old.as_ref() {
            let jump = attestation
                .price
                .0
                .abs_diff(previous.price.0)
                .saturating_mul(10_000)
                / previous.price.0;
            if jump > 2_000 {
                health.circuit_broken = true;
                write_oracle_health(&attestation.market_id, &health);
                write_market_price(&MarketPrice {
                    sequence: attestation.sequence,
                    ..previous.clone()
                });
                write_json_storage(
                    &pending_jump_key,
                    &PendingOracleJump {
                        price: next_price,
                        staged_at_ms: now,
                        source_round: attestation.source_round,
                    },
                );
                settle_storage_credit("protocol:market_prices", storage_before);
                return;
            }
        }
        write_market_price(&next_price);
        write_oracle_health(&attestation.market_id, &health);
        env::storage_remove(&pending_jump_key);
        settle_storage_credit("protocol:market_prices", storage_before);
    }

    pub fn perp_total_margin(&self) -> U128 {
        U128(read_u128_storage(PERP_TOTAL_MARGIN_KEY))
    }

    pub fn perp_bad_debt(&self) -> U128 {
        U128(read_u128_storage(PERP_BAD_DEBT_KEY))
    }

    pub fn perp_insurance(&self) -> U128 {
        U128(read_u128_storage(PERP_INSURANCE_KEY))
    }

    pub fn bad_debt_cover_message(
        &self,
        account_id: String,
        amount: U128,
        nonce: u64,
        expires_at_ms: u64,
    ) -> String {
        build_bad_debt_cover_message(&account_id, amount, nonce, expires_at_ms)
    }

    /// Owner-authorized recapitalization. Repayment cannot exceed recorded bad
    /// debt, and any remaining debt continues to keep opens paused.
    pub fn cover_perp_bad_debt(
        &mut self,
        account_id: String,
        amount: U128,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) -> U128 {
        assert!(amount.0 > 0, "cover amount must be positive");
        let debt = read_u128_storage(PERP_BAD_DEBT_KEY);
        assert!(
            debt > 0 && amount.0 <= debt,
            "cover exceeds outstanding bad debt"
        );
        let expected_nonce = read_user_perp_nonce(&account_id)
            .checked_add(1)
            .expect("trading nonce overflow");
        assert_eq!(nonce, expected_nonce, "invalid trading nonce");
        assert_valid_short_trade_expiry(expires_at_ms);
        let message = build_bad_debt_cover_message(&account_id, amount, nonce, expires_at_ms);
        let owner = read_owner(&account_id).expect("owner is not registered");
        assert!(
            verify_owner_signature(&owner, &message, signature.as_ref(), nonce),
            "invalid debt cover signature"
        );
        let balance = self.balances.get(&account_id).unwrap_or_default();
        assert!(balance >= amount.0, "insufficient free USDC");
        let vault_id = SHARED_VAULT_RECORD_ID.to_string();
        let vault = self.balances.get(&vault_id).unwrap_or_default();
        let storage_before = env::storage_usage();
        self.balances.insert(&account_id, &(balance - amount.0));
        self.balances.insert(
            &vault_id,
            &vault.checked_add(amount.0).expect("vault overflow"),
        );
        write_u128_storage(PERP_BAD_DEBT_KEY, debt - amount.0);
        write_user_perp_nonce(&account_id, nonce);
        settle_storage_credit(&account_id, storage_before);
        U128(amount.0)
    }

    /// Only the NEAR account selected at A1 initialization can change risk
    /// configuration. Increasing risk requires the configured timelock.
    pub fn configure_perp_market(&mut self, config: PerpMarketConfig) {
        assert_protocol_admin();
        assert_valid_market_id(&config.market_id);
        assert!(
            config.max_leverage_bps >= 10_000 && config.max_leverage_bps <= MAX_PERP_LEVERAGE_BPS,
            "invalid leverage"
        );
        assert!(
            config.maintenance_margin_bps > 0 && config.maintenance_margin_bps < 10_000,
            "invalid maintenance margin"
        );
        assert!(
            config.max_execution_deviation_bps <= MAX_PERP_SLIPPAGE_BPS,
            "execution deviation too high"
        );
        assert!(
            config.max_oracle_confidence_bps <= 1_000,
            "oracle confidence cap exceeds hard bound"
        );
        assert!(
            config.max_position_usdc.0 > 0 && config.max_position_usdc.0 <= MAX_PERP_POSITION_USDC,
            "invalid position cap"
        );
        assert!(
            config.max_market_oi_bps > 0 && config.max_market_oi_bps <= PERP_MAX_MARKET_OI_BPS,
            "market OI cap exceeds protocol bound"
        );
        assert!(
            config.max_market_skew_bps > 0
                && config.max_market_skew_bps <= PERP_MAX_MARKET_SKEW_BPS,
            "market skew cap exceeds protocol bound"
        );
        let pending_key = perp_market_key(PERP_PENDING_MARKET_PREFIX, &config.market_id);
        // Pending risk is never an active baseline. Re-submitting a pending
        // config must not make its own risk increase appear already applied.
        let current_config = read_perp_market(&config.market_id);
        let increases_risk = match current_config {
            Some(current) => {
                (config.enabled && !current.enabled)
                    || (config.max_leverage_bps > current.max_leverage_bps)
                    || (config.maintenance_margin_bps < current.maintenance_margin_bps)
                    || (config.max_execution_deviation_bps > current.max_execution_deviation_bps)
                    || (config.max_oracle_confidence_bps > current.max_oracle_confidence_bps)
                    || (config.max_position_usdc.0 > current.max_position_usdc.0)
                    || (config.max_market_oi_bps > current.max_market_oi_bps)
                    || (config.max_market_skew_bps > current.max_market_skew_bps)
                    || (current.close_only && !config.close_only)
            }
            // A market that does not exist yet has no active config to compare
            // against, so its first enabled config always waits for the delay.
            None => config.enabled,
        };
        if increases_risk {
            write_json_storage(&pending_key, &config);
            write_u64_storage(
                &perp_market_time_key(&config.market_id),
                env::block_timestamp() / 1_000_000 + PERP_MARKET_RISK_DELAY_MS,
            );
            return;
        }
        add_perp_market_id(&config.market_id);
        write_perp_market(&config);
        env::storage_remove(&pending_key);
        env::storage_remove(&perp_market_time_key(&config.market_id));
    }

    pub fn apply_perp_market_config(&mut self, market_id: String) {
        assert_protocol_admin();
        let at = read_u64_storage(&perp_market_time_key(&market_id));
        assert!(at > 0, "no pending market config");
        assert!(
            env::block_timestamp() / 1_000_000 >= at,
            "config delay active"
        );
        let config: PerpMarketConfig =
            read_json_storage(&perp_market_key(PERP_PENDING_MARKET_PREFIX, &market_id))
                .expect("pending config missing");
        add_perp_market_id(&market_id);
        write_perp_market(&config);
        env::storage_remove(&perp_market_key(PERP_PENDING_MARKET_PREFIX, &market_id));
        env::storage_remove(&perp_market_time_key(&market_id));
    }

    pub fn pause_perps(&mut self) {
        assert_protocol_admin();
        write_u64_storage(PERP_PAUSED_KEY, 1);
    }

    pub fn unpause_perps(&mut self) {
        assert_protocol_admin();
        assert_eq!(
            read_u128_storage(PERP_BAD_DEBT_KEY),
            0,
            "bad debt must be covered before unpausing"
        );
        write_u64_storage(PERP_PAUSED_KEY, 0);
    }

    /// LPs receive internal shares; funds remain in A1's shared contract.
    /// This is the account-model equivalent of Jaguar's separate instant vault.
    pub fn deposit_vault(
        &mut self,
        account_id: String,
        amount: U128,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) -> U128 {
        assert!(amount.0 > 0, "deposit must be positive");
        assert!(vault_prices_fresh(), "oracle prices stale");
        let owner = read_owner(&account_id).expect("owner is not registered");
        assert_next_user_action(
            &account_id,
            nonce,
            expires_at_ms,
            "deposit_vault",
            &amount.0.to_string(),
            signature.as_ref(),
            &owner,
        );
        let bal = self.balances.get(&account_id).unwrap_or_default();
        assert!(bal >= amount.0, "insufficient free balance");
        let vault_cash = self
            .balances
            .get(&SHARED_VAULT_RECORD_ID.to_string())
            .unwrap_or_default();
        let cap = if read_u128_storage(PERP_BETA_CAP_KEY) == 0 {
            PERP_DEFAULT_BETA_CAP_USDC
        } else {
            read_u128_storage(PERP_BETA_CAP_KEY)
        };
        assert!(
            vault_cash
                .checked_add(amount.0)
                .is_some_and(|total| total <= cap),
            "vault beta cap exceeded"
        );
        let nav = vault_nav(&self.balances);
        let mut total_shares = read_u128_storage(LP_TOTAL_SHARES_KEY);
        assert!(
            total_shares == 0 || nav > 0,
            "vault NAV is zero while LP shares remain outstanding"
        );
        if total_shares == 0 && nav > 0 {
            // Capital that entered outside LP minting (for example debt
            // recapitalization before the first LP) is permanently locked to
            // the protocol record so the first LP cannot claim it.
            write_lp_shares(SHARED_VAULT_RECORD_ID, nav);
            total_shares = nav;
            write_u128_storage(LP_TOTAL_SHARES_KEY, total_shares);
        }
        let shares = if total_shares == 0 || nav == 0 {
            amount.0
        } else {
            amount.0.saturating_mul(total_shares) / nav
        };
        assert!(shares > 0, "deposit mints zero shares");
        let storage_before = env::storage_usage();
        self.balances.insert(&account_id, &(bal - amount.0));
        let vault = self
            .balances
            .get(&SHARED_VAULT_RECORD_ID.to_string())
            .unwrap_or_default();
        self.balances.insert(
            &SHARED_VAULT_RECORD_ID.to_string(),
            &vault.checked_add(amount.0).expect("vault overflow"),
        );
        write_lp_shares(
            &account_id,
            read_lp_shares(&account_id)
                .checked_add(shares)
                .expect("shares overflow"),
        );
        write_u128_storage(
            LP_TOTAL_SHARES_KEY,
            total_shares.checked_add(shares).expect("shares overflow"),
        );
        settle_storage_credit(&account_id, storage_before);
        write_user_perp_nonce(&account_id, nonce);
        U128(shares)
    }

    pub fn lp_shares_of(&self, account_id: String) -> U128 {
        U128(read_lp_shares(&account_id))
    }

    pub fn vault_nav_usdc(&self) -> U128 {
        U128(vault_nav(&self.balances))
    }

    pub fn perp_margin_message(
        &self,
        account_id: String,
        position_id: String,
        action: String,
        amount: U128,
        nonce: u64,
        expires_at_ms: u64,
    ) -> String {
        let position =
            read_perp_position(&account_id, &position_id).expect("position does not exist");
        build_perp_margin_message(&position, &action, amount, nonce, expires_at_ms)
    }

    pub fn add_perp_margin(
        &mut self,
        account_id: String,
        position_id: String,
        amount: U128,
        nonce: u64,
        expires_at_ms: u64,
        owner_signature: Base64VecU8,
    ) -> U128 {
        assert!(amount.0 > 0, "margin amount must be positive");
        let mut position =
            read_perp_position(&account_id, &position_id).expect("position does not exist");
        let owner_record = position_owner_record(&account_id);
        let expected = read_user_perp_nonce(&owner_record)
            .checked_add(1)
            .expect("nonce overflow");
        assert_eq!(nonce, expected, "invalid owner trading nonce");
        assert_valid_short_trade_expiry(expires_at_ms);
        let message = build_perp_margin_message(&position, "add", amount, nonce, expires_at_ms);
        let owner = read_owner(&owner_record).expect("owner is not registered");
        assert!(
            verify_owner_signature(&owner, &message, owner_signature.as_ref(), nonce),
            "invalid margin authorization"
        );
        let config = read_agent_config(&account_id);
        let source = if config.as_ref().is_some_and(|agent| !agent.enabled) {
            owner_record.clone()
        } else {
            account_id.clone()
        };
        let balance = self.balances.get(&source).unwrap_or_default();
        assert!(balance >= amount.0, "insufficient free USDC");
        let storage_before = env::storage_usage();
        position.margin_usdc = U128(
            position
                .margin_usdc
                .0
                .checked_add(amount.0)
                .expect("margin overflow"),
        );
        if let Some(agent) = &config {
            if agent.enabled {
                assert!(
                    read_agent_reserved_margin(&account_id)
                        .checked_add(amount.0)
                        .is_some_and(|v| v <= agent.max_allocation_usdc.0),
                    "agent allocation limit exceeded"
                );
            }
            write_agent_reserved_margin(
                &account_id,
                read_agent_reserved_margin(&account_id)
                    .checked_add(amount.0)
                    .expect("agent margin overflow"),
            );
        }
        self.balances.insert(&source, &(balance - amount.0));
        write_u128_storage(
            PERP_TOTAL_MARGIN_KEY,
            read_u128_storage(PERP_TOTAL_MARGIN_KEY)
                .checked_add(amount.0)
                .expect("total margin overflow"),
        );
        write_perp_position(&account_id, &position_id, &position);
        write_user_perp_nonce(&owner_record, nonce);
        settle_storage_credit(&owner_record, storage_before);
        U128(amount.0)
    }

    pub fn remove_perp_margin(
        &mut self,
        account_id: String,
        position_id: String,
        amount: U128,
        nonce: u64,
        expires_at_ms: u64,
        owner_signature: Base64VecU8,
    ) -> U128 {
        assert!(amount.0 > 0, "margin amount must be positive");
        let mut position =
            read_perp_position(&account_id, &position_id).expect("position does not exist");
        assert!(
            position.margin_usdc.0 >= amount.0,
            "insufficient position margin"
        );
        let owner_record = position_owner_record(&account_id);
        let expected = read_user_perp_nonce(&owner_record)
            .checked_add(1)
            .expect("nonce overflow");
        assert_eq!(nonce, expected, "invalid owner trading nonce");
        assert_valid_short_trade_expiry(expires_at_ms);
        let message = build_perp_margin_message(&position, "remove", amount, nonce, expires_at_ms);
        let owner = read_owner(&owner_record).expect("owner is not registered");
        assert!(
            verify_owner_signature(&owner, &message, owner_signature.as_ref(), nonce),
            "invalid margin authorization"
        );
        let storage_before = env::storage_usage();
        settle_position_funding(&mut position, &mut self.balances);
        let mark = require_healthy_market_price(&position.market_id);
        let market = read_perp_market(&position.market_id).expect("market not listed");
        let pnl = crate::engine_math::linear_pnl(
            position.notional_usdc.0,
            position.entry_price.0,
            mark.price.0,
            position.is_long,
        )
        .expect("PnL overflow");
        let next_margin = position.margin_usdc.0 - amount.0;
        let base_units = perp_base_units(position.notional_usdc.0, position.entry_price.0);
        let current_notional = revalue_exposure(base_units, mark.price.0);
        assert!(
            !crate::engine_math::is_liquidatable(
                next_margin,
                pnl,
                current_notional,
                market.maintenance_margin_bps
            )
            .expect("maintenance overflow"),
            "margin removal would breach maintenance"
        );
        let config = read_agent_config(&account_id);
        let destination = if config.as_ref().is_some_and(|agent| !agent.enabled) {
            owner_record.clone()
        } else {
            account_id.clone()
        };
        let balance = self.balances.get(&destination).unwrap_or_default();
        self.balances.insert(
            &destination,
            &balance.checked_add(amount.0).expect("balance overflow"),
        );
        position.margin_usdc = U128(next_margin);
        write_perp_position(&account_id, &position_id, &position);
        write_u128_storage(
            PERP_TOTAL_MARGIN_KEY,
            read_u128_storage(PERP_TOTAL_MARGIN_KEY).saturating_sub(amount.0),
        );
        if config.is_some() {
            write_agent_reserved_margin(
                &account_id,
                read_agent_reserved_margin(&account_id).saturating_sub(amount.0),
            );
        }
        write_user_perp_nonce(&owner_record, nonce);
        settle_storage_credit(&owner_record, storage_before);
        U128(amount.0)
    }

    pub fn request_vault_withdrawal(
        &mut self,
        account_id: String,
        shares: U128,
        min_amount_usdc: U128,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) {
        assert!(shares.0 > 0, "shares must be positive");
        let owner = read_owner(&account_id).expect("owner is not registered");
        assert_next_user_action(
            &account_id,
            nonce,
            expires_at_ms,
            "request_vault_withdrawal",
            &format!("{}:{}", shares.0, min_amount_usdc.0),
            signature.as_ref(),
            &owner,
        );
        assert!(
            read_lp_shares(&account_id) >= shares.0,
            "insufficient LP shares"
        );
        assert!(
            read_lp_withdrawal(&account_id).is_none(),
            "withdrawal request already exists"
        );
        let storage_before = env::storage_usage();
        write_lp_withdrawal(
            &account_id,
            &LpWithdrawalRequest {
                shares,
                min_amount_usdc,
                requested_at_ms: env::block_timestamp() / 1_000_000,
            },
        );
        settle_storage_credit(&account_id, storage_before);
        write_user_perp_nonce(&account_id, nonce);
    }

    pub fn cancel_vault_withdrawal(
        &mut self,
        account_id: String,
        nonce: u64,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) {
        assert!(
            read_lp_withdrawal(&account_id).is_some(),
            "no withdrawal request"
        );
        let owner = read_owner(&account_id).expect("owner is not registered");
        assert_next_user_action(
            &account_id,
            nonce,
            expires_at_ms,
            "cancel_vault_withdrawal",
            "",
            signature.as_ref(),
            &owner,
        );
        let storage_before = env::storage_usage();
        delete_lp_withdrawal(&account_id);
        settle_storage_credit(&account_id, storage_before);
        write_user_perp_nonce(&account_id, nonce);
    }

    /// Anyone may finalize a matured LP request, but proceeds return to the
    /// LP's own logical balance and cannot be redirected by the caller.
    pub fn complete_vault_withdrawal(&mut self, account_id: String) -> U128 {
        let request = read_lp_withdrawal(&account_id).expect("no withdrawal request");
        let now = env::block_timestamp() / 1_000_000;
        assert!(
            now >= request
                .requested_at_ms
                .saturating_add(LP_WITHDRAWAL_COOLDOWN_MS),
            "withdrawal cooldown active"
        );
        assert!(vault_prices_fresh(), "oracle prices stale");
        let total_shares = read_u128_storage(LP_TOTAL_SHARES_KEY);
        assert!(total_shares > 0, "no LP shares");
        let amount = request.shares.0.saturating_mul(vault_nav(&self.balances)) / total_shares;
        assert!(amount > 0, "withdrawal output is zero");
        assert!(
            amount >= request.min_amount_usdc.0,
            "withdrawal below minimum output"
        );
        let cash = self
            .balances
            .get(&SHARED_VAULT_RECORD_ID.to_string())
            .unwrap_or_default();
        assert!(
            amount <= cash && amount <= vault_risk_available(&self.balances),
            "insufficient available vault liquidity"
        );
        assert!(
            read_lp_shares(&account_id) >= request.shares.0,
            "LP share invariant failed"
        );
        let lp_balance = self.balances.get(&account_id).unwrap_or_default();
        let storage_before = env::storage_usage();
        self.balances
            .insert(&SHARED_VAULT_RECORD_ID.to_string(), &(cash - amount));
        self.balances.insert(
            &account_id,
            &lp_balance.checked_add(amount).expect("LP balance overflow"),
        );
        write_lp_shares(&account_id, read_lp_shares(&account_id) - request.shares.0);
        write_u128_storage(LP_TOTAL_SHARES_KEY, total_shares - request.shares.0);
        delete_lp_withdrawal(&account_id);
        settle_storage_credit(&account_id, storage_before);
        U128(amount)
    }

    pub fn close_perp_position_message(
        &self,
        account_id: String,
        position_id: String,
        reference_price: U128,
        max_slippage_bps: u16,
        price_sequence: u64,
        fee_bps: u16,
        nonce: u64,
        expires_at_ms: u64,
    ) -> String {
        let position =
            read_perp_position(&account_id, &position_id).expect("position does not exist");
        perp_close_message(
            &position,
            reference_price,
            max_slippage_bps,
            price_sequence,
            fee_bps,
            nonce,
            expires_at_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn reduce_perp_position_message(
        &self,
        account_id: String,
        position_id: String,
        reduce_notional: U128,
        reference_price: U128,
        max_slippage_bps: u16,
        price_sequence: u64,
        fee_bps: u16,
        nonce: u64,
        expires_at_ms: u64,
    ) -> String {
        let position =
            read_perp_position(&account_id, &position_id).expect("position does not exist");
        perp_reduce_message(
            &position,
            reduce_notional,
            reference_price,
            max_slippage_bps,
            price_sequence,
            fee_bps,
            nonce,
            expires_at_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn reduce_perp_position(
        &mut self,
        account_id: String,
        position_id: String,
        reduce_notional: U128,
        reference_price: U128,
        max_slippage_bps: u16,
        price_sequence: u64,
        fee_bps: u16,
        nonce: u64,
        expires_at_ms: u64,
        owner_signature: Base64VecU8,
        quote_signature: Base64VecU8,
    ) -> U128 {
        let mut position =
            read_perp_position(&account_id, &position_id).expect("position does not exist");
        assert!(
            reduce_notional.0 > 0 && reduce_notional.0 < position.notional_usdc.0,
            "partial close amount out of range"
        );
        let market = read_perp_market(&position.market_id).expect("market not listed");
        assert!(
            fee_bps <= MAX_PERP_FEE_BPS && max_slippage_bps <= MAX_PERP_SLIPPAGE_BPS,
            "fee or slippage exceeds hard bound"
        );
        let mark = require_healthy_market_price(&position.market_id);
        assert_eq!(mark.sequence, price_sequence, "stale quote sequence");
        assert!(reference_price.0 > 0, "reference price must be positive");
        let deviation = mark
            .price
            .0
            .abs_diff(reference_price.0)
            .saturating_mul(10_000)
            / reference_price.0;
        assert!(
            deviation <= u128::from(max_slippage_bps.min(market.max_execution_deviation_bps)),
            "execution price exceeds signed slippage"
        );
        let agent_config = read_agent_config(&account_id);
        let owner_record = position_owner_record(&account_id);
        let owner_nonce = read_user_perp_nonce(&owner_record)
            .checked_add(1)
            .expect("trading nonce overflow");
        let agent_nonce = read_agent_action_nonce(&account_id)
            .checked_add(1)
            .expect("agent action nonce overflow");
        assert_valid_short_trade_expiry(expires_at_ms);
        let message = perp_reduce_message(
            &position,
            reduce_notional,
            reference_price,
            max_slippage_bps,
            price_sequence,
            fee_bps,
            nonce,
            expires_at_ms,
        );
        let owner_signed = agent_config
            .as_ref()
            .and_then(|config| read_owner(&config.user_record_id))
            .or_else(|| {
                if agent_config.is_none() {
                    read_owner(&account_id)
                } else {
                    None
                }
            })
            .is_some_and(|owner| {
                nonce == owner_nonce
                    && verify_owner_signature(&owner, &message, owner_signature.as_ref(), nonce)
            });
        let agent_signed = if let Some(config) = &agent_config {
            let now = env::block_timestamp() / 1_000_000;
            nonce == agent_nonce
                && config.enabled
                && now < config.expires_at_ms
                && expires_at_ms <= config.expires_at_ms
                && verify_owner_signature(
                    &config.agent_key,
                    &message,
                    owner_signature.as_ref(),
                    nonce,
                )
        } else {
            false
        };
        assert!(owner_signed || agent_signed, "invalid reduce authorization");
        let emergency_owner_reduce = quote_signature.0.is_empty();
        if emergency_owner_reduce {
            assert!(owner_signed, "emergency reduction requires owner signature");
            assert_eq!(
                reference_price.0, mark.price.0,
                "emergency reduce must use mark"
            );
            assert_eq!(
                max_slippage_bps, 0,
                "emergency reduce slippage must be zero"
            );
        } else {
            let quote_signer = read_perp_quote_signer().expect("quote signer is not configured");
            assert!(
                verify_owner_signature(&quote_signer, &message, quote_signature.as_ref(), nonce),
                "invalid quote signature"
            );
        }
        // Capture before funding settlement or deferred-claim writes so every
        // new byte created by this action is charged to the logical owner.
        let storage_before = env::storage_usage();
        settle_position_funding(&mut position, &mut self.balances);
        let realized_pnl = crate::engine_math::linear_pnl(
            reduce_notional.0,
            position.entry_price.0,
            mark.price.0,
            position.is_long,
        )
        .expect("PnL overflow");
        let margin_part = position
            .margin_usdc
            .0
            .checked_mul(reduce_notional.0)
            .expect("margin split overflow")
            / position.notional_usdc.0;
        let fee = crate::engine_math::fee_ceil(reduce_notional.0, fee_bps).expect("fee overflow");
        let (loss, fee_collected, margin_credit) =
            perp_settlement_components(margin_part, realized_pnl, fee);
        if agent_config.is_some() {
            record_agent_realized_loss(&account_id, loss.saturating_add(fee_collected));
        }
        let vault_id = SHARED_VAULT_RECORD_ID.to_string();
        let vault_cash = self.balances.get(&vault_id).unwrap_or_default();
        let deferred_beneficiary_id = agent_config
            .as_ref()
            .map(|config| config.user_record_id.clone())
            .unwrap_or_else(|| account_id.clone());
        let credit_record_id = agent_config
            .as_ref()
            .filter(|config| config.enabled)
            .map(|_| account_id.clone())
            .unwrap_or_else(|| deferred_beneficiary_id.clone());
        let mut credit = margin_credit;
        if realized_pnl > 0 {
            let free = vault_cash
                .saturating_sub(read_u128_storage(PERP_INSURANCE_KEY))
                .saturating_sub(read_u128_storage(PERP_DEFERRED_TOTAL_KEY))
                .saturating_sub(read_vault_reserved_payout().saturating_sub(reduce_notional.0));
            let paid = (realized_pnl as u128).min(free);
            credit = margin_credit.saturating_add(paid);
            let unpaid = (realized_pnl as u128).saturating_sub(paid);
            if unpaid > 0 {
                write_perp_deferred(
                    &deferred_beneficiary_id,
                    read_perp_deferred(&deferred_beneficiary_id)
                        .checked_add(unpaid)
                        .expect("claim overflow"),
                );
                write_u128_storage(
                    PERP_DEFERRED_TOTAL_KEY,
                    read_u128_storage(PERP_DEFERRED_TOTAL_KEY)
                        .checked_add(unpaid)
                        .expect("claim overflow"),
                );
            }
            self.balances.insert(
                &vault_id,
                &vault_cash
                    .saturating_add(fee_collected)
                    .saturating_sub(paid),
            );
        } else {
            self.balances.insert(
                &vault_id,
                &vault_cash
                    .checked_add(loss)
                    .and_then(|v| v.checked_add(fee_collected))
                    .expect("vault overflow"),
            );
        }
        // A trader's loss is capped at isolated margin. Loss beyond that
        // margin is not a vault liability and must not consume LP insurance.
        if fee_collected > 0 {
            write_u128_storage(
                PERP_INSURANCE_KEY,
                read_u128_storage(PERP_INSURANCE_KEY)
                    .checked_add(fee_collected / 4)
                    .expect("insurance overflow"),
            );
        }
        let exposure = read_perp_exposure(&position.market_id);
        position.funding_remainder = position.funding_remainder.saturating_mul(
            i128::try_from(position.notional_usdc.0 - reduce_notional.0)
                .expect("position size overflow"),
        ) / i128::try_from(position.notional_usdc.0)
            .expect("position size overflow");
        if position.is_long {
            position.notional_usdc = U128(position.notional_usdc.0 - reduce_notional.0);
            position.margin_usdc = U128(position.margin_usdc.0 - margin_part);
            let mut next = exposure;
            next.long_notional_usdc = U128(
                next.long_notional_usdc
                    .0
                    .checked_sub(reduce_notional.0)
                    .expect("long exposure invariant"),
            );
            next.long_base_units = U128(
                next.long_base_units
                    .0
                    .checked_sub(perp_base_units(reduce_notional.0, position.entry_price.0))
                    .expect("long base exposure invariant"),
            );
            write_perp_exposure(&next, &position.market_id);
        } else {
            position.notional_usdc = U128(position.notional_usdc.0 - reduce_notional.0);
            position.margin_usdc = U128(position.margin_usdc.0 - margin_part);
            let mut next = exposure;
            next.short_notional_usdc = U128(
                next.short_notional_usdc
                    .0
                    .checked_sub(reduce_notional.0)
                    .expect("short exposure invariant"),
            );
            next.short_base_units = U128(
                next.short_base_units
                    .0
                    .checked_sub(perp_base_units(reduce_notional.0, position.entry_price.0))
                    .expect("short base exposure invariant"),
            );
            write_perp_exposure(&next, &position.market_id);
        }
        if credit > 0 {
            let balance = self.balances.get(&credit_record_id).unwrap_or_default();
            self.balances.insert(
                &credit_record_id,
                &balance.checked_add(credit).expect("balance overflow"),
            );
        }
        write_vault_reserved_payout(
            read_vault_reserved_payout()
                .checked_sub(reduce_notional.0)
                .expect("vault reservation invariant"),
        );
        write_u128_storage(
            PERP_TOTAL_MARGIN_KEY,
            read_u128_storage(PERP_TOTAL_MARGIN_KEY).saturating_sub(margin_part),
        );
        write_u128_storage(
            PERP_TOTAL_OI_KEY,
            read_u128_storage(PERP_TOTAL_OI_KEY).saturating_sub(reduce_notional.0),
        );
        let net_delta = if position.is_long {
            reduce_notional.0 as i128
        } else {
            -(reduce_notional.0 as i128)
        };
        write_i128_storage(
            PERP_GLOBAL_NET_KEY,
            read_i128_storage(PERP_GLOBAL_NET_KEY).saturating_sub(net_delta),
        );
        write_perp_open_notional(
            &account_id,
            read_perp_open_notional(&account_id).saturating_sub(reduce_notional.0),
        );
        if let Some(config) = &agent_config {
            if agent_signed {
                write_agent_action_nonce(&account_id, nonce);
            } else {
                write_user_perp_nonce(&owner_record, nonce);
            }
            write_agent_open_notional(
                &account_id,
                read_agent_open_notional(&account_id).saturating_sub(reduce_notional.0),
            );
            write_agent_reserved_margin(
                &account_id,
                read_agent_reserved_margin(&account_id).saturating_sub(margin_part),
            );
            settle_storage_credit(&config.user_record_id, storage_before);
        } else {
            write_user_perp_nonce(&account_id, nonce);
            settle_storage_credit(&account_id, storage_before);
        }
        write_perp_position(&account_id, &position_id, &position);
        U128(credit)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn close_perp_position(
        &mut self,
        account_id: String,
        position_id: String,
        reference_price: U128,
        max_slippage_bps: u16,
        price_sequence: u64,
        fee_bps: u16,
        nonce: u64,
        expires_at_ms: u64,
        owner_signature: Base64VecU8,
        quote_signature: Base64VecU8,
    ) -> U128 {
        let mut position =
            read_perp_position(&account_id, &position_id).expect("position does not exist");
        let market = read_perp_market(&position.market_id).expect("market not listed");
        assert!(
            fee_bps <= MAX_PERP_FEE_BPS && max_slippage_bps <= MAX_PERP_SLIPPAGE_BPS,
            "fee or slippage exceeds hard bound"
        );
        let mark = require_healthy_market_price(&position.market_id);
        assert_eq!(mark.sequence, price_sequence, "stale quote sequence");
        // PnL settlement is blocked while the oracle mark is unsafe.
        assert!(reference_price.0 > 0, "reference price must be positive");
        let deviation = mark
            .price
            .0
            .abs_diff(reference_price.0)
            .saturating_mul(10_000)
            / reference_price.0;
        assert!(
            deviation <= u128::from(max_slippage_bps.min(market.max_execution_deviation_bps)),
            "execution price exceeds signed slippage"
        );
        let agent_config = read_agent_config(&account_id);
        let owner_record = position_owner_record(&account_id);
        let owner_nonce = read_user_perp_nonce(&owner_record)
            .checked_add(1)
            .expect("trading nonce overflow");
        let agent_nonce = read_agent_action_nonce(&account_id)
            .checked_add(1)
            .expect("agent action nonce overflow");
        assert_valid_short_trade_expiry(expires_at_ms);
        let message = perp_close_message(
            &position,
            reference_price,
            max_slippage_bps,
            price_sequence,
            fee_bps,
            nonce,
            expires_at_ms,
        );
        let owner_signed = agent_config
            .as_ref()
            .and_then(|config| read_owner(&config.user_record_id))
            .or_else(|| {
                if agent_config.is_none() {
                    read_owner(&account_id)
                } else {
                    None
                }
            })
            .is_some_and(|owner| {
                nonce == owner_nonce
                    && verify_owner_signature(&owner, &message, owner_signature.as_ref(), nonce)
            });
        let agent_signed = if let Some(config) = &agent_config {
            let now = env::block_timestamp() / 1_000_000;
            nonce == agent_nonce
                && config.enabled
                && now < config.expires_at_ms
                && expires_at_ms <= config.expires_at_ms
                && verify_owner_signature(
                    &config.agent_key,
                    &message,
                    owner_signature.as_ref(),
                    nonce,
                )
        } else {
            false
        };
        assert!(owner_signed || agent_signed, "invalid close authorization");
        let emergency_owner_close = quote_signature.0.is_empty();
        if emergency_owner_close {
            assert!(owner_signed, "emergency close requires owner signature");
            assert_eq!(
                reference_price.0, mark.price.0,
                "emergency close must use mark"
            );
            assert_eq!(max_slippage_bps, 0, "emergency close slippage must be zero");
        } else {
            let signer = read_perp_quote_signer().expect("quote signer is not configured");
            assert!(
                verify_owner_signature(&signer, &message, quote_signature.as_ref(), nonce),
                "invalid quote signature"
            );
        }
        // Funding may create balance state, and an underfunded vault may
        // create a deferred-profit claim below. Meter the whole operation.
        let storage_before = env::storage_usage();
        settle_position_funding(&mut position, &mut self.balances);
        let pnl = crate::engine_math::linear_pnl(
            position.notional_usdc.0,
            position.entry_price.0,
            mark.price.0,
            position.is_long,
        )
        .expect("PnL overflow");
        let close_fee =
            crate::engine_math::fee_ceil(position.notional_usdc.0, fee_bps).expect("fee overflow");
        let (loss, fee_collected, margin_credit) =
            perp_settlement_components(position.margin_usdc.0, pnl, close_fee);
        if agent_config.is_some() {
            record_agent_realized_loss(&account_id, loss.saturating_add(fee_collected));
        }
        let vault_id = SHARED_VAULT_RECORD_ID.to_string();
        let vault_balance = self.balances.get(&vault_id).unwrap_or_default();
        let deferred_beneficiary_id = agent_config
            .as_ref()
            .map(|config| config.user_record_id.clone())
            .unwrap_or_else(|| account_id.clone());
        let credit_record_id = agent_config
            .as_ref()
            .filter(|config| config.enabled)
            .map(|_| account_id.clone())
            .unwrap_or_else(|| deferred_beneficiary_id.clone());
        let mut user_credit = margin_credit;
        if pnl > 0 {
            let free = vault_balance
                .saturating_sub(read_u128_storage(PERP_INSURANCE_KEY))
                .saturating_sub(read_u128_storage(PERP_DEFERRED_TOTAL_KEY))
                .saturating_sub(
                    read_vault_reserved_payout().saturating_sub(position.notional_usdc.0),
                );
            let paid = (pnl as u128).min(free);
            user_credit = position
                .margin_usdc
                .0
                .saturating_sub(fee_collected)
                .saturating_add(paid);
            let unpaid = (pnl as u128).saturating_sub(paid);
            if unpaid != 0 {
                write_perp_deferred(
                    &deferred_beneficiary_id,
                    read_perp_deferred(&deferred_beneficiary_id)
                        .checked_add(unpaid)
                        .expect("claim overflow"),
                );
                write_u128_storage(
                    PERP_DEFERRED_TOTAL_KEY,
                    read_u128_storage(PERP_DEFERRED_TOTAL_KEY)
                        .checked_add(unpaid)
                        .expect("claim overflow"),
                );
            }
            self.balances.insert(
                &vault_id,
                &vault_balance
                    .saturating_add(fee_collected)
                    .saturating_sub(paid),
            );
        } else {
            self.balances.insert(
                &vault_id,
                &vault_balance
                    .checked_add(loss)
                    .and_then(|v| v.checked_add(fee_collected))
                    .expect("vault overflow"),
            );
        }
        // Isolated trader losses cannot create vault bad debt; any loss above
        // margin is uncollectible and is not an LP or insurance shortfall.
        if fee_collected != 0 {
            let insurance_fee = fee_collected / 4;
            write_u128_storage(
                PERP_INSURANCE_KEY,
                read_u128_storage(PERP_INSURANCE_KEY)
                    .checked_add(insurance_fee)
                    .expect("insurance overflow"),
            );
        }
        if user_credit != 0 {
            let balance = self.balances.get(&credit_record_id).unwrap_or_default();
            self.balances.insert(
                &credit_record_id,
                &balance.checked_add(user_credit).expect("balance overflow"),
            );
        }
        let exposure = read_perp_exposure(&position.market_id);
        let base_units = perp_base_units(position.notional_usdc.0, position.entry_price.0);
        let mut next_exposure = exposure;
        if position.is_long {
            next_exposure.long_notional_usdc = U128(
                next_exposure
                    .long_notional_usdc
                    .0
                    .checked_sub(position.notional_usdc.0)
                    .expect("long exposure invariant"),
            );
            next_exposure.long_base_units =
                U128(next_exposure.long_base_units.0.saturating_sub(base_units));
        } else {
            next_exposure.short_notional_usdc = U128(
                next_exposure
                    .short_notional_usdc
                    .0
                    .checked_sub(position.notional_usdc.0)
                    .expect("short exposure invariant"),
            );
            next_exposure.short_base_units =
                U128(next_exposure.short_base_units.0.saturating_sub(base_units));
        }
        write_perp_exposure(&next_exposure, &position.market_id);
        write_vault_reserved_payout(
            read_vault_reserved_payout()
                .checked_sub(position.notional_usdc.0)
                .expect("vault reservation invariant"),
        );
        write_u128_storage(
            PERP_TOTAL_MARGIN_KEY,
            read_u128_storage(PERP_TOTAL_MARGIN_KEY).saturating_sub(position.margin_usdc.0),
        );
        write_u128_storage(
            PERP_TOTAL_OI_KEY,
            read_u128_storage(PERP_TOTAL_OI_KEY).saturating_sub(position.notional_usdc.0),
        );
        let net_delta = if position.is_long {
            position.notional_usdc.0 as i128
        } else {
            -(position.notional_usdc.0 as i128)
        };
        write_i128_storage(
            PERP_GLOBAL_NET_KEY,
            read_i128_storage(PERP_GLOBAL_NET_KEY).saturating_sub(net_delta),
        );
        write_perp_open_notional(
            &account_id,
            read_perp_open_notional(&account_id).saturating_sub(position.notional_usdc.0),
        );
        write_perp_position_count(
            &account_id,
            read_perp_position_count(&account_id).saturating_sub(1),
        );
        delete_perp_position(&account_id, &position_id);
        if let Some(config) = &agent_config {
            if agent_signed {
                write_agent_action_nonce(&account_id, nonce);
            } else {
                write_user_perp_nonce(&owner_record, nonce);
            }
            write_agent_open_notional(
                &account_id,
                read_agent_open_notional(&account_id).saturating_sub(position.notional_usdc.0),
            );
            write_agent_reserved_margin(
                &account_id,
                read_agent_reserved_margin(&account_id).saturating_sub(position.margin_usdc.0),
            );
            write_agent_position_count(
                &account_id,
                read_agent_position_count(&account_id).saturating_sub(1),
            );
            settle_storage_credit(&config.user_record_id, storage_before);
        } else {
            write_user_perp_nonce(&account_id, nonce);
            settle_storage_credit(&account_id, storage_before);
        }
        U128(user_credit)
    }

    pub fn claim_perp_deferred_profit(&mut self, account_id: String) -> U128 {
        let storage_before = env::storage_usage();
        let owed = read_perp_deferred(&account_id);
        assert!(owed > 0, "nothing claimable");
        let cash = self
            .balances
            .get(&SHARED_VAULT_RECORD_ID.to_string())
            .unwrap_or_default();
        let available = cash
            .saturating_sub(read_u128_storage(PERP_INSURANCE_KEY))
            .saturating_sub(read_vault_reserved_payout());
        let paid = owed.min(available);
        assert!(paid > 0, "vault has no free liquidity");
        let beneficiary_id = read_agent_config(&account_id)
            .map(|config| config.user_record_id)
            .unwrap_or_else(|| account_id.clone());
        let balance = self.balances.get(&beneficiary_id).unwrap_or_default();
        self.balances
            .insert(&SHARED_VAULT_RECORD_ID.to_string(), &(cash - paid));
        self.balances.insert(
            &beneficiary_id,
            &balance.checked_add(paid).expect("balance overflow"),
        );
        write_perp_deferred(&account_id, owed - paid);
        write_u128_storage(
            PERP_DEFERRED_TOTAL_KEY,
            read_u128_storage(PERP_DEFERRED_TOTAL_KEY).saturating_sub(paid),
        );
        settle_storage_credit(&beneficiary_id, storage_before);
        U128(paid)
    }

    /// Permissionless liquidation once equity is below maintenance or the
    /// seven-day maximum holding period has elapsed. The caller's reward is
    /// credited only to its own registered NEAR logical record.
    pub fn liquidate_perp_position(
        &mut self,
        account_id: String,
        position_id: String,
        keeper_record_id: String,
    ) -> U128 {
        let mut position =
            read_perp_position(&account_id, &position_id).expect("position does not exist");
        let market = read_perp_market(&position.market_id).expect("market not listed");
        let mark = require_healthy_market_price(&position.market_id);
        let health = read_oracle_health(&position.market_id);
        assert!(!health.circuit_broken, "oracle circuit breaker active");
        // The liquidation may create a deferred-profit record as well as
        // update balances and exposure; include all of it in storage metering.
        let storage_before = env::storage_usage();
        settle_position_funding(&mut position, &mut self.balances);
        let pnl = crate::engine_math::linear_pnl(
            position.notional_usdc.0,
            position.entry_price.0,
            mark.price.0,
            position.is_long,
        )
        .expect("PnL overflow");
        let current_notional =
            position.notional_usdc.0.saturating_mul(mark.price.0) / position.entry_price.0;
        let expired = env::block_timestamp() / 1_000_000
            >= position
                .opened_at_ms
                .saturating_add(PERP_MAX_POSITION_AGE_MS);
        assert!(
            expired
                || crate::engine_math::is_liquidatable(
                    position.margin_usdc.0,
                    pnl,
                    current_notional,
                    market.maintenance_margin_bps
                )
                .expect("maintenance calculation overflow"),
            "position is above maintenance margin"
        );
        let caller_record = format!("near:{}", env::predecessor_account_id());
        assert_eq!(
            keeper_record_id, caller_record,
            "keeper reward must go to caller's NEAR record"
        );
        let keeper_registered = read_owner(&keeper_record_id).is_some();
        let agent_config = read_agent_config(&account_id);
        let beneficiary_id = agent_config
            .as_ref()
            .map(|config| config.user_record_id.clone())
            .unwrap_or_else(|| account_id.clone());
        let calculated_penalty =
            crate::engine_math::fee_ceil(current_notional, PERP_LIQUIDATION_PENALTY_BPS)
                .expect("penalty overflow")
                .min(position.margin_usdc.0);
        let (loss, remaining_margin) = if pnl < 0 {
            let loss = pnl.unsigned_abs().min(position.margin_usdc.0);
            (loss, position.margin_usdc.0 - loss)
        } else {
            (0, position.margin_usdc.0)
        };
        let penalty = calculated_penalty.min(remaining_margin);
        if agent_config.is_some() {
            record_agent_realized_loss(&account_id, loss.saturating_add(penalty));
        }
        let keeper_reward = if keeper_registered {
            penalty * u128::from(PERP_LIQUIDATOR_REWARD_BPS)
                / u128::from(PERP_LIQUIDATION_PENALTY_BPS)
        } else {
            0
        };
        let vault_fee = penalty - keeper_reward;
        let mut user_credit = remaining_margin.saturating_sub(penalty);
        let vault_id = SHARED_VAULT_RECORD_ID.to_string();
        let vault_balance = self.balances.get(&vault_id).unwrap_or_default();
        if pnl > 0 {
            let free = vault_balance
                .saturating_sub(read_u128_storage(PERP_INSURANCE_KEY))
                .saturating_sub(read_u128_storage(PERP_DEFERRED_TOTAL_KEY))
                .saturating_sub(
                    read_vault_reserved_payout().saturating_sub(position.notional_usdc.0),
                );
            let paid = (pnl as u128).min(free);
            user_credit = user_credit.saturating_add(paid);
            let unpaid = (pnl as u128).saturating_sub(paid);
            if unpaid != 0 {
                write_perp_deferred(
                    &beneficiary_id,
                    read_perp_deferred(&beneficiary_id)
                        .checked_add(unpaid)
                        .expect("claim overflow"),
                );
                write_u128_storage(
                    PERP_DEFERRED_TOTAL_KEY,
                    read_u128_storage(PERP_DEFERRED_TOTAL_KEY)
                        .checked_add(unpaid)
                        .expect("claim overflow"),
                );
            }
            self.balances.insert(
                &vault_id,
                &vault_balance.saturating_add(vault_fee).saturating_sub(paid),
            );
        } else {
            self.balances.insert(
                &vault_id,
                &vault_balance
                    .checked_add(loss)
                    .and_then(|v| v.checked_add(vault_fee))
                    .expect("vault overflow"),
            );
        }
        if penalty != 0 {
            let reserve = penalty / 4;
            write_u128_storage(
                PERP_INSURANCE_KEY,
                read_u128_storage(PERP_INSURANCE_KEY)
                    .checked_add(reserve)
                    .expect("insurance overflow"),
            );
        }
        if user_credit != 0 {
            let user_balance = self.balances.get(&beneficiary_id).unwrap_or_default();
            self.balances.insert(
                &beneficiary_id,
                &user_balance
                    .checked_add(user_credit)
                    .expect("balance overflow"),
            );
        }
        let mut exposure = read_perp_exposure(&position.market_id);
        let base_units = perp_base_units(position.notional_usdc.0, position.entry_price.0);
        if position.is_long {
            exposure.long_notional_usdc = U128(
                exposure
                    .long_notional_usdc
                    .0
                    .checked_sub(position.notional_usdc.0)
                    .expect("long exposure invariant"),
            );
            exposure.long_base_units = U128(exposure.long_base_units.0.saturating_sub(base_units));
        } else {
            exposure.short_notional_usdc = U128(
                exposure
                    .short_notional_usdc
                    .0
                    .checked_sub(position.notional_usdc.0)
                    .expect("short exposure invariant"),
            );
            exposure.short_base_units =
                U128(exposure.short_base_units.0.saturating_sub(base_units));
        }
        write_perp_exposure(&exposure, &position.market_id);
        write_vault_reserved_payout(
            read_vault_reserved_payout().saturating_sub(position.notional_usdc.0),
        );
        write_u128_storage(
            PERP_TOTAL_MARGIN_KEY,
            read_u128_storage(PERP_TOTAL_MARGIN_KEY).saturating_sub(position.margin_usdc.0),
        );
        write_u128_storage(
            PERP_TOTAL_OI_KEY,
            read_u128_storage(PERP_TOTAL_OI_KEY).saturating_sub(position.notional_usdc.0),
        );
        let net_delta = if position.is_long {
            position.notional_usdc.0 as i128
        } else {
            -(position.notional_usdc.0 as i128)
        };
        write_i128_storage(
            PERP_GLOBAL_NET_KEY,
            read_i128_storage(PERP_GLOBAL_NET_KEY).saturating_sub(net_delta),
        );
        write_perp_open_notional(
            &account_id,
            read_perp_open_notional(&account_id).saturating_sub(position.notional_usdc.0),
        );
        write_perp_position_count(
            &account_id,
            read_perp_position_count(&account_id).saturating_sub(1),
        );
        delete_perp_position(&account_id, &position_id);
        if agent_config.is_some() {
            write_agent_open_notional(
                &account_id,
                read_agent_open_notional(&account_id).saturating_sub(position.notional_usdc.0),
            );
            write_agent_reserved_margin(
                &account_id,
                read_agent_reserved_margin(&account_id).saturating_sub(position.margin_usdc.0),
            );
            write_agent_position_count(
                &account_id,
                read_agent_position_count(&account_id).saturating_sub(1),
            );
        }
        settle_storage_credit_allow_subsidy(
            agent_config
                .as_ref()
                .map(|c| c.user_record_id.as_str())
                .unwrap_or(&account_id),
            storage_before,
        );
        if keeper_reward != 0 {
            let keeper_storage_before = env::storage_usage();
            let keeper_balance = self.balances.get(&keeper_record_id).unwrap_or_default();
            self.balances.insert(
                &keeper_record_id,
                &keeper_balance
                    .checked_add(keeper_reward)
                    .expect("keeper reward overflow"),
            );
            settle_storage_credit_allow_subsidy(&keeper_record_id, keeper_storage_before);
        }
        env::log_str(&format!("EVENT_JSON:{{\"standard\":\"afterbell\",\"version\":\"1.0.0\",\"event\":\"perp_liquidated\",\"data\":[{{\"account_id\":\"{}\",\"position_id\":\"{}\",\"keeper_record_id\":\"{}\",\"pnl_usdc\":\"{}\"}}]}}", account_id, position_id, keeper_record_id, pnl));
        U128(keeper_reward)
    }

    pub fn trade_quote_message(
        &self,
        account_id: String,
        position_id: String,
        market_id: String,
        is_long: bool,
        notional: U128,
        leverage_bps: u32,
        reference_price: U128,
        max_slippage_bps: u16,
        price_sequence: u64,
        fee_bps: u16,
        nonce: u64,
        expires_at_ms: u64,
    ) -> String {
        perp_trade_message(
            &account_id,
            &position_id,
            &market_id,
            is_long,
            notional,
            leverage_bps,
            reference_price,
            max_slippage_bps,
            price_sequence,
            fee_bps,
            nonce,
            expires_at_ms,
        )
    }

    /// Owner-signed market order against the latest fresh A1 mark. The quote
    /// service signs the same exact payload; user authorization remains the
    /// authority for movement of that user's margin.
    #[allow(clippy::too_many_arguments)]
    pub fn open_perp_position(
        &mut self,
        account_id: String,
        position_id: String,
        market_id: String,
        is_long: bool,
        notional: U128,
        leverage_bps: u32,
        reference_price: U128,
        max_slippage_bps: u16,
        price_sequence: u64,
        fee_bps: u16,
        nonce: u64,
        expires_at_ms: u64,
        owner_signature: Base64VecU8,
        quote_signature: Base64VecU8,
    ) -> U128 {
        assert_valid_record_id(&position_id);
        assert!(!self.perp_paused(), "perps paused");
        let market = read_perp_market(&market_id).expect("market not listed");
        assert!(
            market.enabled && !market.close_only,
            "market is not accepting opens"
        );
        assert!(
            notional.0 > 0 && notional.0 <= market.max_position_usdc.0,
            "position cap exceeded"
        );
        assert!(
            leverage_bps >= 10_000 && leverage_bps <= market.max_leverage_bps,
            "leverage limit exceeded"
        );
        assert!(fee_bps <= MAX_PERP_FEE_BPS, "fee exceeds hard bound");
        assert!(
            max_slippage_bps <= MAX_PERP_SLIPPAGE_BPS,
            "slippage exceeds hard bound"
        );
        assert!(
            read_perp_position(&account_id, &position_id).is_none(),
            "position id already exists"
        );
        assert!(
            read_perp_position_count(&account_id) < MAX_PERP_POSITIONS_PER_RECORD,
            "position count limit exceeded"
        );
        assert_eq!(
            read_market_price(&market_id)
                .expect("market price unavailable")
                .sequence,
            price_sequence,
            "stale quote sequence"
        );
        let price = require_fresh_market_price(&market_id);
        assert!(reference_price.0 > 0, "reference price must be positive");
        let price_delta = price.price.0.abs_diff(reference_price.0);
        let max_deviation = u128::from(max_slippage_bps.min(market.max_execution_deviation_bps));
        assert!(
            price_delta.saturating_mul(10_000) <= reference_price.0.saturating_mul(max_deviation),
            "execution price exceeds signed slippage"
        );
        let health = read_oracle_health(&market_id);
        assert!(
            !health.circuit_broken && health.confidence_bps <= market.max_oracle_confidence_bps,
            "oracle mark unsafe"
        );
        assert!(
            health.spot_twap_deviation_bps <= market.max_execution_deviation_bps,
            "oracle spot/TWAP deviation too high"
        );
        let agent_config = read_agent_config(&account_id);
        if let Some(config) = &agent_config {
            let now = env::block_timestamp() / 1_000_000;
            assert!(
                config.enabled && now < config.expires_at_ms,
                "agent authorization expired"
            );
            assert!(
                expires_at_ms <= config.expires_at_ms,
                "agent authorization expired"
            );
            assert!(
                config.allowed_markets.contains(&market_id),
                "market is not allowed for this agent"
            );
            assert_agent_daily_loss_allowed(&account_id, config);
            assert!(
                notional.0 <= config.max_position_usdc.0,
                "agent position limit exceeded"
            );
            assert!(
                leverage_bps <= config.max_leverage_bps,
                "agent leverage limit exceeded"
            );
            assert!(
                read_agent_position(&account_id, &position_id).is_none(),
                "agent position id already exists"
            );
            assert!(
                read_agent_position_count(&account_id) < MAX_AGENT_POSITIONS,
                "agent position limit exceeded"
            );
            assert!(
                read_agent_open_notional(&account_id)
                    .checked_add(notional.0)
                    .is_some_and(|v| v <= config.max_open_notional_usdc.0),
                "agent open notional limit exceeded"
            );
        }
        let nonce_expected = if agent_config.is_some() {
            read_agent_action_nonce(&account_id)
        } else {
            read_user_perp_nonce(&account_id)
        }
        .checked_add(1)
        .expect("trading nonce overflow");
        assert_eq!(nonce, nonce_expected, "invalid trading nonce");
        assert_valid_short_trade_expiry(expires_at_ms);
        let message = perp_trade_message(
            &account_id,
            &position_id,
            &market_id,
            is_long,
            notional,
            leverage_bps,
            reference_price,
            max_slippage_bps,
            price_sequence,
            fee_bps,
            nonce,
            expires_at_ms,
        );
        if let Some(config) = &agent_config {
            let margin = crate::engine_math::required_margin(notional.0, leverage_bps)
                .expect("margin overflow");
            assert!(
                read_agent_reserved_margin(&account_id)
                    .checked_add(margin)
                    .is_some_and(|v| v <= config.max_allocation_usdc.0),
                "agent allocation limit exceeded"
            );
            assert!(
                verify_owner_signature(
                    &config.agent_key,
                    &message,
                    owner_signature.as_ref(),
                    nonce
                ),
                "invalid agent action signature"
            );
        } else {
            let owner = read_owner(&account_id).expect("owner is not registered");
            assert!(
                verify_owner_signature(&owner, &message, owner_signature.as_ref(), nonce),
                "invalid owner signature"
            );
        }
        let quote_signer = read_perp_quote_signer().expect("quote signer is not configured");
        assert!(
            verify_owner_signature(&quote_signer, &message, quote_signature.as_ref(), nonce),
            "invalid quote signature"
        );
        accrue_market_funding(&market_id, &self.balances);
        let margin =
            crate::engine_math::required_margin(notional.0, leverage_bps).expect("margin overflow");
        let fee = crate::engine_math::fee_ceil(notional.0, fee_bps).expect("fee overflow");
        if let Some(config) = &agent_config {
            assert_agent_daily_loss_within(&account_id, config, margin.saturating_add(fee));
        }
        let required = margin.checked_add(fee).expect("required balance overflow");
        let balance = self.balances.get(&account_id).unwrap_or_default();
        assert!(balance >= required, "insufficient free USDC");
        let mut exposure = read_perp_exposure(&market_id);
        let nav = vault_nav(&self.balances);
        let old_long_marked = revalue_exposure(exposure.long_base_units.0, price.price.0);
        let old_short_marked = revalue_exposure(exposure.short_base_units.0, price.price.0);
        let old_oi = old_long_marked
            .checked_add(old_short_marked)
            .expect("OI overflow");
        let new_oi = old_oi.checked_add(notional.0).expect("OI overflow");
        assert!(
            new_oi <= nav.saturating_mul(u128::from(market.max_market_oi_bps)) / 10_000,
            "market open interest cap exceeded"
        );
        let total_oi = read_u128_storage(PERP_TOTAL_OI_KEY)
            .checked_add(notional.0)
            .expect("global OI overflow");
        let (marked_total_oi, marked_global_net) =
            marked_global_exposure(&market_id, price.price.0, is_long, notional.0);
        assert!(
            marked_total_oi <= nav.saturating_mul(u128::from(PERP_MAX_TOTAL_OI_BPS)) / 10_000,
            "global utilization cap exceeded"
        );
        let global_net = read_i128_storage(PERP_GLOBAL_NET_KEY)
            .checked_add(if is_long {
                notional.0 as i128
            } else {
                -(notional.0 as i128)
            })
            .expect("global net overflow");
        assert!(
            marked_global_net.unsigned_abs()
                <= nav.saturating_mul(u128::from(PERP_MAX_GLOBAL_NET_BPS)) / 10_000,
            "global directional exposure cap exceeded"
        );
        let (marked_new_long, marked_new_short) = if is_long {
            (old_long_marked + notional.0, old_short_marked)
        } else {
            (old_long_marked, old_short_marked + notional.0)
        };
        assert!(
            marked_new_long.abs_diff(marked_new_short)
                <= nav.saturating_mul(u128::from(market.max_market_skew_bps)) / 10_000,
            "market skew cap exceeded"
        );
        let new_long = exposure.long_notional_usdc.0 + if is_long { notional.0 } else { 0 };
        let new_short = exposure.short_notional_usdc.0 + if is_long { 0 } else { notional.0 };
        let trader_oi = read_perp_open_notional(&account_id)
            .checked_add(notional.0)
            .expect("trader OI overflow");
        assert!(
            trader_oi <= nav.saturating_mul(u128::from(PERP_MAX_TRADER_OI_BPS)) / 10_000,
            "trader open interest cap exceeded"
        );
        let payout_reserve = read_vault_reserved_payout()
            .checked_add(notional.0)
            .expect("vault reserve overflow");
        assert!(
            notional.0 <= vault_risk_available(&self.balances),
            "insufficient vault payout reserve"
        );
        let storage_before = env::storage_usage();
        self.balances.insert(&account_id, &(balance - required));
        let vault_balance = self
            .balances
            .get(&SHARED_VAULT_RECORD_ID.to_string())
            .unwrap_or_default();
        self.balances.insert(
            &SHARED_VAULT_RECORD_ID.to_string(),
            &vault_balance
                .checked_add(fee)
                .expect("vault balance overflow"),
        );
        let opening_insurance = fee / 4;
        write_u128_storage(
            PERP_INSURANCE_KEY,
            read_u128_storage(PERP_INSURANCE_KEY)
                .checked_add(opening_insurance)
                .expect("insurance overflow"),
        );
        write_u128_storage(PERP_TOTAL_OI_KEY, total_oi);
        write_i128_storage(PERP_GLOBAL_NET_KEY, global_net);
        let base_units = perp_base_units(notional.0, price.price.0);
        if is_long {
            exposure.long_notional_usdc = U128(new_long);
            exposure.long_base_units = U128(
                exposure
                    .long_base_units
                    .0
                    .checked_add(base_units)
                    .expect("base exposure overflow"),
            );
        } else {
            exposure.short_notional_usdc = U128(new_short);
            exposure.short_base_units = U128(
                exposure
                    .short_base_units
                    .0
                    .checked_add(base_units)
                    .expect("base exposure overflow"),
            );
        }
        write_perp_exposure(&exposure, &market_id);
        write_vault_reserved_payout(payout_reserve);
        write_perp_position(
            &account_id,
            &position_id,
            &PerpPosition {
                account_id: account_id.clone(),
                position_id: position_id.clone(),
                market_id: market_id.clone(),
                is_long,
                notional_usdc: notional,
                margin_usdc: U128(margin),
                entry_price: price.price,
                opened_at_ms: env::block_timestamp() / 1_000_000,
                funding_index: U128(if is_long {
                    exposure.long_funding_index.0
                } else {
                    exposure.short_funding_index.0
                }),
                funding_remainder: 0,
            },
        );
        write_perp_open_notional(&account_id, trader_oi);
        write_perp_position_count(
            &account_id,
            read_perp_position_count(&account_id)
                .checked_add(1)
                .expect("position count overflow"),
        );
        write_u128_storage(
            PERP_TOTAL_MARGIN_KEY,
            read_u128_storage(PERP_TOTAL_MARGIN_KEY)
                .checked_add(margin)
                .expect("total margin overflow"),
        );
        if agent_config.is_some() {
            record_agent_realized_loss(&account_id, fee);
            write_agent_action_nonce(&account_id, nonce);
            write_agent_open_notional(
                &account_id,
                read_agent_open_notional(&account_id)
                    .checked_add(notional.0)
                    .expect("agent OI overflow"),
            );
            write_agent_reserved_margin(
                &account_id,
                read_agent_reserved_margin(&account_id)
                    .checked_add(margin)
                    .expect("agent margin overflow"),
            );
            let count = read_agent_position_count(&account_id);
            write_agent_position_count(
                &account_id,
                count.checked_add(1).expect("agent position count overflow"),
            );
        } else {
            write_user_perp_nonce(&account_id, nonce);
        }
        settle_storage_credit(
            agent_config
                .as_ref()
                .map(|config| config.user_record_id.as_str())
                .unwrap_or(&account_id),
            storage_before,
        );
        U128(margin)
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
        assert!(
            cfg!(test) || env::current_account_id().as_str().ends_with(".testnet"),
            "legacy publisher is testnet-only"
        );
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
        let _ = (
            agent_record_id,
            position_id,
            market_id,
            is_long,
            notional_usdc,
            leverage_bps,
            reference_price,
            price_sequence,
            max_slippage_bps,
            expires_at_ms,
        );
        panic!("legacy agent market trading is disabled; use the perp trading flow")
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
        max_daily_loss_usdc: U128,
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
            max_daily_loss_usdc,
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
                max_daily_loss_usdc,
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
        max_daily_loss_usdc: U128,
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
            max_daily_loss_usdc,
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
            max_daily_loss_usdc,
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
                max_daily_loss_usdc,
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
        assert_agent_daily_loss_allowed(&agent_record_id, &config);
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
        assert!(
            read_perp_position(&agent_record_id, &position_id).is_none(),
            "position id already exists in perp engine"
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

    /// Disabled legacy entry point. New agent risk must use the shared perp
    /// engine, which applies the global pause, market, oracle and exposure rules.
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
        let _ = (
            agent_record_id,
            position_id,
            market_id,
            is_long,
            notional_usdc,
            leverage_bps,
            reference_price,
            price_sequence,
            max_slippage_bps,
            nonce,
            expires_at_ms,
            signature,
        );
        panic!("legacy agent market trading is disabled; use open_perp_position")
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
        let mark = require_fresh_market_price(&position.market_id).price;
        let realized_pnl = agent_market_position_pnl(&position, mark);
        if realized_pnl < 0 {
            record_agent_realized_loss(&agent_record_id, realized_pnl.unsigned_abs());
        }
        let payout = settle_market_position(&mut self.balances, &position, &config, mark);
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
        let mark = require_fresh_market_price(&position.market_id).price;
        let realized_pnl = agent_market_position_pnl(&position, mark);
        if realized_pnl < 0 {
            record_agent_realized_loss(&agent_record_id, realized_pnl.unsigned_abs());
        }
        let payout = settle_market_position(&mut self.balances, &position, &config, mark);
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

/// Liquidation is permissionless and must not be blocked by a position
/// owner's depleted storage credit. Charge available credit, with any small
/// mandatory bookkeeping shortfall sponsored by the contract. Callers should
/// meter unrelated records separately before invoking this helper.
fn settle_storage_credit_allow_subsidy(account_id: &str, storage_before: u64) {
    let storage_after = env::storage_usage();
    if storage_after <= storage_before {
        settle_storage_credit(account_id, storage_before);
        return;
    }
    let Some(current) = env::storage_read(&storage_credit_storage_key(account_id))
        .map(|_| read_storage_credit(account_id))
    else {
        return;
    };
    let bytes = storage_after - storage_before;
    let cost = env::storage_byte_cost()
        .as_yoctonear()
        .checked_mul(u128::from(bytes))
        .expect("storage cost overflow");
    write_storage_credit(account_id, current.saturating_sub(cost));
}

fn read_vault_reserved_payout() -> u128 {
    read_u128_storage(VAULT_RESERVED_PREFIX)
}

fn write_vault_reserved_payout(value: u128) {
    write_u128_storage(VAULT_RESERVED_PREFIX, value);
}

fn raw_key(prefix: &[u8], parts: &[&str]) -> Vec<u8> {
    let mut key = prefix.to_vec();
    for part in parts {
        key.extend_from_slice(&(part.len() as u32).to_le_bytes());
        key.extend_from_slice(part.as_bytes());
    }
    key
}

fn assert_protocol_admin() {
    let admin: AccountId =
        read_json_storage(PROTOCOL_ADMIN_KEY).expect("protocol admin is not initialized");
    assert_eq!(
        env::predecessor_account_id(),
        admin,
        "protocol admin required"
    );
}

fn read_json_storage<T: for<'de> Deserialize<'de>>(key: &[u8]) -> Option<T> {
    env::storage_read(key)
        .map(|value| near_sdk::serde_json::from_slice(&value).expect("invalid stored JSON"))
}

fn write_json_storage<T: Serialize>(key: &[u8], value: &T) {
    env::storage_write(
        key,
        &near_sdk::serde_json::to_vec(value).expect("serialize storage value"),
    );
}

fn read_perp_market_ids() -> Vec<String> {
    read_json_storage(PERP_MARKET_IDS_KEY).unwrap_or_default()
}
fn write_perp_market_ids(ids: &[String]) {
    write_json_storage(PERP_MARKET_IDS_KEY, &ids);
}
fn read_perp_market(market_id: &str) -> Option<PerpMarketConfig> {
    read_json_storage(&raw_key(PERP_MARKET_PREFIX, &[market_id]))
}
fn write_perp_market(config: &PerpMarketConfig) {
    write_json_storage(&raw_key(PERP_MARKET_PREFIX, &[&config.market_id]), config);
}
fn add_perp_market_id(market_id: &str) {
    let mut ids = read_perp_market_ids();
    if !ids.iter().any(|id| id == market_id) {
        assert!(ids.len() < MAX_PERP_MARKETS, "market capacity reached");
        ids.push(market_id.to_string());
        write_perp_market_ids(&ids);
    }
}
fn perp_market_key(prefix: &[u8], market_id: &str) -> Vec<u8> {
    raw_key(prefix, &[market_id])
}
fn perp_market_time_key(market_id: &str) -> Vec<u8> {
    raw_key(PERP_PENDING_MARKET_AT_PREFIX, &[market_id])
}
fn read_perp_exposure(market_id: &str) -> PerpExposure {
    let mut exposure: PerpExposure =
        read_json_storage(&raw_key(PERP_EXPOSURE_PREFIX, &[market_id])).unwrap_or_default();
    // Convert legacy arithmetic aggregates to the equivalent base exposure
    // under the old average-entry valuation. Existing open positions keep
    // their prior NAV until their next state mutation, then persist the new
    // representation.
    if exposure.long_base_units.0 == 0 && exposure.long_notional_usdc.0 != 0 {
        exposure.long_base_units = U128(legacy_base_units(
            exposure.long_notional_usdc.0,
            exposure.long_entry_value.0,
        ));
    }
    if exposure.short_base_units.0 == 0 && exposure.short_notional_usdc.0 != 0 {
        exposure.short_base_units = U128(legacy_base_units(
            exposure.short_notional_usdc.0,
            exposure.short_entry_value.0,
        ));
    }
    exposure
}
fn write_perp_exposure(exposure: &PerpExposure, market_id: &str) {
    write_json_storage(&raw_key(PERP_EXPOSURE_PREFIX, &[market_id]), exposure);
}
fn perp_base_units(notional: u128, entry_price: u128) -> u128 {
    assert!(entry_price > 0, "entry price must be positive");
    notional
        .checked_mul(PERP_BASE_UNITS_SCALE)
        .expect("base exposure overflow")
        / entry_price
}
fn legacy_base_units(notional: u128, entry_value: u128) -> u128 {
    let average_entry = entry_value / notional;
    if average_entry == 0 {
        return 0;
    }
    perp_base_units(notional, average_entry)
}
fn read_perp_position(account_id: &str, position_id: &str) -> Option<PerpPosition> {
    read_json_storage(&raw_key(PERP_POSITION_PREFIX, &[account_id, position_id]))
}
fn write_perp_position(account_id: &str, position_id: &str, position: &PerpPosition) {
    write_json_storage(
        &raw_key(PERP_POSITION_PREFIX, &[account_id, position_id]),
        position,
    );
}
fn delete_perp_position(account_id: &str, position_id: &str) {
    env::storage_remove(&raw_key(PERP_POSITION_PREFIX, &[account_id, position_id]));
}
fn read_user_perp_nonce(account_id: &str) -> u64 {
    read_u64_storage(&raw_key(PERP_QUOTE_NONCE_PREFIX, &[account_id]))
}
fn write_user_perp_nonce(account_id: &str, nonce: u64) {
    write_u64_storage(&raw_key(PERP_QUOTE_NONCE_PREFIX, &[account_id]), nonce);
}
fn read_perp_position_count(account_id: &str) -> u64 {
    read_u64_storage(&raw_key(PERP_POSITION_COUNT_PREFIX, &[account_id]))
}
fn write_perp_position_count(account_id: &str, count: u64) {
    write_u64_storage(&raw_key(PERP_POSITION_COUNT_PREFIX, &[account_id]), count);
}
fn read_perp_open_notional(account_id: &str) -> u128 {
    read_u128_storage(&raw_key(PERP_OPEN_NOTIONAL_PREFIX, &[account_id]))
}
fn write_perp_open_notional(account_id: &str, amount: u128) {
    write_u128_storage(&raw_key(PERP_OPEN_NOTIONAL_PREFIX, &[account_id]), amount);
}
fn read_perp_quote_signer() -> Option<OwnerKey> {
    read_json_storage(PERP_QUOTE_SIGNER_KEY)
}
fn write_perp_quote_signer(key: &OwnerKey) {
    write_json_storage(PERP_QUOTE_SIGNER_KEY, key);
}
fn read_oracle_health(market_id: &str) -> OracleHealth {
    read_json_storage(&raw_key(PERP_ORACLE_HEALTH_PREFIX, &[market_id])).unwrap_or_default()
}
fn write_oracle_health(market_id: &str, health: &OracleHealth) {
    write_json_storage(&raw_key(PERP_ORACLE_HEALTH_PREFIX, &[market_id]), health);
}
fn read_perp_deferred(account_id: &str) -> u128 {
    read_u128_storage(&raw_key(PERP_DEFERRED_CLAIM_PREFIX, &[account_id]))
}
fn write_perp_deferred(account_id: &str, amount: u128) {
    write_u128_storage(&raw_key(PERP_DEFERRED_CLAIM_PREFIX, &[account_id]), amount);
}
fn read_lp_shares(account_id: &str) -> u128 {
    read_u128_storage(&raw_key(LP_SHARE_PREFIX, &[account_id]))
}
fn write_lp_shares(account_id: &str, shares: u128) {
    write_u128_storage(&raw_key(LP_SHARE_PREFIX, &[account_id]), shares);
}
fn read_lp_withdrawal(account_id: &str) -> Option<LpWithdrawalRequest> {
    read_json_storage(&raw_key(LP_WITHDRAW_PREFIX, &[account_id]))
}
fn write_lp_withdrawal(account_id: &str, request: &LpWithdrawalRequest) {
    write_json_storage(&raw_key(LP_WITHDRAW_PREFIX, &[account_id]), request);
}
fn delete_lp_withdrawal(account_id: &str) {
    env::storage_remove(&raw_key(LP_WITHDRAW_PREFIX, &[account_id]));
}

fn assert_valid_short_trade_expiry(expires_at_ms: u64) {
    let now = env::block_timestamp() / 1_000_000;
    assert!(
        expires_at_ms > now && expires_at_ms <= now.saturating_add(MAX_AGENT_ACTION_TTL_MS),
        "trade quote expired or too long-lived"
    );
}

fn assert_next_user_action(
    account_id: &str,
    nonce: u64,
    expires_at_ms: u64,
    action: &str,
    amount: &str,
    signature: &[u8],
    owner: &OwnerKey,
) {
    let expected = read_user_perp_nonce(account_id)
        .checked_add(1)
        .expect("nonce overflow");
    assert_eq!(nonce, expected, "invalid action nonce");
    assert_valid_expiry(expires_at_ms);
    let message = format!(
        "afterbell:v1\ncontract:{}\naction:{}\naccount:{}\namount:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(),
        action,
        account_id,
        amount,
        nonce,
        expires_at_ms
    );
    assert!(
        verify_owner_signature(owner, &message, signature, nonce),
        "invalid owner signature"
    );
}

fn perp_trade_message(
    account_id: &str,
    position_id: &str,
    market_id: &str,
    is_long: bool,
    notional: U128,
    leverage_bps: u32,
    reference_price: U128,
    max_slippage_bps: u16,
    price_sequence: u64,
    fee_bps: u16,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!("afterbell:v1\ncontract:{}\naction:open_perp_position\naccount:{}\nposition:{}\nmarket:{}\nside:{}\nnotional_usdc:{}\nleverage_bps:{}\nreference_price:{}\nmax_slippage_bps:{}\nprice_sequence:{}\nfee_bps:{}\nnonce:{}\nexpires_at_ms:{}", env::current_account_id(), account_id, position_id, market_id, if is_long {"long"} else {"short"}, notional.0, leverage_bps, reference_price.0, max_slippage_bps, price_sequence, fee_bps, nonce, expires_at_ms)
}

fn perp_close_message(
    position: &PerpPosition,
    reference_price: U128,
    max_slippage_bps: u16,
    sequence: u64,
    fee_bps: u16,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!("afterbell:v1\ncontract:{}\naction:close_perp_position\naccount:{}\nposition:{}\nmarket:{}\nside:{}\nnotional_usdc:{}\nmargin_usdc:{}\nentry_price:{}\nreference_price:{}\nmax_slippage_bps:{}\nprice_sequence:{}\nfee_bps:{}\nnonce:{}\nexpires_at_ms:{}", env::current_account_id(), position.account_id, position.position_id, position.market_id, if position.is_long {"long"} else {"short"}, position.notional_usdc.0, position.margin_usdc.0, position.entry_price.0, reference_price.0, max_slippage_bps, sequence, fee_bps, nonce, expires_at_ms)
}

fn build_oracle_attestation_message(value: &OracleAttestation) -> String {
    format!("afterbell:v1\ncontract:{}\naction:publish_market_price\nmarket:{}\nprice:{}\nspot_price:{}\nconfidence_bps:{}\nobserved_at_ms:{}\nsequence:{}\nsource_round:{}\nexpires_at_ms:{}", env::current_account_id(), value.market_id, value.price.0, value.spot_price.0, value.confidence_bps, value.observed_at_ms, value.sequence, value.source_round, value.expires_at_ms)
}

fn build_bad_debt_cover_message(
    account_id: &str,
    amount: U128,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:cover_perp_bad_debt\naccount:{}\namount_usdc:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(), account_id, amount.0, nonce, expires_at_ms
    )
}

fn build_perp_margin_message(
    position: &PerpPosition,
    action: &str,
    amount: U128,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:{}_perp_margin\naccount:{}\nposition:{}\nmarket:{}\nside:{}\nnotional_usdc:{}\nmargin_usdc:{}\nentry_price:{}\namount_usdc:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(), action, position.account_id, position.position_id,
        position.market_id, if position.is_long { "long" } else { "short" },
        position.notional_usdc.0, position.margin_usdc.0, position.entry_price.0,
        amount.0, nonce, expires_at_ms
    )
}

fn perp_reduce_message(
    position: &PerpPosition,
    reduce_notional: U128,
    reference_price: U128,
    max_slippage_bps: u16,
    sequence: u64,
    fee_bps: u16,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    format!(
        "afterbell:v1\ncontract:{}\naction:reduce_perp_position\naccount:{}\nposition:{}\nmarket:{}\nside:{}\nnotional_usdc:{}\nreduce_notional_usdc:{}\nmargin_usdc:{}\nentry_price:{}\nreference_price:{}\nmax_slippage_bps:{}\nprice_sequence:{}\nfee_bps:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(), position.account_id, position.position_id,
        position.market_id, if position.is_long { "long" } else { "short" },
        position.notional_usdc.0, reduce_notional.0, position.margin_usdc.0,
        position.entry_price.0, reference_price.0, max_slippage_bps, sequence,
        fee_bps, nonce, expires_at_ms
    )
}

fn position_owner_record(account_id: &str) -> String {
    read_agent_config(account_id)
        .map(|config| config.user_record_id)
        .unwrap_or_else(|| account_id.to_string())
}

fn accrue_market_funding(market_id: &str, balances: &LookupMap<String, u128>) {
    let mut exposure = read_perp_exposure(market_id);
    let now = env::block_timestamp() / 1_000_000;
    if exposure.funding_updated_at_ms == 0 {
        exposure.funding_updated_at_ms = now;
        write_perp_exposure(&exposure, market_id);
        return;
    }
    if now <= exposure.funding_updated_at_ms {
        return;
    }
    let elapsed = now - exposure.funding_updated_at_ms;
    exposure.funding_updated_at_ms = now;
    let long = exposure.long_notional_usdc.0;
    let short = exposure.short_notional_usdc.0;
    let skew = long.abs_diff(short);
    if skew == 0 {
        write_perp_exposure(&exposure, market_id);
        return;
    }
    let nav = balances
        .get(&SHARED_VAULT_RECORD_ID.to_string())
        .unwrap_or_default()
        .saturating_sub(read_u128_storage(PERP_INSURANCE_KEY));
    let rate = crate::engine_math::funding_rate_bps_per_hour(
        skew,
        nav,
        PERP_BASE_FUNDING_BPS_HOURLY,
        PERP_MAX_FUNDING_BPS_HOURLY,
    );
    let increment = u128::from(rate).saturating_mul(u128::from(elapsed));
    if long > short {
        exposure.long_funding_index = U128(
            (exposure.long_funding_index.0 as i128)
                .saturating_add(i128::try_from(increment).unwrap_or(i128::MAX)) as u128,
        );
        exposure.short_funding_index = U128(
            (exposure.short_funding_index.0 as i128)
                .saturating_sub(i128::try_from(increment / 2).unwrap_or(i128::MAX))
                as u128,
        );
    } else {
        exposure.short_funding_index = U128(
            (exposure.short_funding_index.0 as i128)
                .saturating_add(i128::try_from(increment).unwrap_or(i128::MAX)) as u128,
        );
        exposure.long_funding_index = U128(
            (exposure.long_funding_index.0 as i128)
                .saturating_sub(i128::try_from(increment / 2).unwrap_or(i128::MAX))
                as u128,
        );
    }
    write_perp_exposure(&exposure, market_id);
}

fn settle_position_funding(position: &mut PerpPosition, balances: &mut LookupMap<String, u128>) {
    accrue_market_funding(&position.market_id, balances);
    let exposure = read_perp_exposure(&position.market_id);
    let current = if position.is_long {
        exposure.long_funding_index.0 as i128
    } else {
        exposure.short_funding_index.0 as i128
    };
    let previous = position.funding_index.0 as i128;
    let delta = current.saturating_sub(previous);
    let unsigned_numerator = position
        .notional_usdc
        .0
        .saturating_mul(delta.unsigned_abs())
        .min(i128::MAX as u128);
    let signed_numerator = if delta < 0 {
        -(unsigned_numerator as i128)
    } else {
        unsigned_numerator as i128
    }
    .saturating_add(position.funding_remainder);
    let divisor = 10_000_i128 * 60 * 60 * 1_000;
    let amount_signed = signed_numerator / divisor;
    position.funding_remainder = signed_numerator % divisor;
    if amount_signed > 0 {
        let amount = amount_signed as u128;
        let fee = amount.min(position.margin_usdc.0);
        if read_agent_config(&position.account_id).is_some() {
            record_agent_realized_loss(&position.account_id, fee);
        }
        position.margin_usdc = U128(position.margin_usdc.0 - fee);
        if read_agent_config(&position.account_id).is_some() {
            write_agent_reserved_margin(
                &position.account_id,
                read_agent_reserved_margin(&position.account_id).saturating_sub(fee),
            );
        }
        write_u128_storage(
            PERP_TOTAL_MARGIN_KEY,
            read_u128_storage(PERP_TOTAL_MARGIN_KEY).saturating_sub(fee),
        );
        let vault_id = SHARED_VAULT_RECORD_ID.to_string();
        let vault = balances.get(&vault_id).unwrap_or_default();
        balances.insert(&vault_id, &vault.saturating_add(fee));
        let reserve = fee / 2;
        write_u128_storage(
            PERP_INSURANCE_KEY,
            read_u128_storage(PERP_INSURANCE_KEY).saturating_add(reserve),
        );
    } else if amount_signed < 0 {
        let amount = amount_signed.unsigned_abs();
        // Rebate funding is limited to insurance, so funding recipients can
        // never make the shared vault insolvent.
        let rebate = amount.min(read_u128_storage(PERP_INSURANCE_KEY));
        position.margin_usdc = U128(position.margin_usdc.0.saturating_add(rebate));
        if read_agent_config(&position.account_id).is_some() {
            write_agent_reserved_margin(
                &position.account_id,
                read_agent_reserved_margin(&position.account_id)
                    .checked_add(rebate)
                    .expect("agent reserved margin overflow"),
            );
        }
        write_u128_storage(
            PERP_TOTAL_MARGIN_KEY,
            read_u128_storage(PERP_TOTAL_MARGIN_KEY).saturating_add(rebate),
        );
        write_u128_storage(
            PERP_INSURANCE_KEY,
            read_u128_storage(PERP_INSURANCE_KEY) - rebate,
        );
        let vault_id = SHARED_VAULT_RECORD_ID.to_string();
        let vault = balances.get(&vault_id).unwrap_or_default();
        balances.insert(&vault_id, &vault.saturating_sub(rebate));
    }
    position.funding_index = U128(current as u128);
    write_perp_position(&position.account_id, &position.position_id, position);
}

fn vault_prices_fresh() -> bool {
    read_perp_market_ids().iter().all(|id| {
        let exposure = read_perp_exposure(id);
        if exposure.long_notional_usdc.0 == 0 && exposure.short_notional_usdc.0 == 0 {
            return true;
        }
        let Some(price) = read_market_price(id) else {
            return false;
        };
        let config = read_perp_market(id);
        let health = read_oracle_health(id);
        let now = env::block_timestamp() / 1_000_000;
        price.price.0 > 0
            && price.observed_at_ms <= now
            && now.saturating_sub(price.observed_at_ms) <= MAX_MARKET_PRICE_AGE_MS
            && config.is_some_and(|m| {
                health.confidence_bps <= m.max_oracle_confidence_bps && !health.circuit_broken
            })
    })
}

fn vault_risk_available(balances: &LookupMap<String, u128>) -> u128 {
    balances
        .get(&SHARED_VAULT_RECORD_ID.to_string())
        .unwrap_or_default()
        .saturating_sub(read_u128_storage(PERP_INSURANCE_KEY))
        .saturating_sub(read_u128_storage(PERP_DEFERRED_TOTAL_KEY))
        .saturating_sub(read_vault_reserved_payout())
}

fn revalue_exposure(base_units: u128, mark_price: u128) -> u128 {
    base_units
        .checked_mul(mark_price)
        .map(|value| value / PERP_BASE_UNITS_SCALE)
        .unwrap_or(u128::MAX)
}

fn marked_global_exposure(
    target_market: &str,
    target_price: u128,
    target_is_long: bool,
    added_notional: u128,
) -> (u128, i128) {
    let mut gross = 0u128;
    let mut directional_skew = 0i128;
    for market_id in read_perp_market_ids() {
        let exposure = read_perp_exposure(&market_id);
        if market_id != target_market
            && exposure.long_notional_usdc.0 == 0
            && exposure.short_notional_usdc.0 == 0
        {
            continue;
        }
        let mark = if market_id == target_market {
            target_price
        } else {
            require_fresh_market_price(&market_id).price.0
        };
        let market = read_perp_market(&market_id).expect("active market config unavailable");
        let health = read_oracle_health(&market_id);
        assert!(
            !health.circuit_broken
                && health.confidence_bps <= market.max_oracle_confidence_bps
                && health.spot_twap_deviation_bps <= market.max_execution_deviation_bps,
            "active market oracle unsafe for global risk calculation"
        );
        let long = revalue_exposure(exposure.long_base_units.0, mark);
        let short = revalue_exposure(exposure.short_base_units.0, mark);
        let long = long.saturating_add(if market_id == target_market && target_is_long {
            added_notional
        } else {
            0
        });
        let short = short.saturating_add(if market_id == target_market && !target_is_long {
            added_notional
        } else {
            0
        });
        gross = gross
            .checked_add(long)
            .and_then(|value| value.checked_add(short))
            .expect("global marked OI overflow");
        directional_skew = directional_skew
            .checked_add(
                i128::try_from(long.abs_diff(short))
                    .expect("global directional exposure exceeds signed range"),
            )
            .expect("global directional exposure overflow");
    }
    (gross, directional_skew)
}

fn vault_nav(balances: &LookupMap<String, u128>) -> u128 {
    if !vault_prices_fresh() {
        return 0;
    }
    let cash = balances
        .get(&SHARED_VAULT_RECORD_ID.to_string())
        .unwrap_or_default()
        .saturating_sub(read_u128_storage(PERP_INSURANCE_KEY))
        .saturating_sub(read_u128_storage(PERP_DEFERRED_TOTAL_KEY));
    let mut pnl: i128 = 0;
    for id in read_perp_market_ids() {
        let exposure = read_perp_exposure(&id);
        let Some(price) = read_market_price(&id) else {
            continue;
        };
        if price.price.0 == 0 {
            continue;
        }
        for (notional, base_units, is_long) in [
            (
                exposure.long_notional_usdc.0,
                exposure.long_base_units.0,
                true,
            ),
            (
                exposure.short_notional_usdc.0,
                exposure.short_base_units.0,
                false,
            ),
        ] {
            if notional == 0 || base_units == 0 {
                continue;
            }
            let side_pnl = exposure_pnl(notional, base_units, price.price.0, is_long);
            pnl = pnl.saturating_add(side_pnl);
        }
    }
    if pnl > 0 {
        cash.saturating_sub(pnl as u128)
    } else {
        cash.saturating_add(
            pnl.unsigned_abs()
                .min(read_u128_storage(PERP_TOTAL_MARGIN_KEY)),
        )
    }
}

fn exposure_pnl(notional: u128, base_units: u128, mark_price: u128, is_long: bool) -> i128 {
    let marked_value = revalue_exposure(base_units, mark_price);
    let marked = i128::try_from(marked_value).unwrap_or(i128::MAX);
    let notional = i128::try_from(notional).unwrap_or(i128::MAX);
    if is_long {
        marked.saturating_sub(notional)
    } else {
        notional.saturating_sub(marked)
    }
}

/// Splits a perp settlement into amounts backed by trader margin. Fees are
/// collected only from margin left after realized losses; an unpaid fee is
/// waived rather than being minted into vault accounting or treated as debt.
fn perp_settlement_components(margin: u128, realized_pnl: i128, fee: u128) -> (u128, u128, u128) {
    let loss = if realized_pnl < 0 {
        realized_pnl.unsigned_abs().min(margin)
    } else {
        0
    };
    let margin_after_loss = margin - loss;
    let fee_collected = fee.min(margin_after_loss);
    let margin_credit = margin_after_loss - fee_collected;
    (loss, fee_collected, margin_credit)
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
    let mark = require_healthy_market_price(&position.market_id);
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

fn agent_market_position_pnl(position: &AgentPosition, mark_price: U128) -> i128 {
    let entry = position
        .entry_price
        .expect("price-aware position has no entry price");
    let raw_pnl: i128 =
        calculate_linear_pnl_raw(position.notional_usdc, entry, mark_price, position.is_long)
            .parse()
            .expect("PnL exceeds signed range");
    let margin = i128::try_from(position.margin_usdc.0).expect("margin exceeds signed range");
    let notional = i128::try_from(position.notional_usdc.0).expect("notional exceeds signed range");
    raw_pnl.clamp(-margin, notional)
}

/// Legacy price-aware settlement retained for testnet compatibility. The new
/// A1 perp engine uses `PerpPosition` and the engine settlement helpers above.
fn settle_market_position(
    balances: &mut LookupMap<String, u128>,
    position: &AgentPosition,
    config: &AgentConfig,
    mark_price: U128,
) -> U128 {
    let pnl = agent_market_position_pnl(position, mark_price);
    let margin = position.margin_usdc.0;

    let vault_id = SHARED_VAULT_RECORD_ID.to_string();
    let vault_balance = balances.get(&vault_id).unwrap_or_default();
    let reserved = read_vault_reserved_payout();
    assert!(
        reserved >= position.notional_usdc.0,
        "vault reservation invariant failed"
    );
    let paid_profit = if pnl > 0 {
        let profit = pnl as u128;
        let free = vault_balance
            .saturating_sub(read_u128_storage(PERP_INSURANCE_KEY))
            .saturating_sub(read_u128_storage(PERP_DEFERRED_TOTAL_KEY))
            .saturating_sub(reserved.saturating_sub(position.notional_usdc.0));
        let paid = profit.min(free);
        let unpaid = profit - paid;
        if unpaid != 0 {
            write_perp_deferred(
                &config.user_record_id,
                read_perp_deferred(&config.user_record_id)
                    .checked_add(unpaid)
                    .expect("claim overflow"),
            );
            write_u128_storage(
                PERP_DEFERRED_TOTAL_KEY,
                read_u128_storage(PERP_DEFERRED_TOTAL_KEY)
                    .checked_add(unpaid)
                    .expect("claim overflow"),
            );
        }
        balances.insert(&vault_id, &(vault_balance - paid));
        paid
    } else if pnl < 0 {
        balances.insert(
            &vault_id,
            &vault_balance
                .checked_add(pnl.unsigned_abs())
                .expect("vault balance overflow"),
        );
        0
    } else {
        0
    };

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
    let payout = if pnl > 0 {
        margin
            .checked_add(paid_profit)
            .expect("settlement payout overflow")
    } else {
        u128::try_from(i128::try_from(margin).expect("margin exceeds signed range") + pnl)
            .expect("settlement payout is negative")
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
    max_daily_loss_usdc: U128,
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
        max_daily_loss_usdc.0 > 0,
        "daily loss limit must be positive"
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

fn read_i128_storage(key: &[u8]) -> i128 {
    env::storage_read(key)
        .map(|bytes| i128::from_le_bytes(bytes.as_slice().try_into().expect("invalid stored i128")))
        .unwrap_or_default()
}

fn write_i128_storage(key: &[u8], value: i128) {
    env::storage_write(key, &value.to_le_bytes());
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

fn agent_daily_loss_key(agent_record_id: &str) -> Vec<u8> {
    raw_key(AGENT_DAILY_LOSS_PREFIX, &[agent_record_id])
}

fn utc_day_index() -> u64 {
    env::block_timestamp() / 1_000_000 / 86_400_000
}

fn read_agent_daily_loss(agent_record_id: &str) -> AgentDailyLoss {
    read_json_storage::<AgentDailyLoss>(&agent_daily_loss_key(agent_record_id))
        .filter(|entry| entry.utc_day == utc_day_index())
        .unwrap_or(AgentDailyLoss {
            utc_day: utc_day_index(),
            realized_loss_usdc: U128(0),
        })
}

fn record_agent_realized_loss(agent_record_id: &str, loss_usdc: u128) {
    if loss_usdc == 0 {
        return;
    }
    let mut daily = read_agent_daily_loss(agent_record_id);
    daily.realized_loss_usdc = U128(
        daily
            .realized_loss_usdc
            .0
            .checked_add(loss_usdc)
            .expect("agent daily loss overflow"),
    );
    write_json_storage(&agent_daily_loss_key(agent_record_id), &daily);
}

fn assert_agent_daily_loss_allowed(agent_record_id: &str, config: &AgentConfig) {
    assert!(
        read_agent_daily_loss(agent_record_id).realized_loss_usdc.0 < config.max_daily_loss_usdc.0,
        "agent daily loss limit reached"
    );
}

fn assert_agent_daily_loss_within(agent_record_id: &str, config: &AgentConfig, additional: u128) {
    assert!(
        read_agent_daily_loss(agent_record_id)
            .realized_loss_usdc
            .0
            .checked_add(read_agent_reserved_margin(agent_record_id))
            .and_then(|risk| risk.checked_add(additional))
            .is_some_and(|loss| loss <= config.max_daily_loss_usdc.0),
        "agent daily loss limit reached"
    );
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

fn require_healthy_market_price(market_id: &str) -> MarketPrice {
    let price = require_fresh_market_price(market_id);
    let market = read_perp_market(market_id).expect("market not listed");
    let health = read_oracle_health(market_id);
    assert!(
        !health.circuit_broken
            && health.confidence_bps <= market.max_oracle_confidence_bps
            && health.spot_twap_deviation_bps <= market.max_execution_deviation_bps,
        "oracle mark unsafe"
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
    max_daily_loss_usdc: U128,
    nonce: u64,
    expires_at_ms: u64,
) -> String {
    let markets = near_sdk::serde_json::to_string(allowed_markets).expect("serialize markets");
    format!(
        "afterbell:v1\ncontract:{}\naction:create_agent_record\nuser_record:{}\nagent_record:{}\nagent_key:{}\nallowed_markets:{}\nmax_position_usdc:{}\nmax_open_notional_usdc:{}\nmax_leverage_bps:{}\nmax_allocation_usdc:{}\nmax_daily_loss_usdc:{}\nnonce:{}\nexpires_at_ms:{}",
        env::current_account_id(),
        user_record_id,
        agent_record_id,
        owner_key_id(agent_key),
        markets,
        max_position_usdc.0,
        max_open_notional_usdc.0,
        max_leverage_bps,
        max_allocation_usdc.0,
        max_daily_loss_usdc.0,
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
#[allow(dead_code)]
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
            || read_owner(&deposit.account_id).is_none()
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
        SharedLedger::new(TOKEN.parse().unwrap(), "a1.near".parse().unwrap())
    }

    #[test]
    fn perp_settlement_never_collects_fees_beyond_remaining_margin() {
        assert_eq!(perp_settlement_components(100, -100, 10), (100, 0, 0));
        assert_eq!(perp_settlement_components(100, -120, 10), (100, 0, 0));
        assert_eq!(perp_settlement_components(100, 20, 150), (0, 100, 0));
        assert_eq!(perp_settlement_components(100, -40, 30), (40, 30, 30));
    }

    #[test]
    fn isolated_loss_above_margin_does_not_spend_insurance_or_pause_perps() {
        let mut contract = setup("a1.near");
        let owner_key = SigningKey::from_bytes(&[88; 32]);
        let quote_key = SigningKey::from_bytes(&[87; 32]);
        configure_test_perp(&mut contract, &quote_key);
        register_solana_owner_for(&mut contract, USER, &owner_key);
        write_storage_credit(USER, u128::MAX - 10_u128.pow(30));
        fund_record(&mut contract, SHARED_VAULT_RECORD_ID, 10_000, "loss-vault");
        write_market_price(&MarketPrice {
            market_id: "NVDA".to_string(),
            price: U128(5_000_000_000),
            observed_at_ms: NOW_MS,
            sequence: 1,
        });
        write_oracle_health(
            "NVDA",
            &OracleHealth {
                circuit_broken: false,
                confidence_bps: 1,
                spot_twap_deviation_bps: 0,
            },
        );
        write_perp_position(
            USER,
            "deep-loss",
            &PerpPosition {
                account_id: USER.to_string(),
                position_id: "deep-loss".to_string(),
                market_id: "NVDA".to_string(),
                is_long: true,
                notional_usdc: U128(1_000),
                margin_usdc: U128(100),
                entry_price: U128(10_000_000_000),
                opened_at_ms: NOW_MS,
                funding_index: U128(0),
                funding_remainder: 0,
            },
        );
        write_perp_exposure(
            &PerpExposure {
                long_notional_usdc: U128(1_000),
                long_base_units: U128(perp_base_units(1_000, 10_000_000_000)),
                ..PerpExposure::default()
            },
            "NVDA",
        );
        write_vault_reserved_payout(1_000);
        write_u128_storage(PERP_TOTAL_MARGIN_KEY, 100);
        write_u128_storage(PERP_TOTAL_OI_KEY, 1_000);
        write_i128_storage(PERP_GLOBAL_NET_KEY, 1_000);
        write_perp_open_notional(USER, 1_000);
        write_perp_position_count(USER, 1);
        write_u128_storage(PERP_INSURANCE_KEY, 200);
        let expiry = NOW_MS + 30_000;
        let message = contract.close_perp_position_message(
            USER.to_string(),
            "deep-loss".to_string(),
            U128(5_000_000_000),
            0,
            1,
            1,
            1,
            expiry,
        );
        let owner_signature = owner_key.sign(message.as_bytes());
        let quote_signature = quote_key.sign(message.as_bytes());
        contract.close_perp_position(
            USER.to_string(),
            "deep-loss".to_string(),
            U128(5_000_000_000),
            0,
            1,
            1,
            1,
            expiry,
            Base64VecU8::from(owner_signature.to_bytes().to_vec()),
            Base64VecU8::from(quote_signature.to_bytes().to_vec()),
        );
        assert_eq!(contract.perp_insurance(), U128(200));
        assert_eq!(contract.perp_bad_debt(), U128(0));
        assert!(!contract.perp_paused());
        assert_eq!(contract.vault_balance(), U128(10_100));
    }

    #[test]
    fn aggregated_perp_exposure_matches_sum_of_position_pnl() {
        let first_base = perp_base_units(100, 100);
        let second_base = perp_base_units(100, 200);
        assert_eq!(revalue_exposure(first_base + second_base, 200), 300);
        assert_eq!(exposure_pnl(200, first_base + second_base, 200, true), 100);
        assert_eq!(
            exposure_pnl(100, first_base, 200, true) + exposure_pnl(100, second_base, 200, true),
            100
        );
    }

    #[test]
    fn perp_json_views_expose_signed_funding_indices_as_strings() {
        let contract = setup("a1.near");
        write_perp_exposure(
            &PerpExposure {
                long_funding_index: U128((-123_i128) as u128),
                short_funding_index: U128(456),
                ..PerpExposure::default()
            },
            "NVDA",
        );
        let exposure = contract.perp_exposure("NVDA".to_string());
        assert_eq!(exposure.long_funding_index, "-123");
        assert_eq!(exposure.short_funding_index, "456");
        write_perp_position(
            USER,
            "signed-funding",
            &PerpPosition {
                account_id: USER.to_string(),
                position_id: "signed-funding".to_string(),
                market_id: "NVDA".to_string(),
                is_long: true,
                notional_usdc: U128(1),
                margin_usdc: U128(1),
                entry_price: U128(1),
                opened_at_ms: NOW_MS,
                funding_index: U128((-9_i128) as u128),
                funding_remainder: 0,
            },
        );
        let position = contract
            .perp_position(USER.to_string(), "signed-funding".to_string())
            .unwrap();
        assert_eq!(position.funding_index, "-9");
        assert_eq!(position.funding_remainder, "0");
    }

    #[test]
    fn global_directional_exposure_does_not_offset_unrelated_markets() {
        let contract = setup("a1.near");
        let config = PerpMarketConfig {
            market_id: "NVDA".to_string(),
            enabled: true,
            close_only: false,
            max_leverage_bps: 50_000,
            maintenance_margin_bps: 500,
            max_execution_deviation_bps: 1_000,
            max_oracle_confidence_bps: 100,
            max_position_usdc: U128(1_000_000),
            max_market_oi_bps: 1_000,
            max_market_skew_bps: 300,
        };
        let mut other_market = config.clone();
        other_market.market_id = "TSLA".to_string();
        for market in [&config, &other_market] {
            add_perp_market_id(&market.market_id);
            write_perp_market(market);
            write_market_price(&MarketPrice {
                market_id: market.market_id.clone(),
                price: U128(100),
                observed_at_ms: NOW_MS,
                sequence: 1,
            });
        }
        write_perp_exposure(
            &PerpExposure {
                long_notional_usdc: U128(100),
                long_base_units: U128(PERP_BASE_UNITS_SCALE),
                ..PerpExposure::default()
            },
            "NVDA",
        );
        write_perp_exposure(
            &PerpExposure {
                short_notional_usdc: U128(100),
                short_base_units: U128(PERP_BASE_UNITS_SCALE),
                ..PerpExposure::default()
            },
            "TSLA",
        );

        let (_, directional_skew) = marked_global_exposure("NVDA", 100, true, 0);

        assert_eq!(directional_skew, 200);
        assert_eq!(contract.perp_market_ids().len(), 2);
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
        if account_id == SHARED_VAULT_RECORD_ID {
            let current = contract
                .balances
                .get(&account_id.to_string())
                .unwrap_or_default();
            contract.balances.insert(
                &account_id.to_string(),
                &current
                    .checked_add(amount)
                    .expect("test vault balance overflow"),
            );
            return;
        }
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
        let mut contract = SharedLedger::new(TOKEN.parse().unwrap(), "a1.near".parse().unwrap());
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
    fn liquidation_bookkeeping_does_not_require_depleted_user_storage_credit() {
        let _contract = setup(TOKEN);
        let storage_before = env::storage_usage();
        env::storage_write(b"a1:test:liquidation-growth", &[1; 256]);

        settle_storage_credit_allow_subsidy(USER, storage_before);

        assert!(env::storage_read(b"a1:test:liquidation-growth").is_some());
        assert_eq!(read_storage_credit(USER), 0);
    }

    #[test]
    fn deferred_profit_claim_storage_is_charged_to_logical_record() {
        let _contract = setup(TOKEN);
        let starting_credit = 10_000_000_000_000_000_000_000_000u128;
        write_storage_credit(USER, starting_credit);

        let before_claim = env::storage_usage();
        write_perp_deferred(USER, 123_456);
        let claim_growth = env::storage_usage() - before_claim;
        settle_storage_credit(USER, before_claim);

        assert!(claim_growth > 0);
        assert_eq!(
            starting_credit - read_storage_credit(USER),
            env::storage_byte_cost().as_yoctonear() * u128::from(claim_growth)
        );
        assert_eq!(read_perp_deferred(USER), 123_456);
    }

    #[test]
    fn deferred_agent_profit_claim_is_credited_to_owning_user_record() {
        let mut contract = setup(TOKEN);
        let old_agent = "agent:near:test-user:old";
        write_agent_config(
            old_agent,
            &AgentConfig {
                user_record_id: USER.to_string(),
                agent_record_id: old_agent.to_string(),
                agent_key: OwnerKey::NearEd25519 {
                    public_key: [7; 32],
                },
                allowed_markets: vec!["NVDA".to_string()],
                max_position_usdc: U128(1_000),
                max_open_notional_usdc: U128(1_000),
                max_leverage_bps: 10_000,
                max_allocation_usdc: U128(1_000),
                max_daily_loss_usdc: U128(1_000),
                expires_at_ms: EXPIRY_MS,
                enabled: false,
            },
        );
        write_agent_record(USER, "agent:near:test-user:new");
        write_perp_deferred(old_agent, 500);
        write_u128_storage(PERP_DEFERRED_TOTAL_KEY, 500);
        contract
            .balances
            .insert(&SHARED_VAULT_RECORD_ID.to_string(), &1_000);
        write_storage_credit(USER, 10_000_000_000_000_000_000_000_000);

        assert_eq!(
            contract.claim_perp_deferred_profit(old_agent.to_string()),
            U128(500)
        );
        assert_eq!(contract.balance_of(USER.to_string()), U128(500));
        assert_eq!(contract.balance_of(old_agent.to_string()), U128(0));
        assert_eq!(read_perp_deferred(old_agent), 0);
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
            U128(1_000_000),
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
            U128(1_000_000),
            expiry,
            1,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
        agent_id
    }

    #[test]
    fn agent_funding_updates_aggregate_reserved_margin() {
        let mut contract = setup(TOKEN);
        let agent_id = "agent:near:test-user:funding";
        let config = AgentConfig {
            user_record_id: USER.to_string(),
            agent_record_id: agent_id.to_string(),
            agent_key: OwnerKey::NearEd25519 {
                public_key: [9; 32],
            },
            allowed_markets: vec!["NVDA".to_string()],
            max_position_usdc: U128(1_000),
            max_open_notional_usdc: U128(1_000),
            max_leverage_bps: 10_000,
            max_allocation_usdc: U128(1_000),
            max_daily_loss_usdc: U128(1_000),
            expires_at_ms: EXPIRY_MS,
            enabled: false,
        };
        write_agent_config(agent_id, &config);
        write_agent_reserved_margin(agent_id, 200); // Perp margin plus a legacy reservation.
        contract
            .balances
            .insert(&SHARED_VAULT_RECORD_ID.to_string(), &1_000_000);
        write_perp_exposure(
            &PerpExposure {
                long_notional_usdc: U128(100),
                funding_updated_at_ms: NOW_MS,
                ..PerpExposure::default()
            },
            "NVDA",
        );
        let mut position = PerpPosition {
            account_id: agent_id.to_string(),
            position_id: "funded-perp".to_string(),
            market_id: "NVDA".to_string(),
            is_long: true,
            notional_usdc: U128(100),
            margin_usdc: U128(100),
            entry_price: U128(1),
            opened_at_ms: NOW_MS,
            funding_index: U128(0),
            funding_remainder: 0,
        };
        write_perp_position(agent_id, &position.position_id, &position);
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp((NOW_MS + 360_000_000) * 1_000_000);
        testing_env!(context.build());

        settle_position_funding(&mut position, &mut contract.balances);

        assert_eq!(position.margin_usdc, U128(99));
        assert_eq!(read_agent_reserved_margin(agent_id), 199);
        assert!(read_agent_reserved_margin(agent_id) >= 100);
    }

    #[test]
    fn legacy_market_settlement_preserves_insurance_and_defers_unfunded_profit() {
        let mut contract = setup(TOKEN);
        let agent_id = "agent:near:test-user:legacy";
        let config = AgentConfig {
            user_record_id: USER.to_string(),
            agent_record_id: agent_id.to_string(),
            agent_key: OwnerKey::NearEd25519 {
                public_key: [8; 32],
            },
            allowed_markets: vec!["NVDA".to_string()],
            max_position_usdc: U128(1_000),
            max_open_notional_usdc: U128(1_000),
            max_leverage_bps: 10_000,
            max_allocation_usdc: U128(1_000),
            max_daily_loss_usdc: U128(1_000),
            expires_at_ms: EXPIRY_MS,
            enabled: false,
        };
        let position = AgentPosition {
            position_id: "legacy-priced".to_string(),
            agent_record_id: agent_id.to_string(),
            market_id: "NVDA".to_string(),
            is_long: true,
            notional_usdc: U128(100),
            margin_usdc: U128(100),
            leverage_bps: 10_000,
            entry_price: Some(U128(100)),
        };
        write_agent_reserved_margin(agent_id, 100);
        write_agent_open_notional(agent_id, 100);
        write_agent_position_count(agent_id, 1);
        write_vault_reserved_payout(500);
        write_u128_storage(PERP_INSURANCE_KEY, 80);
        contract
            .balances
            .insert(&SHARED_VAULT_RECORD_ID.to_string(), &100);

        assert_eq!(
            settle_market_position(&mut contract.balances, &position, &config, U128(200)),
            U128(100)
        );
        assert_eq!(contract.balance_of(USER.to_string()), U128(100));
        assert_eq!(contract.vault_balance(), U128(100));
        assert_eq!(contract.perp_insurance(), U128(80));
        assert_eq!(read_perp_deferred(USER), 100);
    }

    #[test]
    fn agent_daily_loss_resets_at_utc_day_boundary() {
        let mut contract = setup(TOKEN);
        let owner_key = SigningKey::from_bytes(&[121; 32]);
        let agent_key = SigningKey::from_bytes(&[122; 32]);
        let agent_id = setup_agent(&mut contract, &owner_key, &agent_key);
        record_agent_realized_loss(&agent_id, 75_000);
        assert_eq!(contract.agent_daily_loss(agent_id.clone()), U128(75_000));

        let mut next_day = VMContextBuilder::new();
        next_day
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp((NOW_MS + 86_400_000) * 1_000_000);
        testing_env!(next_day.build());
        assert_eq!(contract.agent_daily_loss(agent_id), U128(0));
    }

    #[test]
    #[should_panic(expected = "agent daily loss limit reached")]
    fn agent_cannot_open_after_realized_daily_loss_cap() {
        let mut contract = setup("a1.near");
        let owner_key = SigningKey::from_bytes(&[123; 32]);
        let agent_key = SigningKey::from_bytes(&[124; 32]);
        let quote_key = SigningKey::from_bytes(&[125; 32]);
        configure_test_perp(&mut contract, &quote_key);
        let agent_id = setup_agent(&mut contract, &owner_key, &agent_key);
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        fund_test_storage_credit(&USER.to_string());
        contract.balances.insert(&agent_id, &1_000_000);
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            100_000_000,
            "daily-loss-vault",
        );
        publish_test_price(&mut contract, U128(10_000_000_000), 1);
        let open_expiry = NOW_MS + 30_000;
        let open_message = contract.trade_quote_message(
            agent_id.clone(),
            "daily-loss-position".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            100,
            1,
            0,
            1,
            open_expiry,
        );
        contract.open_perp_position(
            agent_id.clone(),
            "daily-loss-position".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            100,
            1,
            0,
            1,
            open_expiry,
            Base64VecU8::from(agent_key.sign(open_message.as_bytes()).to_bytes().to_vec()),
            Base64VecU8::from(quote_key.sign(open_message.as_bytes()).to_bytes().to_vec()),
        );
        let mut config = read_agent_config(&agent_id).unwrap();
        config.max_daily_loss_usdc = U128(40_000);
        write_agent_config(&agent_id, &config);
        write_market_price(&MarketPrice {
            market_id: "NVDA".to_string(),
            price: U128(9_000_000_000),
            observed_at_ms: NOW_MS,
            sequence: 2,
        });
        let close_expiry = NOW_MS + 30_000;
        let close_message = contract.close_perp_position_message(
            agent_id.clone(),
            "daily-loss-position".to_string(),
            U128(9_000_000_000),
            1_000,
            2,
            0,
            2,
            close_expiry,
        );
        contract.close_perp_position(
            agent_id.clone(),
            "daily-loss-position".to_string(),
            U128(9_000_000_000),
            1_000,
            2,
            0,
            2,
            close_expiry,
            Base64VecU8::from(agent_key.sign(close_message.as_bytes()).to_bytes().to_vec()),
            Base64VecU8::from(quote_key.sign(close_message.as_bytes()).to_bytes().to_vec()),
        );
        assert_eq!(contract.agent_daily_loss(agent_id.clone()), U128(50_000));
        contract.open_perp_position(
            agent_id,
            "blocked-by-daily-cap".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(9_000_000_000),
            100,
            2,
            0,
            3,
            NOW_MS + 30_000,
            Base64VecU8::from(vec![0; 64]),
            Base64VecU8::from(vec![0; 64]),
        );
    }

    #[test]
    #[should_panic(expected = "agent daily loss limit reached")]
    fn agent_cannot_reserve_margin_beyond_remaining_daily_loss_budget() {
        let mut contract = setup("a1.near");
        let owner_key = SigningKey::from_bytes(&[90; 32]);
        let agent_key = SigningKey::from_bytes(&[89; 32]);
        let agent_id = setup_agent(&mut contract, &owner_key, &agent_key);
        let mut config = read_agent_config(&agent_id).unwrap();
        config.max_daily_loss_usdc = U128(200_000);
        write_agent_config(&agent_id, &config);
        write_agent_reserved_margin(&agent_id, 150_000);
        record_agent_realized_loss(&agent_id, 40_000);
        assert_agent_daily_loss_within(&agent_id, &config, 10_000);
        assert_agent_daily_loss_within(&agent_id, &config, 10_001);
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

    fn configure_test_perp(contract: &mut SharedLedger, quote_key: &SigningKey) {
        fund_test_storage_credit("protocol:market_prices");
        let config = PerpMarketConfig {
            market_id: "NVDA".to_string(),
            enabled: true,
            close_only: false,
            max_leverage_bps: 50_000,
            maintenance_margin_bps: 500,
            max_execution_deviation_bps: 1_000,
            max_oracle_confidence_bps: 100,
            max_position_usdc: U128(1_000_000),
            max_market_oi_bps: 1_000,
            max_market_skew_bps: 300,
        };
        add_perp_market_id(&config.market_id);
        write_perp_market(&config);
        contract.set_perp_quote_signer(OwnerKey::SolanaEd25519 {
            public_key: quote_key.verifying_key().to_bytes(),
        });
    }

    #[test]
    #[should_panic(expected = "protocol admin required")]
    fn non_admin_cannot_change_global_perp_risk_cap() {
        let mut contract = setup(TOKEN);
        contract.set_perp_beta_cap(U128(200_000_000));
    }

    #[test]
    fn owner_and_quote_signed_perp_open_and_close_settle_vault_and_exposure() {
        let mut contract = setup("a1.near");
        let owner_key = SigningKey::from_bytes(&[81; 32]);
        let quote_key = SigningKey::from_bytes(&[82; 32]);
        configure_test_perp(&mut contract, &quote_key);
        register_solana_owner_for(&mut contract, USER, &owner_key);
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        fund_record(&mut contract, USER, 2_000_000, "perp-user");
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            100_000_000,
            "perp-vault",
        );
        publish_test_price(&mut contract, U128(10_000_000_000), 1);

        let expiry = NOW_MS + 30_000;
        let open_message = contract.trade_quote_message(
            USER.to_string(),
            "nvda-1".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            100,
            1,
            6,
            1,
            expiry,
        );
        let owner_signature = owner_key.sign(open_message.as_bytes());
        let quote_signature = quote_key.sign(open_message.as_bytes());
        assert_eq!(
            contract.open_perp_position(
                USER.to_string(),
                "nvda-1".to_string(),
                "NVDA".to_string(),
                true,
                U128(500_000),
                20_000,
                U128(10_000_000_000),
                100,
                1,
                6,
                1,
                expiry,
                Base64VecU8::from(owner_signature.to_bytes().to_vec()),
                Base64VecU8::from(quote_signature.to_bytes().to_vec()),
            ),
            U128(250_000)
        );
        assert_eq!(
            contract
                .perp_position(USER.to_string(), "nvda-1".to_string())
                .unwrap()
                .margin_usdc,
            U128(250_000)
        );
        assert_eq!(contract.vault_reserved_payout(), U128(500_000));

        let add_expiry = NOW_MS + 30_000;
        let add_message = contract.perp_margin_message(
            USER.to_string(),
            "nvda-1".to_string(),
            "add".to_string(),
            U128(10_000),
            2,
            add_expiry,
        );
        let add_signature = owner_key.sign(add_message.as_bytes());
        contract.add_perp_margin(
            USER.to_string(),
            "nvda-1".to_string(),
            U128(10_000),
            2,
            add_expiry,
            Base64VecU8::from(add_signature.to_bytes().to_vec()),
        );
        let remove_expiry = NOW_MS + 30_000;
        let remove_message = contract.perp_margin_message(
            USER.to_string(),
            "nvda-1".to_string(),
            "remove".to_string(),
            U128(10_000),
            3,
            remove_expiry,
        );
        let remove_signature = owner_key.sign(remove_message.as_bytes());
        contract.remove_perp_margin(
            USER.to_string(),
            "nvda-1".to_string(),
            U128(10_000),
            3,
            remove_expiry,
            Base64VecU8::from(remove_signature.to_bytes().to_vec()),
        );

        publish_test_price(&mut contract, U128(11_000_000_000), 2);
        let reduce_expiry = NOW_MS + 30_000;
        let reduce_message = contract.reduce_perp_position_message(
            USER.to_string(),
            "nvda-1".to_string(),
            U128(200_000),
            U128(11_000_000_000),
            0,
            2,
            6,
            4,
            reduce_expiry,
        );
        let reduce_owner_signature = owner_key.sign(reduce_message.as_bytes());
        assert_eq!(
            contract.reduce_perp_position(
                USER.to_string(),
                "nvda-1".to_string(),
                U128(200_000),
                U128(11_000_000_000),
                0,
                2,
                6,
                4,
                reduce_expiry,
                Base64VecU8::from(reduce_owner_signature.to_bytes().to_vec()),
                Base64VecU8::from(Vec::new()),
            ),
            U128(119_880)
        );
        let reduced_position = contract
            .perp_position(USER.to_string(), "nvda-1".to_string())
            .unwrap();
        assert_eq!(reduced_position.notional_usdc, U128(300_000));
        assert_eq!(reduced_position.margin_usdc, U128(150_000));
        let close_expiry = NOW_MS + 30_000;
        let close_message = contract.close_perp_position_message(
            USER.to_string(),
            "nvda-1".to_string(),
            U128(11_000_000_000),
            100,
            2,
            6,
            5,
            close_expiry,
        );
        let owner_signature = owner_key.sign(close_message.as_bytes());
        let quote_signature = quote_key.sign(close_message.as_bytes());
        let paid = contract.close_perp_position(
            USER.to_string(),
            "nvda-1".to_string(),
            U128(11_000_000_000),
            100,
            2,
            6,
            5,
            close_expiry,
            Base64VecU8::from(owner_signature.to_bytes().to_vec()),
            Base64VecU8::from(quote_signature.to_bytes().to_vec()),
        );
        assert_eq!(paid, U128(179_820));
        assert!(contract
            .perp_position(USER.to_string(), "nvda-1".to_string())
            .is_none());
        assert_eq!(contract.vault_reserved_payout(), U128(0));
        assert_eq!(contract.perp_total_margin(), U128(0));
        assert_eq!(
            contract
                .perp_exposure("NVDA".to_string())
                .long_notional_usdc,
            U128(0)
        );
        assert_eq!(contract.balance_of(USER.to_string()), U128(2_049_400));
    }

    #[test]
    fn ethereum_eip191_owner_signature_opens_perp_position_end_to_end() {
        let mut contract = setup("a1.near");
        let secret = SecretKey::from_slice(&[131; 32]).unwrap();
        let public_key = secp256k1::PublicKey::from_secret_key(&Secp256k1::new(), &secret);
        let address_hash = env::keccak256_array(&public_key.serialize_uncompressed()[1..]);
        let mut address = [0; 20];
        address.copy_from_slice(&address_hash[12..]);
        let owner = OwnerKey::Ethereum { address };
        let quote_key = SigningKey::from_bytes(&[132; 32]);
        configure_test_perp(&mut contract, &quote_key);
        let registration = registration_message(USER, &owner, EXPIRY_MS);
        let registration_signature = Secp256k1::new().sign_ecdsa_recoverable(
            &Message::from_slice(&eip191_digest(&registration)).unwrap(),
            &secret,
        );
        let (registration_recovery, registration_compact) =
            registration_signature.serialize_compact();
        let mut registration_bytes = registration_compact.to_vec();
        registration_bytes.push(registration_recovery.to_i32() as u8);
        fund_test_storage_credit(USER);
        contract.register_owner(
            USER.to_string(),
            owner,
            EXPIRY_MS,
            Base64VecU8::from(registration_bytes),
        );
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        fund_record(&mut contract, USER, 1_000_000, "ethereum-perp-user");
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            100_000_000,
            "ethereum-perp-vault",
        );
        publish_test_price(&mut contract, U128(10_000_000_000), 1);
        let expiry = NOW_MS + 30_000;
        let message = contract.trade_quote_message(
            USER.to_string(),
            "eth-nvda".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            100,
            1,
            6,
            1,
            expiry,
        );
        let owner_signature = Secp256k1::new().sign_ecdsa_recoverable(
            &Message::from_slice(&eip191_digest(&message)).unwrap(),
            &secret,
        );
        let (recovery_id, compact) = owner_signature.serialize_compact();
        let mut owner_bytes = compact.to_vec();
        owner_bytes.push(recovery_id.to_i32() as u8);
        let quote_signature = quote_key.sign(message.as_bytes());
        contract.open_perp_position(
            USER.to_string(),
            "eth-nvda".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            100,
            1,
            6,
            1,
            expiry,
            Base64VecU8::from(owner_bytes),
            Base64VecU8::from(quote_signature.to_bytes().to_vec()),
        );
        assert_eq!(
            contract
                .perp_position(USER.to_string(), "eth-nvda".to_string())
                .unwrap()
                .margin_usdc,
            U128(250_000)
        );
    }

    #[test]
    fn agent_trade_obeys_policy_and_owner_can_close_after_revocation() {
        let mut contract = setup("a1.near");
        let owner_key = SigningKey::from_bytes(&[91; 32]);
        let agent_key = SigningKey::from_bytes(&[92; 32]);
        let quote_key = SigningKey::from_bytes(&[93; 32]);
        configure_test_perp(&mut contract, &quote_key);
        let agent_id = setup_agent(&mut contract, &owner_key, &agent_key);
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        fund_record(&mut contract, USER, 1_000_000, "agent-perp-owner");
        fund_test_agent(&mut contract, &owner_key, &agent_id, 1_000_000);
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            100_000_000,
            "agent-perp-vault",
        );
        publish_test_price(&mut contract, U128(10_000_000_000), 1);

        let expiry = NOW_MS + 30_000;
        let message = contract.trade_quote_message(
            agent_id.clone(),
            "agent-nvda-1".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            100,
            1,
            6,
            1,
            expiry,
        );
        let agent_signature = agent_key.sign(message.as_bytes());
        let quote_signature = quote_key.sign(message.as_bytes());
        contract.open_perp_position(
            agent_id.clone(),
            "agent-nvda-1".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            100,
            1,
            6,
            1,
            expiry,
            Base64VecU8::from(agent_signature.to_bytes().to_vec()),
            Base64VecU8::from(quote_signature.to_bytes().to_vec()),
        );
        assert_eq!(
            contract.agent_open_notional(agent_id.clone()),
            U128(500_000)
        );
        assert_eq!(
            contract.agent_reserved_margin(agent_id.clone()),
            U128(250_000)
        );
        assert_eq!(contract.agent_daily_loss(agent_id.clone()), U128(300));

        let revoke_message = agent_revocation_message(USER, &agent_id, 3, EXPIRY_MS);
        let revoke_signature = owner_key.sign(revoke_message.as_bytes());
        contract.revoke_agent(
            USER.to_string(),
            3,
            EXPIRY_MS,
            Base64VecU8::from(revoke_signature.to_bytes().to_vec()),
        );

        let close_expiry = NOW_MS + 30_000;
        let close_message = contract.close_perp_position_message(
            agent_id.clone(),
            "agent-nvda-1".to_string(),
            U128(10_000_000_000),
            0,
            1,
            6,
            1,
            close_expiry,
        );
        let owner_close_signature = owner_key.sign(close_message.as_bytes());
        contract.close_perp_position(
            agent_id.clone(),
            "agent-nvda-1".to_string(),
            U128(10_000_000_000),
            0,
            1,
            6,
            1,
            close_expiry,
            Base64VecU8::from(owner_close_signature.to_bytes().to_vec()),
            Base64VecU8::from(Vec::new()),
        );
        assert!(contract
            .perp_position(agent_id.clone(), "agent-nvda-1".to_string())
            .is_none());
        assert_eq!(contract.agent_open_notional(agent_id.clone()), U128(0));
        assert_eq!(contract.agent_reserved_margin(agent_id.clone()), U128(0));
        assert_eq!(contract.agent_daily_loss(agent_id), U128(600));
        assert_eq!(contract.balance_of(USER.to_string()), U128(999_400));
    }

    #[test]
    fn active_agent_perp_close_returns_collateral_to_agent_record() {
        let mut contract = setup("a1.near");
        let owner_key = SigningKey::from_bytes(&[141; 32]);
        let agent_key = SigningKey::from_bytes(&[142; 32]);
        let quote_key = SigningKey::from_bytes(&[143; 32]);
        configure_test_perp(&mut contract, &quote_key);
        let agent_id = setup_agent(&mut contract, &owner_key, &agent_key);
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        fund_record(&mut contract, USER, 1_000_000, "active-agent-owner");
        fund_test_agent(&mut contract, &owner_key, &agent_id, 1_000_000);
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            100_000_000,
            "active-agent-vault",
        );
        publish_test_price(&mut contract, U128(10_000_000_000), 1);

        let expiry = NOW_MS + 30_000;
        let open_message = contract.trade_quote_message(
            agent_id.clone(),
            "active-agent-position".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            100,
            1,
            6,
            1,
            expiry,
        );
        let open_signature = agent_key.sign(open_message.as_bytes());
        let open_quote_signature = quote_key.sign(open_message.as_bytes());
        contract.open_perp_position(
            agent_id.clone(),
            "active-agent-position".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            20_000,
            U128(10_000_000_000),
            100,
            1,
            6,
            1,
            expiry,
            Base64VecU8::from(open_signature.to_bytes().to_vec()),
            Base64VecU8::from(open_quote_signature.to_bytes().to_vec()),
        );
        assert_eq!(contract.balance_of(agent_id.clone()), U128(749_700));

        let close_message = contract.close_perp_position_message(
            agent_id.clone(),
            "active-agent-position".to_string(),
            U128(10_000_000_000),
            0,
            1,
            6,
            2,
            expiry,
        );
        let close_signature = agent_key.sign(close_message.as_bytes());
        let close_quote_signature = quote_key.sign(close_message.as_bytes());
        contract.close_perp_position(
            agent_id.clone(),
            "active-agent-position".to_string(),
            U128(10_000_000_000),
            0,
            1,
            6,
            2,
            expiry,
            Base64VecU8::from(close_signature.to_bytes().to_vec()),
            Base64VecU8::from(close_quote_signature.to_bytes().to_vec()),
        );

        assert_eq!(contract.balance_of(agent_id), U128(999_400));
        assert_eq!(contract.balance_of(USER.to_string()), U128(0));
    }

    #[test]
    fn liquidation_closes_underwater_position_and_pays_registered_keeper() {
        let mut contract = setup("a1.near");
        let owner_key = SigningKey::from_bytes(&[94; 32]);
        let quote_key = SigningKey::from_bytes(&[95; 32]);
        let keeper_key = SigningKey::from_bytes(&[96; 32]);
        configure_test_perp(&mut contract, &quote_key);
        register_solana_owner_for(&mut contract, USER, &owner_key);
        register_solana_owner_for(&mut contract, "near:keeper.near", &keeper_key);
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        fund_record(&mut contract, USER, 1_000_000, "liq-user");
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            100_000_000,
            "liq-vault",
        );
        publish_test_price(&mut contract, U128(10_000_000_000), 1);
        let expiry = NOW_MS + 30_000;
        let message = contract.trade_quote_message(
            USER.to_string(),
            "liq-nvda".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            50_000,
            U128(10_000_000_000),
            100,
            1,
            6,
            1,
            expiry,
        );
        let owner_signature = owner_key.sign(message.as_bytes());
        let quote_signature = quote_key.sign(message.as_bytes());
        contract.open_perp_position(
            USER.to_string(),
            "liq-nvda".to_string(),
            "NVDA".to_string(),
            true,
            U128(500_000),
            50_000,
            U128(10_000_000_000),
            100,
            1,
            6,
            1,
            expiry,
            Base64VecU8::from(owner_signature.to_bytes().to_vec()),
            Base64VecU8::from(quote_signature.to_bytes().to_vec()),
        );
        write_market_price(&MarketPrice {
            market_id: "NVDA".to_string(),
            price: U128(8_040_000_000),
            observed_at_ms: NOW_MS,
            sequence: 2,
        });
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id("keeper.near".parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        let reward = contract.liquidate_perp_position(
            USER.to_string(),
            "liq-nvda".to_string(),
            "near:keeper.near".to_string(),
        );
        assert!(reward.0 > 0);
        assert!(contract
            .perp_position(USER.to_string(), "liq-nvda".to_string())
            .is_none());
        assert_eq!(contract.vault_reserved_payout(), U128(0));
        assert_eq!(contract.perp_total_margin(), U128(0));
        assert!(!contract.perp_paused());
        assert_eq!(contract.perp_bad_debt(), U128(0));
    }

    #[test]
    fn funding_charges_the_crowded_side_and_credits_vault() {
        let mut contract = setup(TOKEN);
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            100_000_000,
            "funding-vault",
        );
        write_perp_exposure(
            &PerpExposure {
                long_notional_usdc: U128(1_000_000),
                short_notional_usdc: U128(0),
                funding_updated_at_ms: NOW_MS,
                ..PerpExposure::default()
            },
            "NVDA",
        );
        let mut position = PerpPosition {
            account_id: USER.to_string(),
            position_id: "funding-position".to_string(),
            market_id: "NVDA".to_string(),
            is_long: true,
            notional_usdc: U128(1_000_000),
            margin_usdc: U128(100_000),
            entry_price: U128(10_000_000_000),
            opened_at_ms: NOW_MS,
            funding_index: U128(0),
            funding_remainder: 0,
        };
        write_perp_position(&USER.to_string(), &position.position_id, &position);
        write_u128_storage(PERP_TOTAL_MARGIN_KEY, 100_000);
        let vault_before = contract.vault_balance().0;
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp((NOW_MS + 60 * 60 * 1000) * 1_000_000);
        testing_env!(context.build());
        settle_position_funding(&mut position, &mut contract.balances);
        assert_eq!(position.margin_usdc, U128(99_900));
        assert_eq!(contract.vault_balance().0, vault_before + 100);
        assert_eq!(contract.perp_total_margin(), U128(99_900));
    }

    #[test]
    fn single_eoa_signer_can_publish_oracle_attestation() {
        let mut contract = setup("a1.near");
        configure_test_perp(&mut contract, &SigningKey::from_bytes(&[97; 32]));
        let oracle_secret = SecretKey::from_slice(&[141; 32]).unwrap();
        let signer = test_eoa_address(&oracle_secret);
        contract.configure_oracle_signer(signer);
        let attestation = OracleAttestation {
            market_id: "NVDA".to_string(),
            price: U128(10_000_000_000),
            spot_price: U128(10_000_000_000),
            confidence_bps: 10,
            observed_at_ms: NOW_MS,
            sequence: 1,
            source_round: "round-1".to_string(),
            expires_at_ms: NOW_MS + 30_000,
        };
        let message = contract.oracle_attestation_message(attestation.clone());
        contract.publish_market_price_attestation(
            attestation,
            test_eoa_signature(&oracle_secret, &message),
        );
        assert_eq!(
            read_market_price("NVDA").unwrap().price,
            U128(10_000_000_000)
        );
        assert!(!contract.oracle_health("NVDA".to_string()).circuit_broken);
    }

    #[test]
    fn oracle_signer_rotation_waits_for_configuration_delay() {
        let mut contract = setup("a1.near");
        let first = SecretKey::from_slice(&[146; 32]).unwrap();
        let second = SecretKey::from_slice(&[147; 32]).unwrap();
        let first_address = test_eoa_address(&first);
        let second_address = test_eoa_address(&second);
        contract.configure_oracle_signer(first_address);
        contract.configure_oracle_signer(second_address);

        assert_eq!(contract.oracle_signer(), Some(first_address));
        let ready_at = read_u64_storage(PERP_PENDING_ORACLE_SIGNER_AT_KEY);
        assert_eq!(ready_at, NOW_MS + PERP_ORACLE_SIGNER_DELAY_MS);

        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id("a1.near".parse().unwrap())
            .block_timestamp(ready_at * 1_000_000);
        testing_env!(context.build());
        contract.apply_oracle_signer();
        assert_eq!(contract.oracle_signer(), Some(second_address));
    }

    #[test]
    fn single_eoa_oracle_confirms_large_price_move_and_recovers_market() {
        let mut contract = setup("a1.near");
        configure_test_perp(&mut contract, &SigningKey::from_bytes(&[97; 32]));
        let oracle_secret = SecretKey::from_slice(&[142; 32]).unwrap();
        contract.configure_oracle_signer(test_eoa_address(&oracle_secret));

        let publish =
            |contract: &mut SharedLedger, price: u128, sequence: u64, round: &str, at_ms: u64| {
                let attestation = OracleAttestation {
                    market_id: "NVDA".to_string(),
                    price: U128(price),
                    spot_price: U128(price),
                    confidence_bps: 10,
                    observed_at_ms: at_ms,
                    sequence,
                    source_round: round.to_string(),
                    expires_at_ms: at_ms + 30_000,
                };
                let message = contract.oracle_attestation_message(attestation.clone());
                contract.publish_market_price_attestation(
                    attestation,
                    test_eoa_signature(&oracle_secret, &message),
                );
            };

        publish(&mut contract, 10_000_000_000, 1, "stable-1", NOW_MS);
        publish(&mut contract, 13_000_000_000, 2, "jump-2", NOW_MS);
        assert!(contract.oracle_health("NVDA".to_string()).circuit_broken);
        assert_eq!(
            read_market_price("NVDA").unwrap().price,
            U128(10_000_000_000)
        );

        publish(&mut contract, 13_000_000_000, 3, "jump-2", NOW_MS);
        assert!(contract.oracle_health("NVDA".to_string()).circuit_broken);
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp((NOW_MS + PERP_ORACLE_JUMP_CONFIRM_DELAY_MS) * 1_000_000);
        testing_env!(context.build());
        publish(
            &mut contract,
            13_000_000_000,
            4,
            "confirm-4",
            NOW_MS + PERP_ORACLE_JUMP_CONFIRM_DELAY_MS,
        );
        assert!(!contract.oracle_health("NVDA".to_string()).circuit_broken);
        assert_eq!(
            read_market_price("NVDA").unwrap().price,
            U128(13_000_000_000)
        );
    }

    #[test]
    #[should_panic(expected = "oracle EOA signature invalid")]
    fn oracle_attestation_rejects_signature_from_other_eoa() {
        let mut contract = setup("a1.near");
        configure_test_perp(&mut contract, &SigningKey::from_bytes(&[97; 32]));
        let configured_secret = SecretKey::from_slice(&[143; 32]).unwrap();
        let attacker_secret = SecretKey::from_slice(&[144; 32]).unwrap();
        contract.configure_oracle_signer(test_eoa_address(&configured_secret));
        let attestation = OracleAttestation {
            market_id: "NVDA".to_string(),
            price: U128(10_000_000_000),
            spot_price: U128(10_000_000_000),
            confidence_bps: 10,
            observed_at_ms: NOW_MS,
            sequence: 1,
            source_round: "round-1".to_string(),
            expires_at_ms: NOW_MS + 30_000,
        };
        let message = contract.oracle_attestation_message(attestation.clone());
        contract.publish_market_price_attestation(
            attestation,
            test_eoa_signature(&attacker_secret, &message),
        );
    }

    #[test]
    #[should_panic(expected = "oracle mark unsafe")]
    fn unsafe_oracle_mark_cannot_be_used_for_margin_or_pnl_settlement() {
        let mut contract = setup("a1.near");
        configure_test_perp(&mut contract, &SigningKey::from_bytes(&[97; 32]));
        write_market_price(&MarketPrice {
            market_id: "NVDA".to_string(),
            price: U128(10_000_000_000),
            observed_at_ms: NOW_MS,
            sequence: 1,
        });
        write_oracle_health(
            "NVDA",
            &OracleHealth {
                circuit_broken: true,
                ..OracleHealth::default()
            },
        );
        require_healthy_market_price("NVDA");
    }

    #[test]
    #[should_panic(expected = "vault NAV is zero while LP shares remain outstanding")]
    fn lp_cannot_mint_shares_against_zero_nav_with_old_shares_outstanding() {
        let mut contract = setup(TOKEN);
        let owner_key = SigningKey::from_bytes(&[145; 32]);
        register_solana_owner_for(&mut contract, USER, &owner_key);
        fund_record(&mut contract, USER, 10_000, "zero-nav-lp-user");
        write_lp_shares(USER, 100);
        write_u128_storage(LP_TOTAL_SHARES_KEY, 100);
        let expiry = EXPIRY_MS;
        let message = format!(
            "afterbell:v1\ncontract:a1.near\naction:deposit_vault\naccount:{}\namount:{}\nnonce:1\nexpires_at_ms:{}",
            USER, 10_000, expiry
        );
        contract.deposit_vault(
            USER.to_string(),
            U128(10_000),
            1,
            expiry,
            Base64VecU8::from(owner_key.sign(message.as_bytes()).to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "legacy agent market trading is disabled")]
    fn legacy_agent_market_open_cannot_bypass_perp_risk_controls() {
        let mut contract = setup("a1.near");
        contract.open_agent_market_position(
            "agent:near:test-user:bot".to_string(),
            "legacy-position".to_string(),
            "NVDA".to_string(),
            true,
            U128(100_000),
            20_000,
            U128(10_000_000_000),
            1,
            100,
            1,
            EXPIRY_MS,
            Base64VecU8::from(vec![0; 64]),
        );
    }

    #[test]
    fn risk_increasing_market_config_waits_for_delay() {
        let mut contract = setup("a1.near");
        configure_test_perp(&mut contract, &SigningKey::from_bytes(&[99; 32]));
        let mut config = contract.perp_market("NVDA".to_string()).unwrap();
        config.maintenance_margin_bps = 400;
        contract.configure_perp_market(config.clone());
        assert_eq!(
            contract
                .perp_market("NVDA".to_string())
                .unwrap()
                .maintenance_margin_bps,
            500
        );
        assert_eq!(
            read_u64_storage(&perp_market_time_key("NVDA")),
            NOW_MS + PERP_MARKET_RISK_DELAY_MS
        );
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id("a1.near".parse().unwrap())
            .block_timestamp((NOW_MS + PERP_MARKET_RISK_DELAY_MS) * 1_000_000);
        testing_env!(context.build());
        contract.apply_perp_market_config("NVDA".to_string());
        assert_eq!(
            contract
                .perp_market("NVDA".to_string())
                .unwrap()
                .maintenance_margin_bps,
            400
        );
    }

    #[test]
    fn enabling_a_new_market_waits_for_delay() {
        let mut contract = setup("a1.near");
        contract.configure_perp_market(PerpMarketConfig {
            market_id: "NVDA".to_string(),
            enabled: true,
            close_only: false,
            max_leverage_bps: 50_000,
            maintenance_margin_bps: 500,
            max_execution_deviation_bps: 1_000,
            max_oracle_confidence_bps: 100,
            max_position_usdc: U128(1_000_000),
            max_market_oi_bps: 1_000,
            max_market_skew_bps: 300,
        });
        // Repeating the exact staged config cannot treat it as an active
        // baseline and activate it early.
        contract.configure_perp_market(PerpMarketConfig {
            market_id: "NVDA".to_string(),
            enabled: true,
            close_only: false,
            max_leverage_bps: 50_000,
            maintenance_margin_bps: 500,
            max_execution_deviation_bps: 1_000,
            max_oracle_confidence_bps: 100,
            max_position_usdc: U128(1_000_000),
            max_market_oi_bps: 1_000,
            max_market_skew_bps: 300,
        });

        assert!(contract.perp_market("NVDA".to_string()).is_none());
        assert!(!contract.perp_market_ids().contains(&"NVDA".to_string()));
        let ready_at = read_u64_storage(&perp_market_time_key("NVDA"));
        assert_eq!(ready_at, NOW_MS + PERP_MARKET_RISK_DELAY_MS);

        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id("a1.near".parse().unwrap())
            .block_timestamp(ready_at * 1_000_000);
        testing_env!(context.build());
        contract.apply_perp_market_config("NVDA".to_string());
        assert!(contract.perp_market_ids().contains(&"NVDA".to_string()));
        assert!(contract.perp_market("NVDA".to_string()).unwrap().enabled);
    }

    #[test]
    fn quote_signer_rotation_waits_one_hour() {
        let mut contract = setup(TOKEN);
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id("a1.near".parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        let first = SigningKey::from_bytes(&[101; 32]);
        let second = SigningKey::from_bytes(&[102; 32]);
        contract.set_perp_quote_signer(OwnerKey::SolanaEd25519 {
            public_key: first.verifying_key().to_bytes(),
        });
        contract.set_perp_quote_signer(OwnerKey::SolanaEd25519 {
            public_key: second.verifying_key().to_bytes(),
        });

        assert_eq!(
            contract.perp_quote_signer(),
            Some(OwnerKey::SolanaEd25519 {
                public_key: first.verifying_key().to_bytes(),
            })
        );
        let ready_at = read_u64_storage(PERP_PENDING_QUOTE_SIGNER_AT_KEY);
        assert_eq!(ready_at, NOW_MS + PERP_QUOTE_SIGNER_DELAY_MS);

        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id("a1.near".parse().unwrap())
            .block_timestamp(ready_at * 1_000_000);
        testing_env!(context.build());
        contract.apply_perp_quote_signer();
        assert_eq!(
            contract.perp_quote_signer(),
            Some(OwnerKey::SolanaEd25519 {
                public_key: second.verifying_key().to_bytes(),
            })
        );
    }

    #[test]
    fn owner_can_cover_bad_debt_before_admin_unpauses() {
        let mut contract = setup(TOKEN);
        let owner_key = SigningKey::from_bytes(&[121; 32]);
        register_solana_owner_for(&mut contract, USER, &owner_key);
        fund_record(&mut contract, USER, 10_000, "debt-cover-user");
        write_u128_storage(PERP_BAD_DEBT_KEY, 5_000);
        write_u64_storage(PERP_PAUSED_KEY, 1);
        let cover_expiry = NOW_MS + 30_000;
        let message =
            contract.bad_debt_cover_message(USER.to_string(), U128(5_000), 1, cover_expiry);
        let signature = owner_key.sign(message.as_bytes());
        contract.cover_perp_bad_debt(
            USER.to_string(),
            U128(5_000),
            1,
            cover_expiry,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
        assert_eq!(contract.perp_bad_debt(), U128(0));
        assert_eq!(contract.balance_of(USER.to_string()), U128(5_000));
        assert_eq!(contract.vault_balance(), U128(5_000));

        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id("a1.near".parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        contract.unpause_perps();
        assert!(!contract.perp_paused());
    }

    #[test]
    fn initial_lp_cannot_capture_unallocated_protocol_vault_equity() {
        let mut contract = setup(TOKEN);
        let owner_key = SigningKey::from_bytes(&[122; 32]);
        register_solana_owner_for(&mut contract, USER, &owner_key);
        fund_record(&mut contract, USER, 1_000_000, "lp-user");
        fund_record(
            &mut contract,
            SHARED_VAULT_RECORD_ID,
            100_000_000,
            "lp-seed",
        );
        let deposit_expiry = EXPIRY_MS;
        let deposit_message = format!(
            "afterbell:v1\ncontract:a1.near\naction:deposit_vault\naccount:{}\namount:{}\nnonce:1\nexpires_at_ms:{}",
            USER, 1_000_000, deposit_expiry
        );
        let deposit_signature = owner_key.sign(deposit_message.as_bytes());
        assert_eq!(
            contract.deposit_vault(
                USER.to_string(),
                U128(1_000_000),
                1,
                deposit_expiry,
                Base64VecU8::from(deposit_signature.to_bytes().to_vec()),
            ),
            U128(1_000_000)
        );
        assert_eq!(
            contract.lp_shares_of(SHARED_VAULT_RECORD_ID.to_string()),
            U128(100_000_000)
        );
        assert_eq!(contract.vault_nav_usdc(), U128(101_000_000));

        let request_message = format!(
            "afterbell:v1\ncontract:a1.near\naction:request_vault_withdrawal\naccount:{}\namount:{}:{}\nnonce:2\nexpires_at_ms:{}",
            USER, 1_000_000, 1_000_000, deposit_expiry
        );
        let request_signature = owner_key.sign(request_message.as_bytes());
        contract.request_vault_withdrawal(
            USER.to_string(),
            U128(1_000_000),
            U128(1_000_000),
            2,
            deposit_expiry,
            Base64VecU8::from(request_signature.to_bytes().to_vec()),
        );
        let cancel_message = format!(
            "afterbell:v1\ncontract:a1.near\naction:cancel_vault_withdrawal\naccount:{}\namount:\nnonce:3\nexpires_at_ms:{}",
            USER, deposit_expiry
        );
        let cancel_signature = owner_key.sign(cancel_message.as_bytes());
        contract.cancel_vault_withdrawal(
            USER.to_string(),
            3,
            deposit_expiry,
            Base64VecU8::from(cancel_signature.to_bytes().to_vec()),
        );
        assert!(read_lp_withdrawal(USER).is_none());
        let request_message = format!(
            "afterbell:v1\ncontract:a1.near\naction:request_vault_withdrawal\naccount:{}\namount:{}:{}\nnonce:4\nexpires_at_ms:{}",
            USER, 1_000_000, 1_000_000, deposit_expiry
        );
        let request_signature = owner_key.sign(request_message.as_bytes());
        contract.request_vault_withdrawal(
            USER.to_string(),
            U128(1_000_000),
            U128(1_000_000),
            4,
            deposit_expiry,
            Base64VecU8::from(request_signature.to_bytes().to_vec()),
        );
        let mut context = VMContextBuilder::new();
        context
            .current_account_id("a1.near".parse().unwrap())
            .predecessor_account_id(TOKEN.parse().unwrap())
            .block_timestamp((NOW_MS + LP_WITHDRAWAL_COOLDOWN_MS) * 1_000_000);
        testing_env!(context.build());
        assert_eq!(
            contract.complete_vault_withdrawal(USER.to_string()),
            U128(1_000_000)
        );
        assert_eq!(contract.balance_of(USER.to_string()), U128(1_000_000));
    }

    #[test]
    fn external_token_transfer_cannot_credit_vault_without_lp_shares() {
        let mut contract = setup(TOKEN);
        let result = contract.ft_on_transfer(
            SENDER.parse().unwrap(),
            U128(1_000_000),
            deposit_msg("direct-vault-deposit", SHARED_VAULT_RECORD_ID),
        );
        assert_unused(result, 1_000_000);
        assert_eq!(contract.vault_balance(), U128(0));
        assert_eq!(read_u128_storage(LP_TOTAL_SHARES_KEY), 0);
    }

    #[test]
    #[should_panic(expected = "protocol admin required")]
    fn strangers_cannot_configure_perp_markets() {
        let mut contract = setup("attacker.near");
        configure_test_perp(&mut contract, &SigningKey::from_bytes(&[83; 32]));
    }

    #[test]
    #[should_panic(expected = "legacy agent market trading is disabled")]
    fn legacy_signed_price_updates_route_is_disabled() {
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
    #[should_panic(expected = "legacy agent market trading is disabled")]
    fn legacy_slippage_open_route_is_disabled() {
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
    #[should_panic(expected = "legacy agent market trading is disabled")]
    fn legacy_market_open_route_is_disabled_without_new_engine_controls() {
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
    #[should_panic(expected = "legacy agent market trading is disabled")]
    fn legacy_market_open_route_cannot_be_used_by_another_agent() {
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
    #[should_panic(expected = "legacy agent market trading is disabled")]
    fn legacy_market_open_route_does_not_accept_amended_position_size() {
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
    #[should_panic(expected = "legacy agent market trading is disabled")]
    fn legacy_second_open_route_is_disabled() {
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
            U128(1_000_000),
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
            U128(1_000_000),
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
            U128(1_000_000),
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
            U128(1_000_000),
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

    fn test_eoa_address(secret: &SecretKey) -> [u8; 20] {
        let public_key = secp256k1::PublicKey::from_secret_key(&Secp256k1::new(), secret);
        let address_hash = env::keccak256_array(&public_key.serialize_uncompressed()[1..]);
        let mut address = [0; 20];
        address.copy_from_slice(&address_hash[12..]);
        address
    }

    fn test_eoa_signature(secret: &SecretKey, message: &str) -> Base64VecU8 {
        let signature = Secp256k1::new().sign_ecdsa_recoverable(
            &Message::from_slice(&eip191_digest(message)).unwrap(),
            secret,
        );
        let (recovery_id, compact) = signature.serialize_compact();
        let mut bytes = compact.to_vec();
        bytes.push(recovery_id.to_i32() as u8 + 27);
        Base64VecU8::from(bytes)
    }

    fn eip191_digest(message: &str) -> [u8; 32] {
        let mut personal_message =
            format!("\x19Ethereum Signed Message:\n{}", message.len()).into_bytes();
        personal_message.extend_from_slice(message.as_bytes());
        env::keccak256_array(&personal_message)
    }
}
