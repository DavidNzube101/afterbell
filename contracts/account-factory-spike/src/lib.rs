use near_sdk::json_types::Base64VecU8;
use near_sdk::{env, near, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError};
use serde::Serialize;

const NEP_413_MESSAGE_PREFIX: u32 = (1 << 31) + 413;
const MAX_CREATION_TTL_MS: u64 = 5 * 60 * 1000;
const CREATE_NONCE: u64 = 1;

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub enum OwnerKey {
    NearEd25519 { public_key: [u8; 32] },
    SolanaEd25519 { public_key: [u8; 32] },
    Ethereum { address: [u8; 20] },
}

#[near(serializers = [json])]
pub struct AccountCreationSigningPayload {
    pub message: String,
    pub nonce: Base64VecU8,
    pub recipient: AccountId,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct SmartAccountInitArgs {
    ledger_contract: AccountId,
    owner: OwnerKey,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct CreationCallbackArgs {
    refund_to: AccountId,
    amount: near_sdk::json_types::U128,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct AccountFactory {
    ledger_contract: AccountId,
    smart_account_wasm: Vec<u8>,
    minimum_account_deposit_yocto: u128,
}

#[near]
impl AccountFactory {
    /// The account code and A1 target are fixed at deployment and cannot be
    /// changed by a factory administrator later.
    #[init]
    pub fn new(
        ledger_contract: AccountId,
        smart_account_wasm: Base64VecU8,
        minimum_account_deposit: NearToken,
    ) -> Self {
        assert!(
            !smart_account_wasm.0.is_empty(),
            "smart-account code is required"
        );
        assert!(
            minimum_account_deposit.as_yoctonear() > 0,
            "minimum account deposit must be positive"
        );
        Self {
            ledger_contract,
            smart_account_wasm: smart_account_wasm.0,
            minimum_account_deposit_yocto: minimum_account_deposit.as_yoctonear(),
        }
    }

    pub fn account_creation_signing_payload(
        &self,
        account_id: AccountId,
        owner: OwnerKey,
        expires_at_ms: u64,
    ) -> AccountCreationSigningPayload {
        AccountCreationSigningPayload {
            message: self.creation_message(&account_id, &owner, expires_at_ms),
            nonce: Base64VecU8::from(nep413_nonce(CREATE_NONCE).to_vec()),
            recipient: env::current_account_id(),
        }
    }

    /// Create a direct child account, deploy the pinned smart-account code,
    /// and initialize its owner and A1 target in one atomic NEAR receipt.
    #[payable]
    pub fn create_account(
        &mut self,
        account_id: AccountId,
        owner: OwnerKey,
        expires_at_ms: u64,
        signature: Base64VecU8,
    ) -> Promise {
        let factory_id = env::current_account_id();
        assert!(
            is_direct_child(&account_id, &factory_id),
            "account must be a direct child of this factory"
        );
        let attached_deposit = env::attached_deposit();
        assert!(
            attached_deposit.as_yoctonear() >= self.minimum_account_deposit_yocto,
            "attached deposit is below the factory minimum"
        );

        let now_ms = env::block_timestamp() / 1_000_000;
        assert!(
            expires_at_ms > now_ms,
            "account creation authorization expired"
        );
        assert!(
            expires_at_ms <= now_ms.saturating_add(MAX_CREATION_TTL_MS),
            "account creation authorization expiry is too far in the future"
        );
        let message = self.creation_message(&account_id, &owner, expires_at_ms);
        assert!(
            verify_owner_signature(
                &owner,
                &message,
                signature.as_ref(),
                CREATE_NONCE,
                &factory_id,
            ),
            "invalid owner signature"
        );

        let init_args = near_sdk::serde_json::to_vec(&SmartAccountInitArgs {
            ledger_contract: self.ledger_contract.clone(),
            owner,
        })
        .expect("serialize smart-account initializer arguments");

        let callback_args = near_sdk::serde_json::to_vec(&CreationCallbackArgs {
            refund_to: env::predecessor_account_id(),
            amount: near_sdk::json_types::U128(attached_deposit.as_yoctonear()),
        })
        .expect("serialize creation callback arguments");

        Promise::new(account_id)
            .create_account()
            .transfer(attached_deposit)
            .deploy_contract(self.smart_account_wasm.clone())
            .function_call(
                "new".to_string(),
                init_args,
                NearToken::from_yoctonear(0),
                Gas::from_tgas(30),
            )
            .then(Promise::new(factory_id).function_call(
                "on_create_account_complete".to_string(),
                callback_args,
                NearToken::from_yoctonear(0),
                Gas::from_tgas(10),
            ))
    }

    /// Return the attached account deposit if account creation or initialization
    /// fails. The failed child-account action batch rolls back before this
    /// callback runs, leaving the deposit in the factory to refund.
    #[private]
    pub fn on_create_account_complete(
        &mut self,
        refund_to: AccountId,
        amount: near_sdk::json_types::U128,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        if result.is_ok() {
            return true;
        }
        Promise::new(refund_to)
            .transfer(NearToken::from_yoctonear(amount.0))
            .detach();
        false
    }
}

impl AccountFactory {
    fn creation_message(
        &self,
        account_id: &AccountId,
        owner: &OwnerKey,
        expires_at_ms: u64,
    ) -> String {
        format!(
            "afterbell:v1\ncontract:{}\naction:create_smart_account\naccount:{}\nledger:{}\nowner:{}\nnonce:{}\nexpires_at_ms:{}",
            env::current_account_id(),
            account_id,
            self.ledger_contract,
            owner_key_id(owner),
            CREATE_NONCE,
            expires_at_ms,
        )
    }
}

fn is_direct_child(account_id: &AccountId, factory_id: &AccountId) -> bool {
    let Some((label, parent)) = account_id.as_str().split_once('.') else {
        return false;
    };
    !label.is_empty() && !label.contains('.') && parent == factory_id.as_str()
}

fn owner_key_id(owner: &OwnerKey) -> String {
    match owner {
        OwnerKey::NearEd25519 { public_key } => format!("near-ed25519:{}", hex(public_key)),
        OwnerKey::SolanaEd25519 { public_key } => format!("solana-ed25519:{}", hex(public_key)),
        OwnerKey::Ethereum { address } => format!("ethereum:{}", hex(address)),
    }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn verify_owner_signature(
    owner: &OwnerKey,
    message: &str,
    signature: &[u8],
    nonce: u64,
    recipient: &AccountId,
) -> bool {
    match owner {
        OwnerKey::NearEd25519 { public_key } => {
            let Ok(signature) = <&[u8; 64]>::try_from(signature) else {
                return false;
            };
            let nonce = nep413_nonce(nonce);
            let digest = nep413_message_hash(message, recipient, &nonce);
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

fn nep413_message_hash(message: &str, recipient: &AccountId, nonce: &[u8; 32]) -> [u8; 32] {
    let mut payload = NEP_413_MESSAGE_PREFIX.to_le_bytes().to_vec();
    append_borsh_string(&mut payload, message);
    payload.extend_from_slice(nonce);
    append_borsh_string(&mut payload, recipient.as_str());
    payload.push(0);
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
    use ed25519_dalek::{Signer, SigningKey};
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    const FACTORY: &str = "accounts.afterbell.near";
    const CHILD: &str = "alice.accounts.afterbell.near";
    const NOW_MS: u64 = 1_000_000;
    const EXPIRY_MS: u64 = NOW_MS + 60_000;

    fn factory() -> AccountFactory {
        let factory_id: AccountId = FACTORY.parse().unwrap();
        testing_env!(VMContextBuilder::new()
            .current_account_id(factory_id.clone())
            .predecessor_account_id(factory_id)
            .block_timestamp(NOW_MS * 1_000_000)
            .attached_deposit(NearToken::from_near(5))
            .build());
        AccountFactory::new(
            "a1.afterbell.near".parse().unwrap(),
            Base64VecU8(vec![1, 2, 3]),
            NearToken::from_near(1),
        )
    }

    #[test]
    fn signed_creation_binds_child_owner_factory_and_a1() {
        let key = SigningKey::from_bytes(&[8; 32]);
        let owner = OwnerKey::NearEd25519 {
            public_key: key.verifying_key().to_bytes(),
        };
        let mut contract = factory();
        let account_id: AccountId = CHILD.parse().unwrap();
        let message = contract.creation_message(&account_id, &owner, EXPIRY_MS);
        let recipient: AccountId = FACTORY.parse().unwrap();
        let digest = nep413_message_hash(&message, &recipient, &nep413_nonce(CREATE_NONCE));
        let signature = key.sign(&digest);
        let promise = contract.create_account(
            account_id,
            owner,
            EXPIRY_MS,
            Base64VecU8(signature.to_bytes().to_vec()),
        );
        let _ = promise;
    }

    #[test]
    fn failed_child_creation_schedules_refund_to_original_caller() {
        let mut context = VMContextBuilder::new();
        context
            .current_account_id(FACTORY.parse().unwrap())
            .predecessor_account_id(FACTORY.parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000);
        testing_env!(context.build());
        let mut contract = factory();
        let refunded = contract.on_create_account_complete(
            "david.near".parse().unwrap(),
            near_sdk::json_types::U128(5_000_000_000_000_000_000_000_000),
            Err(PromiseError::Failed),
        );
        assert!(!refunded);
        assert!(!near_sdk::test_utils::get_created_receipts().is_empty());
    }

    #[test]
    fn direct_child_check_rejects_foreign_or_nested_accounts() {
        let factory: AccountId = FACTORY.parse().unwrap();
        assert!(is_direct_child(&CHILD.parse().unwrap(), &factory));
        assert!(!is_direct_child(
            &"alice.other.near".parse().unwrap(),
            &factory
        ));
        assert!(!is_direct_child(
            &"nested.alice.accounts.afterbell.near".parse().unwrap(),
            &factory
        ));
    }

    #[test]
    #[should_panic(expected = "invalid owner signature")]
    fn rejects_signature_for_a_different_account() {
        let key = SigningKey::from_bytes(&[8; 32]);
        let owner = OwnerKey::NearEd25519 {
            public_key: key.verifying_key().to_bytes(),
        };
        let mut contract = factory();
        let message = contract.creation_message(&CHILD.parse().unwrap(), &owner, EXPIRY_MS);
        let digest = nep413_message_hash(
            &message,
            &FACTORY.parse().unwrap(),
            &nep413_nonce(CREATE_NONCE),
        );
        let signature = key.sign(&digest);
        let other_account: AccountId = "bob.accounts.afterbell.near".parse().unwrap();
        let _promise = contract.create_account(
            other_account,
            owner,
            EXPIRY_MS,
            Base64VecU8(signature.to_bytes().to_vec()),
        );
    }
}
