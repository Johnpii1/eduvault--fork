#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger};
use soroban_sdk::{contract, contractimpl, contracttype};
use soroban_sdk::{vec, Bytes, Event, Symbol};

#[contracttype]
#[derive(Clone)]
enum MockRegistryKey {
    Material(BytesN<32>),
}

#[contract]
struct MockRegistry;

#[contractimpl]
impl MockRegistry {
    pub fn set_material(env: Env, material_id: BytesN<32>, material: MaterialRecord) {
        env.storage()
            .persistent()
            .set(&MockRegistryKey::Material(material_id), &material);
    }

    pub fn get_material(
        env: Env,
        material_id: BytesN<32>,
    ) -> Result<MaterialRecord, PurchaseError> {
        env.storage()
            .persistent()
            .get(&MockRegistryKey::Material(material_id))
            .ok_or(PurchaseError::MaterialNotFound)
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct MockTransfer {
    from: Address,
    to: Address,
    amount: i128,
}

#[contracttype]
#[derive(Clone)]
enum MockAssetKey {
    Transfers,
}

#[contract]
struct MockAsset;

#[contractimpl]
impl MockAsset {
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let mut transfers: Vec<MockTransfer> = env
            .storage()
            .persistent()
            .get(&MockAssetKey::Transfers)
            .unwrap_or(vec![&env]);
        transfers.push_back(MockTransfer { from, to, amount });
        env.storage()
            .persistent()
            .set(&MockAssetKey::Transfers, &transfers);
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        let transfers: Vec<MockTransfer> = env
            .storage()
            .persistent()
            .get(&MockAssetKey::Transfers)
            .unwrap_or(vec![&env]);
        let mut balance: i128 = 0;
        let mut i: u32 = 0;
        while i < transfers.len() {
            let t = transfers.get_unchecked(i);
            if t.to == id {
                balance += t.amount;
            }
            if t.from == id {
                balance -= t.amount;
            }
            i += 1;
        }
        balance
    }

    pub fn transfer_count(env: Env) -> u32 {
        let transfers: Vec<MockTransfer> = env
            .storage()
            .persistent()
            .get(&MockAssetKey::Transfers)
            .unwrap_or(vec![&env]);
        transfers.len()
    }

    pub fn transfer_at(env: Env, index: u32) -> MockTransfer {
        let transfers: Vec<MockTransfer> = env
            .storage()
            .persistent()
            .get(&MockAssetKey::Transfers)
            .unwrap_or(vec![&env]);
        transfers.get_unchecked(index)
    }
}

fn bytes32(env: &Env, value: u8) -> BytesN<32> {
    BytesN::from_array(env, &[value; 32])
}

fn sample_transaction_id(env: &Env) -> Bytes {
    Bytes::from_array(env, b"550e8400-e29b-41d4-a716-446655440000")
}

fn create_payout_shares_for(
    env: &Env,
    first: &Address,
    first_bps: u32,
    second: &Address,
    second_bps: u32,
) -> Vec<PayoutShare> {
    vec![
        env,
        PayoutShare {
            recipient: first.clone(),
            share_bps: first_bps,
        },
        PayoutShare {
            recipient: second.clone(),
            share_bps: second_bps,
        },
    ]
}

fn install_and_init_contract<'a>(
    env: &'a Env,
    admin: &Address,
    registry: &Address,
    treasury: &Address,
    platform_fee_bps: u32,
) -> (Address, PurchaseManagerClient<'a>) {
    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(env, &contract_id);

    client.initialize(admin, registry, treasury, &platform_fee_bps);

    (contract_id, client)
}

fn setup_purchase(env: &Env) -> (Address, PurchaseManagerClient, Address, Address, Address, BytesN<32>, u64) {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(env);
    let buyer = Address::generate(env);
    let creator = Address::generate(env);
    let asset = env.register(MockAsset, ());
    let _asset_client = MockAssetClient::new(env, &asset);

    let material_id = bytes32(env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(env, &registry);
    registry_client.set_material(&material_id, &material);

    let (contract_id, client) = install_and_init_contract(env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(env));

    (contract_id, client, buyer, creator, asset, material_id, purchase_id)
}

// ============== Initialization Tests ==============

#[test]
fn initializes_contract_successfully() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    env.mock_all_auths();

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let config = client.get_platform_config().unwrap();
    assert_eq!(config.registry, registry);
    assert_eq!(config.treasury, treasury);
    assert_eq!(config.platform_fee_bps, 500);
    assert!(!config.paused);
}

#[test]
fn fails_initialize_twice() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    env.mock_all_auths();

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let result = client.try_initialize(&admin, &registry, &treasury, &500);
    assert_eq!(result, Err(Ok(PurchaseError::AlreadyInitialized)));
}

// ============== Settlement State Tests ==============

#[test]
fn purchase_creates_settlement_in_pending_state() {
    let env = Env::default();
    let (_contract_id, client, _buyer, _creator, _asset, _material_id, purchase_id) = setup_purchase(&env);

    let settlement = client.get_settlement(&purchase_id).unwrap();
    assert_eq!(settlement.state, SettlementState::Pending);
    assert_eq!(settlement.purchase_id, purchase_id);
    assert!(settlement.disputed_ledger.is_none());
    assert!(settlement.resolved_ledger.is_none());
    assert_eq!(settlement.refunded_amount, 0);
}

#[test]
fn settlement_returns_proper_state() {
    let env = Env::default();
    let (_contract_id, client, _buyer, _creator, _asset, _material_id, purchase_id) = setup_purchase(&env);

    let state = client.get_settlement_state(&purchase_id).unwrap();
    assert_eq!(state, SettlementState::Pending);

    // Not yet settled (terminal)
    assert!(!client.is_settled(&purchase_id));
    assert!(!client.is_refunded(&purchase_id));
}

#[test]
fn settlement_transitions_to_released_on_withdraw() {
    let env = Env::default();
    let (_contract_id, client, _buyer, creator, _asset, _material_id, purchase_id) = setup_purchase(&env);

    // Advance ledger past lock period
    env.ledger().set_sequence_number(36_000);

    // Withdraw payouts
    client.withdraw_payouts(&creator, &purchase_id);

    // Settlement should be Released
    let settlement = client.get_settlement(&purchase_id).unwrap();
    assert_eq!(settlement.state, SettlementState::Released);
    assert!(settlement.resolved_ledger.is_some());

    // Terminal state checks
    assert!(client.is_settled(&purchase_id));
    assert!(!client.is_refunded(&purchase_id));
}

#[test]
fn settlement_transitions_to_refunded_on_admin_refund() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Refund via admin route (uses PurchaseBuyer mapping)
    let result = client.try_refund_purchase(&admin, &purchase_id);
    assert!(result.is_ok());

    // Settlement should be Refunded
    let settlement = client.get_settlement(&purchase_id).unwrap();
    assert_eq!(settlement.state, SettlementState::Refunded);
    assert!(settlement.resolved_ledger.is_some());
    assert!(settlement.refunded_amount > 0);

    // Terminal state checks
    assert!(client.is_settled(&purchase_id));
    assert!(client.is_refunded(&purchase_id));

    // Entitlement should be revoked
    assert!(!client.has_entitlement(&material_id, &buyer));
}

#[test]
fn withdraw_fails_when_settlement_not_pending() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // First refund the purchase
    client.refund_purchase(&admin, &purchase_id);

    // Now try to withdraw — should fail with EscrowAlreadyClaimed (checked before settlement)
    env.ledger().set_sequence_number(36_000);
    let result = client.try_withdraw_payouts(&creator, &purchase_id);
    assert_eq!(result, Err(Ok(PurchaseError::EscrowAlreadyClaimed)));
}

#[test]
fn refund_fails_when_settlement_not_pending() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // First refund
    client.refund_purchase(&admin, &purchase_id);

    // Second refund should fail
    let result = client.try_refund_purchase(&admin, &purchase_id);
    assert_eq!(result, Err(Ok(PurchaseError::RefundNotAllowed)));
}

#[test]
fn is_escrow_releasable_checks_settlement() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // After lock period, should be releasable
    env.ledger().set_sequence_number(36_000);
    assert!(client.is_escrow_releasable(&purchase_id));

    // Refund, then it should not be releasable
    client.refund_purchase(&admin, &purchase_id);
    assert!(!client.is_escrow_releasable(&purchase_id));
}

// ============== Dispute Tests ==============

#[test]
fn buyer_can_open_dispute_within_window() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Open dispute within window
    let reason = Bytes::from_array(&env, b"Material does not match description");
    let result = client.try_open_dispute(&buyer, &purchase_id, &reason);
    assert!(result.is_ok());

    // Settlement should now be Disputed
    let settlement = client.get_settlement(&purchase_id).unwrap();
    assert_eq!(settlement.state, SettlementState::Disputed);
    assert!(settlement.disputed_ledger.is_some());
}

#[test]
fn dispute_window_expires_after_threshold() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Advance past dispute window (30,000 ledgers)
    env.ledger().set_sequence_number(31_000);

    let reason = Bytes::from_array(&env, b"Too late to dispute");
    let result = client.try_open_dispute(&buyer, &purchase_id, &reason);
    assert_eq!(result, Err(Ok(PurchaseError::DisputeWindowExpired)));
}

#[test]
fn dispute_requires_non_empty_reason() {
    let env = Env::default();
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) = setup_purchase(&env);

    let empty_reason = Bytes::new(&env);
    let result = client.try_open_dispute(&buyer, &purchase_id, &empty_reason);
    assert_eq!(result, Err(Ok(PurchaseError::InvalidDisputeReason)));
}

#[test]
fn duplicate_dispute_fails() {
    let env = Env::default();
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) = setup_purchase(&env);

    let reason = Bytes::from_array(&env, b"First dispute");
    client.open_dispute(&buyer, &purchase_id, &reason);

    // Second dispute should fail with DisputeAlreadyExists (dispute check happens before settlement check)
    let reason2 = Bytes::from_array(&env, b"Second dispute");
    let result = client.try_open_dispute(&buyer, &purchase_id, &reason2);
    assert_eq!(result, Err(Ok(PurchaseError::DisputeAlreadyExists)));
}

#[test]
fn dispute_cannot_be_opened_on_refunded_purchase() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Refund first
    client.refund_purchase(&admin, &purchase_id);

    // Try to dispute — should fail because entitlement was revoked (NotAuthorized)
    // The entitlement check happens before settlement check, which is correct behavior
    let reason = Bytes::from_array(&env, b"Should not work");
    let result = client.try_open_dispute(&buyer, &purchase_id, &reason);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

// ============== Dispute Resolution Tests ==============

#[test]
fn resolve_dispute_refund_buyer() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());
    let asset_client = MockAssetClient::new(&env, &asset);

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Contract should have seller_net in escrow
    let escrow = client.get_escrow_record(&purchase_id).unwrap();
    assert_eq!(escrow.seller_net, 950_000);

    // Open dispute
    let reason = Bytes::from_array(&env, b"Product not as described");
    client.open_dispute(&buyer, &purchase_id, &reason);

    // Resolve with RefundBuyer — this should transfer funds back to buyer
    let result = client.try_resolve_dispute(&admin, &purchase_id, &DisputeResolution::RefundBuyer);
    assert!(result.is_ok());

    // Settlement should be Refunded
    let settlement = client.get_settlement(&purchase_id).unwrap();
    assert_eq!(settlement.state, SettlementState::Refunded);
    assert!(settlement.refunded_amount > 0);

    // Entitlement should be revoked
    assert!(!client.has_entitlement(&material_id, &buyer));

    // Dispute should have resolution recorded
    let dispute = client.get_dispute(&purchase_id).unwrap();
    assert_eq!(dispute.resolution, DisputeResolution::RefundBuyer);
    assert!(dispute.resolved_ledger.is_some());
}

#[test]
fn resolve_dispute_release_to_creator() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());
    let asset_client = MockAssetClient::new(&env, &asset);

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Open dispute
    let reason = Bytes::from_array(&env, b"Changed mind but admin rules in favor");
    client.open_dispute(&buyer, &purchase_id, &reason);

    // Resolve with ReleaseToCreator
    let result = client.try_resolve_dispute(&admin, &purchase_id, &DisputeResolution::ReleaseToCreator);
    assert!(result.is_ok());

    // Settlement should be Released
    let settlement = client.get_settlement(&purchase_id).unwrap();
    assert_eq!(settlement.state, SettlementState::Released);

    // Entitlement should still be active
    assert!(client.has_entitlement(&material_id, &buyer));

    // Dispute should have resolution recorded
    let dispute = client.get_dispute(&purchase_id).unwrap();
    assert_eq!(dispute.resolution, DisputeResolution::ReleaseToCreator);
}

#[test]
fn resolve_dispute_requires_admin() {
    let env = Env::default();
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) = setup_purchase(&env);

    let reason = Bytes::from_array(&env, b"Dispute reason");
    client.open_dispute(&buyer, &purchase_id, &reason);

    // Non-admin tries to resolve
    let non_admin = Address::generate(&env);
    let result = client.try_resolve_dispute(&non_admin, &purchase_id, &DisputeResolution::RefundBuyer);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn can_query_dispute_record() {
    let env = Env::default();
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) = setup_purchase(&env);

    // No dispute yet
    assert!(client.get_dispute(&purchase_id).is_none());

    let reason = Bytes::from_array(&env, b"Query test dispute");
    client.open_dispute(&buyer, &purchase_id, &reason);

    let dispute = client.get_dispute(&purchase_id).unwrap();
    assert_eq!(dispute.purchase_id, purchase_id);
    assert_eq!(dispute.opener, buyer);
    assert_eq!(dispute.resolution, DisputeResolution::Unresolved);
}

// ============== Refund Purchase Tests ==============

#[test]
fn refund_purchase_via_purchase_buyer_mapping() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Verify purchase → buyer mapping works
    let stored_buyer = client.get_purchase_buyer(&purchase_id).unwrap();
    assert_eq!(stored_buyer, buyer);

    // Refund using the mapping
    let result = client.try_refund_purchase(&admin, &purchase_id);
    assert!(result.is_ok());

    // Entitlement revoked
    assert!(!client.has_entitlement(&material_id, &buyer));
}

#[test]
fn refund_purchase_to_buyer_works() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Refund via explicit buyer
    let result = client.try_refund_purchase_to_buyer(&admin, &purchase_id, &buyer);
    assert!(result.is_ok());

    // Settlement should be Refunded
    let settlement = client.get_settlement(&purchase_id).unwrap();
    assert_eq!(settlement.state, SettlementState::Refunded);
    assert!(settlement.refunded_amount > 0);
}

#[test]
fn refund_purchase_fails_for_wrong_buyer() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let wrong_buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Try to refund with wrong buyer
    let result = client.try_refund_purchase_to_buyer(&admin, &purchase_id, &wrong_buyer);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

// ============== Dual State Constraint Tests ==============

#[test]
fn release_and_refund_are_mutually_exclusive() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Refund first
    client.refund_purchase(&admin, &purchase_id);
    assert!(client.is_refunded(&purchase_id));

    // Try to withdraw after refund — must fail with EscrowAlreadyClaimed (checked before settlement)
    env.ledger().set_sequence_number(36_000);
    let result = client.try_withdraw_payouts(&creator, &purchase_id);
    assert_eq!(result, Err(Ok(PurchaseError::EscrowAlreadyClaimed)));
}

#[test]
fn refunded_entitlement_cannot_pass_check() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Before refund — has entitlement
    assert!(client.has_entitlement(&material_id, &buyer));

    // Refund
    client.refund_purchase(&admin, &purchase_id);

    // After refund — entitlement should be false
    assert!(!client.has_entitlement(&material_id, &buyer));

    // get_entitlement should show active: false
    let entitlement = client.get_entitlement(&material_id, &buyer).unwrap();
    assert!(!entitlement.active);
}

// ============== Admin Abuse Tests ==============

#[test]
fn non_admin_cannot_refund() {
    let env = Env::default();
    let (_contract_id, client, _buyer, _creator, _asset, _material_id, purchase_id) = setup_purchase(&env);

    let non_admin = Address::generate(&env);
    let result = client.try_refund_purchase(&non_admin, &purchase_id);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn non_admin_cannot_resolve_dispute() {
    let env = Env::default();
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) = setup_purchase(&env);

    let reason = Bytes::from_array(&env, b"Test dispute");
    client.open_dispute(&buyer, &purchase_id, &reason);

    let non_admin = Address::generate(&env);
    let result = client.try_resolve_dispute(&non_admin, &purchase_id, &DisputeResolution::RefundBuyer);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

// ============== Event Tests ==============

#[test]
fn dispute_opened_event_emitted() {
    let env = Env::default();
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) = setup_purchase(&env);

    let reason = Bytes::from_array(&env, b"Event test dispute");
    client.open_dispute(&buyer, &purchase_id, &reason);

    let dispute_events = env.events().all();
    let events = dispute_events.events();

    // Find the dispute.opened event
    let dispute_opened_found = events.iter().any(|event| {
        let s = std::format!("{:?}", event);
        s.contains("dispute") && s.contains("opened")
    });
    assert!(dispute_opened_found);
}

#[test]
fn purchase_refunded_event_emitted() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    client.refund_purchase(&admin, &purchase_id);

    // Verify purchase.refunded event exists
    let all_events = env.events().all();
    let events = all_events.events();
    let refunded_found = events.iter().any(|event| {
        let s = std::format!("{:?}", event);
        s.contains("purchase") && s.contains("refunded")
    });
    assert!(refunded_found);
}

// ============== Existing Tests (preserved for compatibility) ==============

#[test]
fn sets_asset_allowed() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let asset = Address::generate(&env);

    env.mock_all_auths();

    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    assert!(!client.is_asset_allowed(&asset));

    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    let asset_policy_events = env.events().all();

    assert!(client.is_asset_allowed(&asset));

    let info = client.get_asset_info(&asset).unwrap();
    assert_eq!(info.kind, AssetKind::Token);
    assert!(info.enabled);

    let events = asset_policy_events.events();
    let last_event = &events[events.len() - 1];
    assert_eq!(
        last_event,
        &AssetPolicyUpdatedEvent {
            asset,
            kind: AssetKind::Token,
            enabled: true,
        }
        .to_xdr(&env, &contract_id)
    );
}

#[test]
fn successful_purchase_creates_entitlement_and_distributes_multiple_payouts() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let creator_payout = Address::generate(&env);
    let collaborator = Address::generate(&env);
    let asset = env.register(MockAsset, ());
    let asset_client = MockAssetClient::new(&env, &asset);

    let material_id = bytes32(&env, 1);
    let payout_shares =
        create_payout_shares_for(&env, &creator_payout, 8_000, &collaborator, 2_000);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares,
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));
    let purchase_events = env.events().all();
    assert_eq!(purchase_id, 0);
    assert!(client.has_entitlement(&material_id, &buyer));
    let entitlement = client.get_entitlement(&material_id, &buyer).unwrap();
    assert_eq!(entitlement.purchase_id, purchase_id);
    assert_eq!(entitlement.amount, 1_000_000);

    assert_eq!(asset_client.transfer_count(), 2);
    assert_eq!(
        asset_client.transfer_at(&0),
        MockTransfer {
            from: buyer.clone(),
            to: treasury.clone(),
            amount: 50_000,
        }
    );
    assert_eq!(
        asset_client.transfer_at(&1),
        MockTransfer {
            from: buyer.clone(),
            to: contract_id.clone(),
            amount: 950_000,
        }
    );

    let escrow = client.get_escrow_record(&purchase_id).unwrap();
    assert_eq!(escrow.purchase_id, purchase_id);
    assert_eq!(escrow.seller_net, 950_000);
    assert!(!escrow.claimed);
    assert_eq!(escrow.payout_shares.len(), 2);

    assert_eq!(purchase_events.events().len(), 3);

    let duplicate = client.try_purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));
    assert_eq!(duplicate, Err(Ok(PurchaseError::EntitlementAlreadyExists)));
}

#[test]
fn withdraw_payouts_succeeds_after_lock_period() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());
    let asset_client = MockAssetClient::new(&env, &asset);

    let material_id = bytes32(&env, 3);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 1_000_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    env.ledger().set_sequence_number(36_000);

    let purchase_id = 0;
    client.withdraw_payouts(&creator, &purchase_id);

    assert_eq!(asset_client.transfer_count(), 3);
    assert_eq!(
        asset_client.transfer_at(&1),
        MockTransfer {
            from: buyer.clone(),
            to: contract_id.clone(),
            amount: 950_000,
        }
    );
    assert_eq!(
        asset_client.transfer_at(&2),
        MockTransfer {
            from: contract_id.clone(),
            to: creator.clone(),
            amount: 950_000,
        }
    );

    let escrow = client.get_escrow_record(&purchase_id).unwrap();
    assert!(escrow.claimed);
}

// ============== Sequence / Boundary Tests ==============

#[test]
fn purchase_id_increments_sequentially() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer_a = Address::generate(&env);
    let buyer_b = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 100_000,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: creator.clone(),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let pid1 = client.purchase(&buyer_a, &material_id, &asset, &100_000, &sample_transaction_id(&env));
    let pid2 = client.purchase(&buyer_b, &material_id, &asset, &100_000, &sample_transaction_id(&env));

    assert_eq!(pid1, 0);
    assert_eq!(pid2, 1);

    assert!(client.get_settlement(&pid1).is_some());
    assert!(client.get_settlement(&pid2).is_some());
    assert!(client.get_purchase_buyer(&pid1).is_some());
    assert!(client.get_purchase_buyer(&pid2).is_some());
}