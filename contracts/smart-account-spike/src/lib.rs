use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::{env, near, AccountId, Gas, NearToken, PanicOnDefault, Promise};
use serde::Serialize;

const NEP_413_MESSAGE_PREFIX: u32 = (1 << 31) + 413;
const MAX_WITHDRAWAL_TTL_MS: u64 = 5 * 60 * 1000;

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub enum OwnerKey {
    NearEd25519 { public_key: [u8; 32] },
    SolanaEd25519 { public_key: [u8; 32] },
    Ethereum { address: [u8; 20] },
}

#[near(serializers = [json])]
pub struct WithdrawalSigningPayload {
    pub message: String,
    pub nonce: Base64VecU8,
    pub recipient: AccountId,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct LedgerWithdrawalArgs {
    receiver_id: AccountId,
    amount: U128,
    nonce: u64,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct SmartAccount {
    ledger_contract: AccountId,
    owner: OwnerKey,
    next_nonce: u64,
}

#[near]
impl SmartAccount {
    /// Initialize atomically from the account factory with the wallet owner.
    #[init]
    pub fn new(ledger_contract: AccountId, owner: OwnerKey) -> Self {
        Self {
            ledger_contract,
            owner,
            next_nonce: 1,
        }
    }

    pub fn withdrawal_nonce(&self) -> u64 {
        self.next_nonce
    }

    /// Returns the exact action message and NEP-413 envelope fields to be
    /// signed by the configured owner wallet.
    pub fn withdrawal_signing_payload(
        &self,
        recipient_id: AccountId,
        amount: U128,
        expires_at_ms: u64,
        max_fee: U128,
    ) -> WithdrawalSigningPayload {
        WithdrawalSigningPayload {
            message: self.withdrawal_message(
                &recipient_id,
                amount,
                self.next_nonce,
                expires_at_ms,
                max_fee,
            ),
            nonce: Base64VecU8::from(nep413_nonce(self.next_nonce).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    /// Verify the owner's chain-specific signature, consume its nonce, and
    /// ask A1 to pay USDC from this smart account's shared-ledger balance.
    pub fn withdraw(
        &mut self,
        recipient_id: AccountId,
        amount: U128,
        nonce: u64,
        expires_at_ms: u64,
        max_fee: U128,
        signature: Base64VecU8,
    ) -> Promise {
        assert!(amount.0 > 0, "withdrawal amount must be greater than zero");
        assert!(max_fee.0 == 0, "withdrawal fees are not supported yet");
        assert!(nonce == self.next_nonce, "invalid withdrawal nonce");

        let now_ms = env::block_timestamp() / 1_000_000;
        assert!(expires_at_ms > now_ms, "withdrawal authorization expired");
        assert!(
            expires_at_ms <= now_ms.saturating_add(MAX_WITHDRAWAL_TTL_MS),
            "withdrawal authorization expiry is too far in the future"
        );

        let message = self.withdrawal_message(&recipient_id, amount, nonce, expires_at_ms, max_fee);
        assert!(
            self.verify_owner_signature(&message, signature.as_ref(), nonce),
            "invalid withdrawal signature"
        );

        self.next_nonce = nonce.checked_add(1).expect("withdrawal nonce overflow");

        let args = near_sdk::serde_json::to_vec(&LedgerWithdrawalArgs {
            receiver_id: recipient_id,
            amount,
            nonce,
        })
        .expect("serialize A1 withdrawal arguments");

        Promise::new(self.ledger_contract.clone()).function_call(
            "withdraw".to_string(),
            args,
            NearToken::from_yoctonear(0),
            Gas::from_tgas(50),
        )
    }
}

impl SmartAccount {
    fn withdrawal_message(
        &self,
        recipient_id: &AccountId,
        amount: U128,
        nonce: u64,
        expires_at_ms: u64,
        max_fee: U128,
    ) -> String {
        let smart_account_id = env::current_account_id();
        format!(
            "afterbell:v1\ncontract:{}\nsmart_account:{}\naction:withdraw\nrecipient:{}\namount:{}\nnonce:{}\nexpires_at_ms:{}\nmax_fee_usdc:{}",
            self.ledger_contract,
            smart_account_id,
            recipient_id,
            amount.0,
            nonce,
            expires_at_ms,
            max_fee.0,
        )
    }

    fn verify_owner_signature(&self, message: &str, signature: &[u8], nonce: u64) -> bool {
        match &self.owner {
            OwnerKey::NearEd25519 { public_key } => {
                let Ok(signature) = <&[u8; 64]>::try_from(signature) else {
                    return false;
                };
                let recipient = env::current_account_id();
                let nonce = nep413_nonce(nonce);
                let digest = nep413_message_hash(message, &recipient, &nonce);
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

/// NEP-413 hashes `u32(2^31 + 413) || Borsh(payload)` with SHA-256;
/// payload fields are message, 32-byte nonce, recipient, and no callback URL.
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

#[cfg(test)]
mod tests {
    use super::*;
    use afterbell_a1::SharedLedger;
    use ed25519_dalek::{Signer, SigningKey};
    use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;
    use secp256k1::{Message, Secp256k1, SecretKey};

    const SMART_ACCOUNT: &str = "alice.smart.near";
    const LEDGER: &str = "ledger.near";
    const RECIPIENT: &str = "recipient.near";
    const NOW_MS: u64 = 1_000_000;
    const EXPIRY_MS: u64 = NOW_MS + 120_000;
    const AMOUNT: U128 = U128(2_500_000);
    const MAX_FEE: U128 = U128(0);

    fn set_context() {
        let mut context = VMContextBuilder::new();
        context
            .current_account_id(SMART_ACCOUNT.parse().unwrap())
            .predecessor_account_id("relayer.near".parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
    }

    fn ed25519_owner() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn call_with_signature(
        account: &mut SmartAccount,
        signing_key: &SigningKey,
        nonce: u64,
        expires_at_ms: u64,
        max_fee: U128,
    ) -> Promise {
        let recipient: AccountId = RECIPIENT.parse().unwrap();
        let message = account.withdrawal_message(&recipient, AMOUNT, nonce, expires_at_ms, max_fee);
        let signature = signing_key.sign(message.as_bytes());
        account.withdraw(
            recipient,
            AMOUNT,
            nonce,
            expires_at_ms,
            max_fee,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        )
    }

    #[test]
    fn solana_owner_signature_authorizes_a1_call_and_consumes_nonce() {
        set_context();
        let key = ed25519_owner();
        let mut account = SmartAccount::new(
            LEDGER.parse().unwrap(),
            OwnerKey::SolanaEd25519 {
                public_key: key.verifying_key().to_bytes(),
            },
        );

        let _promise = call_with_signature(&mut account, &key, 1, EXPIRY_MS, MAX_FEE);

        assert_eq!(account.withdrawal_nonce(), 2);
    }

    #[test]
    fn deposit_owner_signature_and_a1_debit_work_as_one_flow() {
        set_context();
        let key = ed25519_owner();
        let mut account = SmartAccount::new(
            LEDGER.parse().unwrap(),
            OwnerKey::SolanaEd25519 {
                public_key: key.verifying_key().to_bytes(),
            },
        );
        let mut ledger = SharedLedger::new(
            "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1"
                .parse()
                .unwrap(),
        );

        let mut context = VMContextBuilder::new();
        context
            .current_account_id(LEDGER.parse().unwrap())
            .predecessor_account_id(
                "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1"
                    .parse()
                    .unwrap(),
            );
        testing_env!(context.build());
        let deposit = ledger.ft_on_transfer(
            "intents.near".parse().unwrap(),
            U128(5_000_000),
            format!(r#"{{"account_id":"{SMART_ACCOUNT}","deposit_id":"deposit-1"}}"#),
        );
        assert!(matches!(deposit, near_sdk::PromiseOrValue::Value(U128(0))));

        set_context();
        let _smart_account_call = call_with_signature(&mut account, &key, 1, EXPIRY_MS, MAX_FEE);

        let mut context = VMContextBuilder::new();
        context
            .current_account_id(LEDGER.parse().unwrap())
            .predecessor_account_id(SMART_ACCOUNT.parse().unwrap());
        testing_env!(context.build());
        let _usdc_transfer = ledger.withdraw(RECIPIENT.parse().unwrap(), AMOUNT, 1);

        assert_eq!(
            ledger.balance_of(SMART_ACCOUNT.to_string()),
            U128(2_500_000)
        );
        assert_eq!(ledger.withdrawal_nonce(SMART_ACCOUNT.parse().unwrap()), 1);
        assert_eq!(account.withdrawal_nonce(), 2);
    }

    #[test]
    fn near_owner_signature_uses_nep413_payload() {
        set_context();
        let key = ed25519_owner();
        let mut account = SmartAccount::new(
            LEDGER.parse().unwrap(),
            OwnerKey::NearEd25519 {
                public_key: key.verifying_key().to_bytes(),
            },
        );
        let recipient: AccountId = RECIPIENT.parse().unwrap();
        let message = account.withdrawal_message(&recipient, AMOUNT, 1, EXPIRY_MS, MAX_FEE);
        let nonce = nep413_nonce(1);
        let digest = nep413_message_hash(&message, &SMART_ACCOUNT.parse().unwrap(), &nonce);
        let signature = key.sign(&digest);

        let _promise = account.withdraw(
            recipient,
            AMOUNT,
            1,
            EXPIRY_MS,
            MAX_FEE,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );

        assert_eq!(account.withdrawal_nonce(), 2);
    }

    #[test]
    fn ethereum_personal_sign_signature_authorizes_withdrawal() {
        set_context();
        let secret = SecretKey::from_slice(&[7; 32]).unwrap();
        let secp = Secp256k1::new();
        let public_key =
            secp256k1::PublicKey::from_secret_key(&secp, &secret).serialize_uncompressed();
        let address_hash = env::keccak256_array(&public_key[1..]);
        let address: [u8; 20] = address_hash[12..].try_into().unwrap();
        let mut account =
            SmartAccount::new(LEDGER.parse().unwrap(), OwnerKey::Ethereum { address });

        let recipient: AccountId = RECIPIENT.parse().unwrap();
        let message = account.withdrawal_message(&recipient, AMOUNT, 1, EXPIRY_MS, MAX_FEE);
        let mut personal_message =
            format!("\x19Ethereum Signed Message:\n{}", message.len()).into_bytes();
        personal_message.extend_from_slice(message.as_bytes());
        let digest = env::keccak256_array(&personal_message);
        let message_hash = Message::from_slice(&digest).unwrap();
        let (recovery_id, raw_signature) = secp
            .sign_ecdsa_recoverable(&message_hash, &secret)
            .serialize_compact();
        let mut signature = raw_signature.to_vec();
        signature.push(recovery_id.to_i32() as u8);

        let _promise = account.withdraw(
            recipient,
            AMOUNT,
            1,
            EXPIRY_MS,
            MAX_FEE,
            Base64VecU8::from(signature),
        );

        assert_eq!(account.withdrawal_nonce(), 2);
    }

    #[test]
    #[should_panic(expected = "invalid withdrawal signature")]
    fn rejects_signature_for_a_different_recipient() {
        set_context();
        let key = ed25519_owner();
        let mut account = SmartAccount::new(
            LEDGER.parse().unwrap(),
            OwnerKey::SolanaEd25519 {
                public_key: key.verifying_key().to_bytes(),
            },
        );
        let recipient: AccountId = RECIPIENT.parse().unwrap();
        let message = account.withdrawal_message(&recipient, AMOUNT, 1, EXPIRY_MS, MAX_FEE);
        let signature = key.sign(message.as_bytes());

        let _promise = account.withdraw(
            "attacker.near".parse().unwrap(),
            AMOUNT,
            1,
            EXPIRY_MS,
            MAX_FEE,
            Base64VecU8::from(signature.to_bytes().to_vec()),
        );
    }

    #[test]
    #[should_panic(expected = "invalid withdrawal nonce")]
    fn rejects_replaying_a_valid_owner_signature() {
        set_context();
        let key = ed25519_owner();
        let mut account = SmartAccount::new(
            LEDGER.parse().unwrap(),
            OwnerKey::SolanaEd25519 {
                public_key: key.verifying_key().to_bytes(),
            },
        );
        let _first = call_with_signature(&mut account, &key, 1, EXPIRY_MS, MAX_FEE);
        let _replay = call_with_signature(&mut account, &key, 1, EXPIRY_MS, MAX_FEE);
    }

    #[test]
    #[should_panic(expected = "withdrawal authorization expired")]
    fn rejects_expired_owner_signature() {
        set_context();
        let key = ed25519_owner();
        let mut account = SmartAccount::new(
            LEDGER.parse().unwrap(),
            OwnerKey::SolanaEd25519 {
                public_key: key.verifying_key().to_bytes(),
            },
        );
        let _promise = call_with_signature(&mut account, &key, 1, NOW_MS, MAX_FEE);
    }
}
