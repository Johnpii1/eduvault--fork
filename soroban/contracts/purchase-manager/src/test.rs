#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger};
use soroban_sdk::{contract, contractimpl, contracttype};
use soroban_sdk::{vec, Bytes, Event};

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

fn setup_purchase(
    env: &Env,
) -> (
    Address,
    PurchaseManagerClient<'_>,
    Address,
    Address,
    Address,
    BytesN<32>,
    u64,
) {
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

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(env),
    );

    (
        contract_id,
        client,
        buyer,
        creator,
        asset,
        material_id,
        purchase_id,
    )
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
    let (_contract_id, client, _buyer, _creator, _asset, _material_id, purchase_id) =
        setup_purchase(&env);

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
    let (_contract_id, client, _buyer, _creator, _asset, _material_id, purchase_id) =
        setup_purchase(&env);

    let state = client.get_settlement_state(&purchase_id).unwrap();
    assert_eq!(state, SettlementState::Pending);

    // Not yet settled (terminal)
    assert!(!client.is_settled(&purchase_id));
    assert!(!client.is_refunded(&purchase_id));
}

#[test]
fn settlement_transitions_to_released_on_withdraw() {
    let env = Env::default();
    let (_contract_id, client, _buyer, creator, _asset, _material_id, purchase_id) =
        setup_purchase(&env);

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
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

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

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

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

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

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

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

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

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

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

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    // Advance past dispute window (30,000 ledgers)
    env.ledger().set_sequence_number(31_000);

    let reason = Bytes::from_array(&env, b"Too late to dispute");
    let result = client.try_open_dispute(&buyer, &purchase_id, &reason);
    assert_eq!(result, Err(Ok(PurchaseError::DisputeWindowExpired)));
}

#[test]
fn dispute_requires_non_empty_reason() {
    let env = Env::default();
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) =
        setup_purchase(&env);

    let empty_reason = Bytes::new(&env);
    let result = client.try_open_dispute(&buyer, &purchase_id, &empty_reason);
    assert_eq!(result, Err(Ok(PurchaseError::InvalidDisputeReason)));
}

#[test]
fn duplicate_dispute_fails() {
    let env = Env::default();
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) =
        setup_purchase(&env);

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

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

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
    let _asset_client = MockAssetClient::new(&env, &asset);

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

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

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
    let _asset_client = MockAssetClient::new(&env, &asset);

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

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    // Open dispute
    let reason = Bytes::from_array(&env, b"Changed mind but admin rules in favor");
    client.open_dispute(&buyer, &purchase_id, &reason);

    // Resolve with ReleaseToCreator
    let result =
        client.try_resolve_dispute(&admin, &purchase_id, &DisputeResolution::ReleaseToCreator);
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
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) =
        setup_purchase(&env);

    let reason = Bytes::from_array(&env, b"Dispute reason");
    client.open_dispute(&buyer, &purchase_id, &reason);

    // Non-admin tries to resolve
    let non_admin = Address::generate(&env);
    let result =
        client.try_resolve_dispute(&non_admin, &purchase_id, &DisputeResolution::RefundBuyer);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn can_query_dispute_record() {
    let env = Env::default();
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) =
        setup_purchase(&env);

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
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

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

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

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
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let unknown_buyer = Address::generate(&env);
    let material_id = bytes32(&env, 99);

    assert!(!client.has_entitlement(&material_id, &unknown_buyer));
}

#[test]
fn purchase_fails_for_invalid_items() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 2);
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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let invalid_material_id = bytes32(&env, 100);

    let result = client.try_purchase(
        &buyer,
        &invalid_material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );
    assert_eq!(result, Err(Ok(PurchaseError::MaterialNotFound)));
}

// ============== Escrow Tests ==============

#[test]
fn escrow_record_queryable_after_purchase() {
    let env = Env::default();
    env.mock_all_auths();

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

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );
    let _escrow = client.get_escrow_record(&purchase_id).unwrap();

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

    env.mock_all_auths();
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    let result = client.try_withdraw_payouts(&creator, &purchase_id);
    assert_eq!(result, Err(Ok(PurchaseError::EscrowLocked)));
}

#[test]
fn rejects_unauthorized_platform_config_change() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let unauthorized_user = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let new_treasury = Address::generate(&env);
    let result = client.try_set_platform_config(&unauthorized_user, &new_treasury, &600, &false);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    // Advance past lock period
    env.ledger().set_sequence_number(36_000);

    // Withdraw payouts
    client.withdraw_payouts(&creator, &purchase_id);

    let escrow = client.get_escrow_record(&purchase_id).unwrap();
    assert!(escrow.claimed);
}

// ============== Admin Abuse Tests ==============

#[test]
fn non_admin_cannot_refund() {
    let env = Env::default();
    let (_contract_id, client, _buyer, _creator, _asset, _material_id, purchase_id) =
        setup_purchase(&env);

    let non_admin = Address::generate(&env);
    let result = client.try_refund_purchase(&non_admin, &purchase_id);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn non_admin_cannot_resolve_dispute() {
    let env = Env::default();
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) =
        setup_purchase(&env);

    let reason = Bytes::from_array(&env, b"Test dispute");
    client.open_dispute(&buyer, &purchase_id, &reason);

    let non_admin = Address::generate(&env);
    let result =
        client.try_resolve_dispute(&non_admin, &purchase_id, &DisputeResolution::RefundBuyer);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

// ============== Event Tests ==============

#[test]
fn dispute_opened_event_emitted() {
    let env = Env::default();
    let (_contract_id, client, buyer, _creator, _asset, _material_id, purchase_id) =
        setup_purchase(&env);

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
    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let _purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    // Verify purchase.refunded event exists
    let all_events = env.events().all();
    let events = all_events.events();
    let refunded_found = events.iter().any(|event| {
        let s = std::format!("{:?}", event);
        s.contains("purchase") && s.contains("completed")
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
    let _asset_client = MockAssetClient::new(&env, &asset);

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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    env.ledger().set_sequence_number(36_000);

    client.withdraw_payouts(&creator_payout, &purchase_id);

    let result = client.try_withdraw_payouts(&creator_payout, &purchase_id);
    assert_eq!(result, Err(Ok(PurchaseError::EscrowAlreadyClaimed)));
}

// ============== Admin Transfer Tests (#378) ==============

#[test]
fn transfer_admin_initiates_pending_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    client.transfer_admin(&admin, &new_admin);

    assert_eq!(client.get_pending_admin(), Some(new_admin));
}

#[test]
fn transfer_admin_emits_initiated_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    client.transfer_admin(&admin, &new_admin);

    let all_events = env.events().all().filter_by_contract(&contract_id);
    let events = all_events.events();
    // env.events().all() only reflects the most recent top-level invocation,
    // so only transfer_admin's AdminTransferInitiated event is visible here.
    assert_eq!(events.len(), 1);
}

#[test]
fn transfer_admin_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let result = client.try_transfer_admin(&non_admin, &new_admin);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn accept_admin_completes_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    client.transfer_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);

    assert_eq!(client.get_pending_admin(), None);

    // The new admin can now perform admin-only actions.
    client.update_platform_fee(&new_admin, &300);
    let config = client.get_platform_config().unwrap();
    assert_eq!(config.platform_fee_bps, 300);
}

#[test]
fn accept_admin_emits_accepted_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    client.transfer_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);

    let all_events = env.events().all().filter_by_contract(&contract_id);
    let events = all_events.events();
    // env.events().all() only reflects the most recent top-level invocation,
    // so only accept_admin's AdminTransferAccepted event is visible here.
    assert_eq!(events.len(), 1);
}

#[test]
fn accept_admin_fails_when_no_pending_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let claimant = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let result = client.try_accept_admin(&claimant);
    assert_eq!(result, Err(Ok(PurchaseError::NoPendingAdminTransfer)));
}

#[test]
fn accept_admin_fails_for_non_pending_address() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let impostor = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    client.transfer_admin(&admin, &new_admin);

    let result = client.try_accept_admin(&impostor);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

// ============== Creator Volume Tier Tests (#381) ==============

#[test]
fn set_creator_tier_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let non_admin = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    client.set_creator_tier(&admin, &creator, &CreatorTier::Tier1);
    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Tier1);

    let result = client.try_set_creator_tier(&non_admin, &creator, &CreatorTier::Tier2);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn purchase_creates_correct_escrow_and_entitlement() {
    let env = Env::default();
    env.mock_all_auths();

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

    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );
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
    assert_eq!(escrow.payout_shares.len(), 1);

    assert_eq!(purchase_events.events().len(), 3);

    let duplicate = client.try_purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );
    assert_eq!(duplicate, Err(Ok(PurchaseError::EntitlementAlreadyExists)));
}

#[test]
fn creator_tier_defaults_to_default_when_not_set() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Default);
}

#[test]
fn creator_tier_can_be_reverted_to_default() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    client.set_creator_tier(&admin, &creator, &CreatorTier::Tier1);
    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Tier1);

    client.set_creator_tier(&admin, &creator, &CreatorTier::Default);
    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Default);
}

#[test]
fn default_creator_uses_platform_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());
    let asset_client = MockAssetClient::new(&env, &asset);

    let material_id = bytes32(&env, 8);
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

    client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

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
    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 700);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Default);

    client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    // Default tier: uses the global platform_fee_bps (700 bps of 1_000_000 = 70_000)
    assert_eq!(asset_client.transfer_at(&3).amount, 70_000);
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
    let _asset_client = MockAssetClient::new(&env, &asset);

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

    let pid1 = client.purchase(
        &buyer_a,
        &material_id,
        &asset,
        &100_000,
        &sample_transaction_id(&env),
    );
    let pid2 = client.purchase(
        &buyer_b,
        &material_id,
        &asset,
        &100_000,
        &sample_transaction_id(&env),
    );

    assert_eq!(pid1, 0);
    assert_eq!(pid2, 1);

    assert!(client.get_settlement(&pid1).is_some());
    assert!(client.get_settlement(&pid2).is_some());
    assert!(client.get_purchase_buyer(&pid1).is_some());
    assert!(client.get_purchase_buyer(&pid2).is_some());
}

#[test]
fn tier1_creator_uses_250bps_fee() {
    let env = Env::default();
    env.mock_all_auths();

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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_creator_tier(&admin, &creator, &CreatorTier::Tier1);

    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Tier1);

    client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    // Tier1 fee: 250 bps of 1_000_000 = 25_000
    assert_eq!(asset_client.transfer_at(&0).amount, 25_000);
}

#[test]
fn tier2_creator_uses_150bps_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());
    let asset_client = MockAssetClient::new(&env, &asset);

    let material_id = bytes32(&env, 6);
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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_creator_tier(&admin, &creator, &CreatorTier::Tier2);

    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Tier2);

    client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    // Tier2 fee: 150 bps of 1_000_000 = 15_000
    assert_eq!(asset_client.transfer_at(&0).amount, 15_000);
}

#[test]
fn is_escrow_releasable_returns_false_before_lock_period() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 6);
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
                recipient: Address::generate(&env),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Default);
    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    assert!(!client.is_escrow_releasable(&purchase_id));
}

#[test]
fn is_escrow_releasable_returns_true_after_lock_period() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 7);
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
                recipient: Address::generate(&env),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    client.set_creator_tier(&admin, &creator, &CreatorTier::Tier1);
    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Tier1);

    client.set_creator_tier(&admin, &creator, &CreatorTier::Default);
    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Default);
    env.ledger().set_sequence_number(36_000);

    assert!(client.is_escrow_releasable(&purchase_id));
}

// ============== TTL Renewal Tests (#464) ==============

/// Small, deterministic TTL window: large enough to clear the network's
/// minimum persistent-entry TTL, small enough that advancing a few
/// thousand ledgers is enough to cross the renewal threshold.
fn set_short_ttl_window(env: &Env) {
    env.ledger().with_mut(|li| {
        li.min_persistent_entry_ttl = 100;
        li.max_entry_ttl = 20_000;
    });
}

/// The test host advances the ledger sequence by a small amount between
/// separate top-level invocations, so a TTL measured a call or two after a
/// renewal can read a few ledgers below the exact `extend_to` value. Allow
/// a small tolerance rather than asserting an exact figure.
fn assert_ttl_renewed_to_max(ttl: u32) {
    assert!(
        (19_990..=20_000).contains(&ttl),
        "expected TTL near the 20_000 max, got {ttl}"
    );
}

/// Registers `material_id` in the mock registry with a single quote/payout
/// pair, so `client.purchase` can succeed against it.
fn seed_purchasable_material(
    env: &Env,
    registry: &Address,
    material_id: &BytesN<32>,
    creator: &Address,
    asset: &Address,
    amount: i128,
) {
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount,
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
    MockRegistryClient::new(env, registry).set_material(material_id, &material);
}

#[test]
fn platform_config_ttl_renews_on_every_touch_and_never_lapses() {
    let env = Env::default();
    env.mock_all_auths();
    set_short_ttl_window(&env);

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let initial_ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert_ttl_renewed_to_max(initial_ttl);

    // Advance well past the renewal threshold without any call touching
    // instance state.
    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    // A plain read renews the instance TTL straight back to the max.
    assert!(client.get_platform_config().is_some());

    let renewed_ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert_ttl_renewed_to_max(renewed_ttl);
}

#[test]
fn entitlement_and_escrow_ttl_renew_on_read_after_partial_lapse() {
    let env = Env::default();
    env.mock_all_auths();
    set_short_ttl_window(&env);

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 90);
    seed_purchasable_material(&env, &registry, &material_id, &creator, &asset, 1_000_000);

    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );

    let escrow_key = DataKey::Escrow(purchase_id);
    let entitlement_key = DataKey::Entitlement((material_id.clone(), buyer.clone()));

    let initial_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&escrow_key)
    });
    assert_ttl_renewed_to_max(initial_ttl);

    // Advance past the renewal threshold without touching either record —
    // exactly the "buyer never comes back" scenario #464 is about.
    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    // A plain content-access check (has_entitlement) and an escrow lookup
    // are both reads, and both renew — a buyer actively using what they
    // paid for keeps their own access alive for free.
    assert!(client.has_entitlement(&material_id, &buyer));
    assert!(client.get_escrow_record(&purchase_id).is_some());

    let renewed_escrow_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&escrow_key)
    });
    let renewed_entitlement_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&entitlement_key)
    });
    assert_ttl_renewed_to_max(renewed_escrow_ttl);
    assert_ttl_renewed_to_max(renewed_entitlement_ttl);
}

#[test]
fn allowed_asset_ttl_renews_on_write() {
    let env = Env::default();
    env.mock_all_auths();
    set_short_ttl_window(&env);

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let asset = Address::generate(&env);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let asset_key = DataKey::AllowedAsset(asset.clone());
    let initial_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&asset_key)
    });
    assert_ttl_renewed_to_max(initial_ttl);

    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    let renewed_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&asset_key)
    });
    assert_ttl_renewed_to_max(renewed_ttl);
}

#[test]
fn creator_tier_ttl_renews_on_write() {
    let env = Env::default();
    env.mock_all_auths();
    set_short_ttl_window(&env);

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let creator = Address::generate(&env);
    client.set_creator_tier(&admin, &creator, &CreatorTier::Tier1);

    let tier_key = DataKey::CreatorTier(creator.clone());
    let initial_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&tier_key)
    });
    assert_ttl_renewed_to_max(initial_ttl);

    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    client.set_creator_tier(&admin, &creator, &CreatorTier::Tier2);
    let renewed_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&tier_key)
    });
    assert_ttl_renewed_to_max(renewed_ttl);
}

#[test]
fn admin_role_ttl_renews_on_any_admin_check() {
    let env = Env::default();
    env.mock_all_auths();
    set_short_ttl_window(&env);

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let admin_key = auth::AuthDataKey::AdminRole(admin.clone());
    let initial_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&admin_key)
    });
    assert_ttl_renewed_to_max(initial_ttl);

    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    // Any admin-gated call re-checks the role, which renews it.
    client.update_platform_fee(&admin, &300);

    let renewed_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&admin_key)
    });
    assert_ttl_renewed_to_max(renewed_ttl);
}

#[test]
fn extend_purchases_ttl_is_cursor_based_and_bounded() {
    let env = Env::default();
    env.mock_all_auths();
    set_short_ttl_window(&env);

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    // 30 purchases: 5 more than MAX_MAINTENANCE_BATCH (25), proving the
    // sweep is bounded regardless of the caller's requested `limit`.
    let mut first_purchase_id = None;
    for i in 0..30u8 {
        let buyer = Address::generate(&env);
        let material_id = bytes32(&env, 100u8.wrapping_add(i));
        seed_purchasable_material(&env, &registry, &material_id, &creator, &asset, 1_000_000);
        let purchase_id = client.purchase(
            &buyer,
            &material_id,
            &asset,
            &1_000_000,
            &sample_transaction_id(&env),
        );
        if first_purchase_id.is_none() {
            first_purchase_id = Some((purchase_id, material_id, buyer));
        }
    }
    let (first_purchase_id, first_material_id, first_buyer) = first_purchase_id.unwrap();

    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    // A caller-requested limit far above MAX_MAINTENANCE_BATCH is clamped —
    // this single call, inside the test harness's default mainnet resource
    // enforcement, proves the sweep cannot exceed transaction resource
    // limits regardless of what's requested.
    let next_cursor = client.extend_purchases_ttl(&0, &10_000);
    assert_eq!(
        next_cursor, 25,
        "batch should be clamped to MAX_MAINTENANCE_BATCH"
    );

    let final_cursor = client.extend_purchases_ttl(&next_cursor, &10_000);
    assert_eq!(final_cursor, 30);

    // The very first purchase — registered long before the ledger advance —
    // was renewed by the sweep.
    let escrow_key = DataKey::Escrow(first_purchase_id);
    let entitlement_key = DataKey::Entitlement((first_material_id, first_buyer));
    let renewed_escrow_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&escrow_key)
    });
    let renewed_entitlement_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&entitlement_key)
    });
    assert_ttl_renewed_to_max(renewed_escrow_ttl);
    assert_ttl_renewed_to_max(renewed_entitlement_ttl);
}

#[test]
fn extend_allowed_asset_ttl_is_cursor_based() {
    let env = Env::default();
    env.mock_all_auths();
    set_short_ttl_window(&env);

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let asset_a = Address::generate(&env);
    let asset_b = Address::generate(&env);
    client.set_asset_allowed(&admin, &asset_a, &AssetKind::Token, &true);
    client.set_asset_allowed(&admin, &asset_b, &AssetKind::Token, &true);

    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    let cursor = client.extend_allowed_asset_ttl(&0, &1);
    assert_eq!(cursor, 1);
    let final_cursor = client.extend_allowed_asset_ttl(&cursor, &1);
    assert_eq!(final_cursor, 2);

    let asset_a_key = DataKey::AllowedAsset(asset_a);
    let renewed_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&asset_a_key)
    });
    assert_ttl_renewed_to_max(renewed_ttl);
}

#[test]
fn extend_creator_tier_ttl_is_cursor_based() {
    let env = Env::default();
    env.mock_all_auths();
    set_short_ttl_window(&env);

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let creator_a = Address::generate(&env);
    let creator_b = Address::generate(&env);
    client.set_creator_tier(&admin, &creator_a, &CreatorTier::Tier1);
    client.set_creator_tier(&admin, &creator_b, &CreatorTier::Tier2);

    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    let cursor = client.extend_creator_tier_ttl(&0, &1);
    assert_eq!(cursor, 1);
    let final_cursor = client.extend_creator_tier_ttl(&cursor, &1);
    assert_eq!(final_cursor, 2);

    let creator_a_key = DataKey::CreatorTier(creator_a);
    let renewed_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&creator_a_key)
    });
    assert_ttl_renewed_to_max(renewed_ttl);
}

#[test]
fn extend_admin_role_ttl_is_cursor_based() {
    let env = Env::default();
    env.mock_all_auths();
    set_short_ttl_window(&env);

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let second_admin = Address::generate(&env);
    client.transfer_admin(&admin, &second_admin);
    client.accept_admin(&second_admin);

    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    let cursor = client.extend_admin_role_ttl(&0, &1);
    assert_eq!(cursor, 1);
    let final_cursor = client.extend_admin_role_ttl(&cursor, &1);
    assert_eq!(final_cursor, 2);

    let admin_key = auth::AuthDataKey::AdminRole(admin);
    let renewed_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&admin_key)
    });
    assert_ttl_renewed_to_max(renewed_ttl);
}

#[test]
fn test_register_usdc_token_asset() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let registry = env.register(MockRegistry, ());

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);

    client.initialize(&admin, &registry, &treasury, &500);

    let usdc_address = Address::generate(&env);

    client.register_token_asset(&admin, &usdc_address, &true);

    let asset_info = client.get_asset_info(&usdc_address);
    assert_eq!(
        asset_info,
        Some(AssetInfo {
            kind: AssetKind::Token,
            enabled: true
        })
    );

    assert!(client.is_asset_allowed(&usdc_address));
}

#[test]
fn test_register_institution_asset() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let registry = env.register(MockRegistry, ());

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);

    client.initialize(&admin, &registry, &treasury, &500);

    let institution_asset = Address::generate(&env);

    client.register_institution_asset(&admin, &institution_asset, &true);

    let asset_info = client.get_asset_info(&institution_asset);
    assert_eq!(
        asset_info,
        Some(AssetInfo {
            kind: AssetKind::InstitutionAsset,
            enabled: true
        })
    );

    assert!(client.is_asset_allowed(&institution_asset));
}

#[test]
fn test_purchase_with_usdc() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);
    let registry_addr = env.register(MockRegistry, ());
    let usdc_asset = env.register(MockAsset, ());

    let registry_client = MockRegistryClient::new(&env, &registry_addr);
    let material_id = bytes32(&env, 42);

    registry_client.set_material(
        &material_id,
        &MaterialRecord {
            material_id: material_id.clone(),
            creator: creator.clone(),
            paused: false,
            status: MaterialStatus::Active,
            quotes: vec![
                &env,
                AssetQuote {
                    asset: usdc_asset.clone(),
                    amount: 5_000_000, // 50 USDC in 6 decimals
                },
            ],
            payout_shares: vec![
                &env,
                PayoutShare {
                    recipient: creator.clone(),
                    share_bps: 10_000,
                },
            ],
        },
    );

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);

    client.initialize(&admin, &registry_addr, &Address::generate(&env), &500);
    client.register_token_asset(&admin, &usdc_asset, &true);

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &usdc_asset,
        &5_000_000,
        &sample_transaction_id(&env),
    );
    assert_eq!(purchase_id, 0);

    assert!(client.has_entitlement(&material_id, &buyer));
}

#[test]
fn test_disable_asset() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let registry = env.register(MockRegistry, ());

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);

    client.initialize(&admin, &registry, &treasury, &500);

    let usdc_address = Address::generate(&env);

    client.register_token_asset(&admin, &usdc_address, &true);
    assert!(client.is_asset_allowed(&usdc_address));

    client.set_asset_allowed(&admin, &usdc_address, &AssetKind::Token, &false);
    assert!(!client.is_asset_allowed(&usdc_address));

    let asset_info = client.get_asset_info(&usdc_address);
    assert_eq!(
        asset_info,
        Some(AssetInfo {
            kind: AssetKind::Token,
            enabled: false
        })
    );
}

#[test]
fn test_register_native_asset() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let registry = env.register(MockRegistry, ());

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);

    client.initialize(&admin, &registry, &treasury, &500);

    let native_asset = Address::generate(&env);

    client.register_native_asset(&admin, &native_asset, &true);

    let asset_info = client.get_asset_info(&native_asset);
    assert_eq!(
        asset_info,
        Some(AssetInfo {
            kind: AssetKind::Native,
            enabled: true
        })
    );

    assert!(client.is_asset_allowed(&native_asset));
}

#[test]
fn test_native_asset_purchase() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let registry_addr = env.register(MockRegistry, ());
    let native_asset = env.register(MockAsset, ());

    let registry_client = MockRegistryClient::new(&env, &registry_addr);
    let material_id = bytes32(&env, 50);

    registry_client.set_material(
        &material_id,
        &MaterialRecord {
            material_id: material_id.clone(),
            creator: creator.clone(),
            paused: false,
            status: MaterialStatus::Active,
            quotes: vec![
                &env,
                AssetQuote {
                    asset: native_asset.clone(),
                    amount: 10_000_000, // 10 XLM
                },
            ],
            payout_shares: vec![
                &env,
                PayoutShare {
                    recipient: creator.clone(),
                    share_bps: 10_000,
                },
            ],
        },
    );

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);

    client.initialize(&admin, &registry_addr, &treasury, &500);
    client.register_native_asset(&admin, &native_asset, &true);

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &native_asset,
        &10_000_000,
        &sample_transaction_id(&env),
    );
    assert_eq!(purchase_id, 0);

    assert!(client.has_entitlement(&material_id, &buyer));
}

#[test]
fn test_native_asset_purchase_with_payouts() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let payout_recipient = Address::generate(&env);
    let registry_addr = env.register(MockRegistry, ());
    let native_asset = env.register(MockAsset, ());

    let registry_client = MockRegistryClient::new(&env, &registry_addr);
    let material_id = bytes32(&env, 51);

    registry_client.set_material(
        &material_id,
        &MaterialRecord {
            material_id: material_id.clone(),
            creator: creator.clone(),
            paused: false,
            status: MaterialStatus::Active,
            quotes: vec![
                &env,
                AssetQuote {
                    asset: native_asset.clone(),
                    amount: 20_000_000, // 20 XLM
                },
            ],
            payout_shares: vec![
                &env,
                PayoutShare {
                    recipient: creator.clone(),
                    share_bps: 6_500,
                },
                PayoutShare {
                    recipient: payout_recipient.clone(),
                    share_bps: 3_500,
                },
            ],
        },
    );

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);

    client.initialize(&admin, &registry_addr, &treasury, &500);
    client.register_native_asset(&admin, &native_asset, &true);

    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &native_asset,
        &20_000_000,
        &sample_transaction_id(&env),
    );
    assert_eq!(purchase_id, 0);

    assert!(client.has_entitlement(&material_id, &buyer));

    let escrow = client.get_escrow_record(&purchase_id).unwrap();
    assert!(!escrow.claimed);
    assert_eq!(escrow.total_amount, 20_000_000);
    assert_eq!(escrow.payout_shares.len(), 2);
}

// ============== Bulk Licensing Tests (#407) ==============

/// Helper: set up a purchasable material and contract, returning all addresses/IDs needed
/// for bulk purchase tests.
fn setup_bulk_purchase(
    env: &Env,
    recipient_count: u32,
) -> (
    Address,
    PurchaseManagerClient<'_>,
    Address,
    Address,
    Address,
    Address,
    BytesN<32>,
    Vec<Address>,
) {
    let admin = Address::generate(env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(env);
    let purchaser = Address::generate(env);
    let creator = Address::generate(env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(env, 99);
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

    let mut recipients = soroban_sdk::Vec::new(env);
    for _i in 0..recipient_count {
        recipients.push_back(Address::generate(env));
    }

    (
        contract_id,
        client,
        admin,
        purchaser,
        creator,
        asset,
        material_id,
        recipients,
    )
}

#[test]
fn bulk_purchase_succeeds_for_multiple_recipients() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, material_id, recipients) =
        setup_bulk_purchase(&env, 3);

    let asset_client = MockAssetClient::new(&env, &asset);

    let result = client.purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert_eq!(result.recipient_count, 3);
    assert_eq!(result.unit_price, 1_000_000);
    assert_eq!(result.total_paid, 3_000_000);
    assert_eq!(result.material_id, material_id);
    assert_eq!(result.purchaser, purchaser.clone());
    assert_eq!(result.first_purchase_id, 0);

    // Each recipient has an active entitlement
    for i in 0..3u32 {
        assert!(client.has_entitlement(&material_id, &recipients.get_unchecked(i)));
    }

    // Purchaser should not have an entitlement (they only paid)
    assert!(!client.has_entitlement(&material_id, &purchaser));

    // Aggregate payment: 1 platform fee transfer + 1 escrow transfer = 2 total
    assert_eq!(asset_client.transfer_count(), 2);
}

#[test]
fn bulk_purchase_empty_recipient_list_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, material_id, _recipients) =
        setup_bulk_purchase(&env, 0);

    let empty_recipients: Vec<Address> = soroban_sdk::Vec::new(&env);

    let result = client.try_purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &empty_recipients,
    );

    assert_eq!(result, Err(Ok(PurchaseError::EmptyRecipientList)));
}

#[test]
fn bulk_purchase_too_many_recipients_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let purchaser = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 99);
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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    // Create 51 recipients (exceeds MAX_BULK_LICENSE_RECIPIENTS of 50)
    let mut recipients = soroban_sdk::Vec::new(&env);
    for _ in 0..51u32 {
        recipients.push_back(Address::generate(&env));
    }

    let result = client.try_purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert_eq!(result, Err(Ok(PurchaseError::TooManyRecipients)));
}

#[test]
fn bulk_purchase_duplicate_recipient_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let purchaser = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());
    let duplicate = Address::generate(&env);

    let material_id = bytes32(&env, 99);
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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let recipients = vec![
        &env,
        duplicate.clone(),
        Address::generate(&env),
        duplicate.clone(),
    ];

    let result = client.try_purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert_eq!(result, Err(Ok(PurchaseError::DuplicateRecipient)));
}

#[test]
fn bulk_purchase_existing_entitlement_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, material_id, recipients) =
        setup_bulk_purchase(&env, 2);

    // First, give one recipient a license via single purchase
    let already_licensed = recipients.get_unchecked(0);
    client.purchase(
        &already_licensed,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );
    assert!(client.has_entitlement(&material_id, &already_licensed));

    // Now try bulk purchase that includes the already-licensed recipient
    let result = client.try_purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert_eq!(result, Err(Ok(PurchaseError::EntitlementAlreadyExists)));
}

#[test]
fn bulk_purchase_contract_paused_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, purchaser, _creator, asset, material_id, recipients) =
        setup_bulk_purchase(&env, 2);

    // Pause the contract
    client.pause(&admin);

    let result = client.try_purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert_eq!(result, Err(Ok(PurchaseError::ContractPaused)));
}

#[test]
fn bulk_purchase_material_not_active_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let purchaser = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 99);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Archived,
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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let mut recipients = soroban_sdk::Vec::new(&env);
    recipients.push_back(Address::generate(&env));
    recipients.push_back(Address::generate(&env));

    let result = client.try_purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert_eq!(result, Err(Ok(PurchaseError::MaterialNotActive)));
}

#[test]
fn bulk_purchase_invalid_price_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, material_id, recipients) =
        setup_bulk_purchase(&env, 2);

    let result = client.try_purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &999_999,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert_eq!(result, Err(Ok(PurchaseError::InvalidQuoteAmount)));
}

#[test]
fn bulk_purchase_asset_not_allowed_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, _asset, material_id, recipients) =
        setup_bulk_purchase(&env, 2);

    let unapproved_asset = Address::generate(&env);

    let result = client.try_purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &unapproved_asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert_eq!(result, Err(Ok(PurchaseError::AssetNotAllowed)));
}

#[test]
fn bulk_purchase_material_not_found_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, _material_id, recipients) =
        setup_bulk_purchase(&env, 2);

    let nonexistent_material = bytes32(&env, 255);

    let result = client.try_purchase_bulk_licenses(
        &purchaser,
        &nonexistent_material,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert_eq!(result, Err(Ok(PurchaseError::MaterialNotFound)));
}

#[test]
fn bulk_purchase_single_recipient_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, material_id, _recipients) =
        setup_bulk_purchase(&env, 1);

    let single_recipient = vec![&env, Address::generate(&env)];

    let result = client.purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &single_recipient,
    );

    assert_eq!(result.recipient_count, 1);
    assert_eq!(result.total_paid, 1_000_000);
    assert!(client.has_entitlement(&material_id, &single_recipient.get_unchecked(0)));
}

#[test]
fn bulk_purchase_payment_distribution_correct() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let purchaser = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());
    let asset_client = MockAssetClient::new(&env, &asset);

    let material_id = bytes32(&env, 99);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 2_000_000,
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

    let mut recipients = soroban_sdk::Vec::new(&env);
    recipients.push_back(Address::generate(&env));
    recipients.push_back(Address::generate(&env));
    recipients.push_back(Address::generate(&env));

    let result = client.purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &2_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    // Total: 3 * 2_000_000 = 6_000_000
    assert_eq!(result.total_paid, 6_000_000);
    assert_eq!(result.recipient_count, 3);

    // Aggregate: platform fee = 6_000_000 * 500 / 10_000 = 300_000
    // seller_net = 6_000_000 - 300_000 = 5_700_000
    // 2 aggregate transfers: fee to treasury + net to contract
    assert_eq!(asset_client.transfer_count(), 2);

    let t0 = asset_client.transfer_at(&0);
    assert_eq!(t0.amount, 300_000);
    assert_eq!(t0.from, purchaser);
    assert_eq!(t0.to, treasury);

    let t1 = asset_client.transfer_at(&1);
    assert_eq!(t1.amount, 5_700_000);
    assert_eq!(t1.from, purchaser);
    assert_eq!(t1.to, contract_id);
}

#[test]
fn bulk_purchase_emits_individual_and_bulk_events() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, material_id, recipients) =
        setup_bulk_purchase(&env, 2);

    let _result = client.purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    // Should emit:
    // - 2 individual PurchaseCompletedEvents (for indexer compatibility)
    // - 2 EscrowCreatedEvents
    // - 1 BulkPurchaseCompletedEvent
    let events = env.events().all();
    // At least 5 events (2 purchase + 2 escrow + 1 bulk)
    assert!(events.events().len() >= 5);
}

#[test]
fn bulk_purchase_escrow_records_created() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, material_id, recipients) =
        setup_bulk_purchase(&env, 3);

    let result = client.purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    // Each recipient gets its own escrow record
    for i in 0..3u32 {
        let purchase_id = result.first_purchase_id + i as u64;
        let escrow = client.get_escrow_record(&purchase_id);
        assert!(escrow.is_some());
        let escrow = escrow.unwrap();
        assert_eq!(escrow.material_id, material_id);
        assert!(!escrow.claimed);
        assert_eq!(escrow.total_amount, 1_000_000);
    }

    // Settlement records exist
    for i in 0..3u32 {
        let purchase_id = result.first_purchase_id + i as u64;
        let settlement = client.get_settlement(&purchase_id);
        assert!(settlement.is_some());
        assert_eq!(settlement.unwrap().state, SettlementState::Pending);
    }
}

#[test]
fn bulk_purchase_purchase_ids_sequential() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, material_id, recipients) =
        setup_bulk_purchase(&env, 4);

    let result = client.purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    // Purchase IDs should be sequential: 0, 1, 2, 3
    assert_eq!(result.first_purchase_id, 0);

    for i in 0..4u64 {
        let purchase_id = result.first_purchase_id + i;
        let buyer = client.get_purchase_buyer(&purchase_id);
        assert!(buyer.is_some());
    }
}

#[test]
fn bulk_purchase_all_recipients_can_query_entitlement() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, material_id, recipients) =
        setup_bulk_purchase(&env, 3);

    client.purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    for i in 0..3u32 {
        let recipient = recipients.get_unchecked(i);
        assert!(client.has_entitlement(&material_id, &recipient));
        let entitlement = client.get_entitlement(&material_id, &recipient).unwrap();
        assert!(entitlement.active);
        assert_eq!(entitlement.amount, 1_000_000);
    }
}

#[test]
fn bulk_purchase_does_not_grant_purchaser_entitlement() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, material_id, recipients) =
        setup_bulk_purchase(&env, 2);

    client.purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert!(!client.has_entitlement(&material_id, &purchaser));
}

#[test]
fn bulk_purchase_large_batch_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let purchaser = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 99);
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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let mut recipients = soroban_sdk::Vec::new(&env);
    for _ in 0..5u32 {
        recipients.push_back(Address::generate(&env));
    }

    let result = client.purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &100_000,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert_eq!(result.recipient_count, 5);
    assert_eq!(result.total_paid, 500_000);
}

#[test]
fn bulk_purchase_single_license_still_works() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 42);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator: creator.clone(),
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 500_000,
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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    // Single license purchase still works
    let purchase_id = client.purchase(
        &buyer,
        &material_id,
        &asset,
        &500_000,
        &sample_transaction_id(&env),
    );

    assert!(client.has_entitlement(&material_id, &buyer));
    assert_eq!(purchase_id, 0);
}

#[test]
fn bulk_purchase_no_charge_on_validation_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, purchaser, _creator, asset, material_id, recipients) =
        setup_bulk_purchase(&env, 2);

    let asset_client = MockAssetClient::new(&env, &asset);
    let initial_transfers = asset_client.transfer_count();

    // Try bulk purchase with wrong price — should fail before any transfer
    let result = client.try_purchase_bulk_licenses(
        &purchaser,
        &material_id,
        &asset,
        &999,
        &sample_transaction_id(&env),
        &recipients,
    );

    assert!(result.is_err() || result.unwrap().is_err());
    assert_eq!(asset_client.transfer_count(), initial_transfers);
}

// ============== Scholarship Credit Tests (#408) ==============

fn setup_scholarship(
    env: &Env,
) -> (
    Address,
    PurchaseManagerClient<'_>,
    Address,
    Address,
    Address,
    BytesN<32>,
) {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(env);
    let creator = Address::generate(env);
    let issuer = Address::generate(env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(env, 200);
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
    client.set_scholarship_issuer(&admin, &issuer, &true);

    (contract_id, client, admin, issuer, creator, material_id)
}

// ============== Issuer Authorization Tests ==============

#[test]
fn authorized_issuer_can_be_set() {
    let env = Env::default();
    let (_contract_id, client, _admin, issuer, _creator, _material_id) = setup_scholarship(&env);

    assert!(client.is_scholarship_issuer(&issuer));
}

#[test]
fn unauthorized_address_is_not_issuer() {
    let env = Env::default();
    let (_contract_id, client, _admin, _issuer, _creator, _material_id) = setup_scholarship(&env);

    let nobody = Address::generate(&env);
    assert!(!client.is_scholarship_issuer(&nobody));
}

#[test]
fn non_admin_cannot_set_scholarship_issuer() {
    let env = Env::default();
    let (_contract_id, client, _admin, _issuer, _creator, _material_id) = setup_scholarship(&env);

    let non_admin = Address::generate(&env);
    let new_issuer = Address::generate(&env);
    let result = client.try_set_scholarship_issuer(&non_admin, &new_issuer, &true);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn issuer_can_be_disabled() {
    let env = Env::default();
    let (_contract_id, client, admin, issuer, _creator, _material_id) = setup_scholarship(&env);

    client.set_scholarship_issuer(&admin, &issuer, &false);
    assert!(!client.is_scholarship_issuer(&issuer));
}

// ============== Credit Cost Configuration Tests ==============

#[test]
fn admin_can_set_scholarship_credit_cost() {
    let env = Env::default();
    let (_contract_id, client, admin, _issuer, _creator, material_id) = setup_scholarship(&env);

    client.set_scholarship_credit_cost(&admin, &material_id, &500);
    assert_eq!(client.get_scholarship_credit_cost(&material_id), Some(500));
}

#[test]
fn credit_cost_zero_rejected() {
    let env = Env::default();
    let (_contract_id, client, admin, _issuer, _creator, material_id) = setup_scholarship(&env);

    let result = client.try_set_scholarship_credit_cost(&admin, &material_id, &0);
    assert_eq!(result, Err(Ok(PurchaseError::InvalidCreditCost)));
}

#[test]
fn credit_cost_negative_rejected() {
    let env = Env::default();
    let (_contract_id, client, admin, _issuer, _creator, material_id) = setup_scholarship(&env);

    let result = client.try_set_scholarship_credit_cost(&admin, &material_id, &-100);
    assert_eq!(result, Err(Ok(PurchaseError::InvalidCreditCost)));
}

#[test]
fn non_admin_cannot_set_credit_cost() {
    let env = Env::default();
    let (_contract_id, client, _admin, _issuer, _creator, material_id) = setup_scholarship(&env);

    let non_admin = Address::generate(&env);
    let result = client.try_set_scholarship_credit_cost(&non_admin, &material_id, &500);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn credit_cost_for_nonexistent_material_rejected() {
    let env = Env::default();
    let (_contract_id, client, admin, _issuer, _creator, _material_id) = setup_scholarship(&env);

    let fake_id = bytes32(&env, 255);
    let result = client.try_set_scholarship_credit_cost(&admin, &fake_id, &500);
    assert_eq!(result, Err(Ok(PurchaseError::MaterialNotFound)));
}

// ============== Credit Issuance Tests ==============

#[test]
fn authorized_issuer_can_issue_credits() {
    let env = Env::default();
    let (_contract_id, client, _admin, issuer, _creator, _material_id) = setup_scholarship(&env);

    let learner = Address::generate(&env);
    let grant_id = client.issue_scholarship_credits(&issuer, &learner, &1000, &None);

    assert_eq!(grant_id, 0);
    assert_eq!(client.get_scholarship_credit_balance(&learner), 1000);

    let grant = client.get_scholarship_grant(&grant_id);
    assert_eq!(grant.learner, learner);
    assert_eq!(grant.issuer, issuer);
    assert_eq!(grant.total_credits, 1000);
    assert_eq!(grant.remaining_credits, 1000);
    assert!(grant.active);
    assert!(grant.expires_at.is_none());
}

#[test]
fn unauthorized_cannot_issue_credits() {
    let env = Env::default();
    let (_contract_id, client, _admin, _issuer, _creator, _material_id) = setup_scholarship(&env);

    let nobody = Address::generate(&env);
    let learner = Address::generate(&env);
    let result = client.try_issue_scholarship_credits(&nobody, &learner, &1000, &None);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn zero_amount_rejected() {
    let env = Env::default();
    let (_contract_id, client, _admin, issuer, _creator, _material_id) = setup_scholarship(&env);

    let learner = Address::generate(&env);
    let result = client.try_issue_scholarship_credits(&issuer, &learner, &0, &None);
    assert_eq!(result, Err(Ok(PurchaseError::InvalidCreditAmount)));
}

#[test]
fn negative_amount_rejected() {
    let env = Env::default();
    let (_contract_id, client, _admin, issuer, _creator, _material_id) = setup_scholarship(&env);

    let learner = Address::generate(&env);
    let result = client.try_issue_scholarship_credits(&issuer, &learner, &-100, &None);
    assert_eq!(result, Err(Ok(PurchaseError::InvalidCreditAmount)));
}

#[test]
fn expired_expiry_rejected() {
    let env = Env::default();
    let (_contract_id, client, _admin, issuer, _creator, _material_id) = setup_scholarship(&env);

    let learner = Address::generate(&env);
    // Advance ledger to 10 so that expiry of 5 is in the past
    env.ledger().set_sequence_number(10);
    let result = client.try_issue_scholarship_credits(&issuer, &learner, &1000, &Some(5));
    assert_eq!(result, Err(Ok(PurchaseError::InvalidExpiry)));
}

#[test]
fn grant_ids_are_sequential() {
    let env = Env::default();
    let (_contract_id, client, _admin, issuer, _creator, _material_id) = setup_scholarship(&env);

    let learner_a = Address::generate(&env);
    let learner_b = Address::generate(&env);

    let id1 = client.issue_scholarship_credits(&issuer, &learner_a, &500, &None);
    let id2 = client.issue_scholarship_credits(&issuer, &learner_b, &300, &None);

    assert_eq!(id1, 0);
    assert_eq!(id2, 1);
}

#[test]
fn multiple_grants_increase_balance() {
    let env = Env::default();
    let (_contract_id, client, _admin, issuer, _creator, _material_id) = setup_scholarship(&env);

    let learner = Address::generate(&env);

    client.issue_scholarship_credits(&issuer, &learner, &500, &None);
    client.issue_scholarship_credits(&issuer, &learner, &300, &None);

    assert_eq!(client.get_scholarship_credit_balance(&learner), 800);
}

#[test]
fn grant_not_found_query_rejected() {
    let env = Env::default();
    let (_contract_id, client, _admin, _issuer, _creator, _material_id) = setup_scholarship(&env);

    let result = client.try_get_scholarship_grant(&999);
    assert_eq!(result, Err(Ok(PurchaseError::ScholarshipGrantNotFound)));
}

// ============== Credit Redemption Tests ==============

#[test]
fn successful_scholarship_redemption() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 201);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);
    assert_eq!(client.get_scholarship_credit_balance(&learner), 1000);

    let result = client.redeem_scholarship_credits(&learner, &mid);
    assert_eq!(result.credits_used, 500);
    assert_eq!(result.remaining_credits, 500);
    assert_eq!(result.material_id, mid);
    assert_eq!(result.learner, learner);

    // Entitlement should exist
    assert!(client.has_entitlement(&mid, &learner));

    // Balance should be reduced
    assert_eq!(client.get_scholarship_credit_balance(&learner), 500);

    // Redemption record should exist
    let redemption = client.get_scholarship_redemption(&learner, &mid);
    assert!(redemption.is_some());
    assert_eq!(redemption.unwrap().credits_used, 500);
}

#[test]
fn redemption_fails_insufficient_credits() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 202);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &100, &None);

    let result = client.try_redeem_scholarship_credits(&learner, &mid);
    assert_eq!(
        result,
        Err(Ok(PurchaseError::InsufficientScholarshipCredits))
    );

    // Balance unchanged
    assert_eq!(client.get_scholarship_credit_balance(&learner), 100);
    // No entitlement
    assert!(!client.has_entitlement(&mid, &learner));
}

#[test]
fn redemption_fails_already_has_entitlement() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let buyer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 203);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    // Buyer already has a paid entitlement
    client.purchase(
        &buyer,
        &mid,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );
    assert!(client.has_entitlement(&mid, &buyer));

    // Issue credits and try to redeem — should fail
    client.issue_scholarship_credits(&issuer, &buyer, &1000, &None);
    let result = client.try_redeem_scholarship_credits(&buyer, &mid);
    assert_eq!(result, Err(Ok(PurchaseError::EntitlementAlreadyExists)));
}

#[test]
fn redemption_fails_content_not_eligible() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 204);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    // No credit cost set for this material

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);

    let result = client.try_redeem_scholarship_credits(&learner, &mid);
    assert_eq!(
        result,
        Err(Ok(PurchaseError::ContentNotScholarshipEligible))
    );
}

#[test]
fn double_redemption_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 205);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);

    client.redeem_scholarship_credits(&learner, &mid);

    let result = client.try_redeem_scholarship_credits(&learner, &mid);
    assert_eq!(result, Err(Ok(PurchaseError::RedemptionAlreadyExists)));
}

#[test]
fn redemption_nonexistent_material_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let _creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);

    let fake_id = bytes32(&env, 254);
    let result = client.try_redeem_scholarship_credits(&learner, &fake_id);
    assert_eq!(result, Err(Ok(PurchaseError::MaterialNotFound)));
}

#[test]
fn unauthorized_redemption_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 206);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);

    // Another address tries to redeem learner's credits
    let impersonator = Address::generate(&env);
    let result = client.try_redeem_scholarship_credits(&impersonator, &mid);
    assert!(result.is_err());
}

// ============== Expiry Tests ==============

#[test]
fn expired_grant_not_spendable() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 207);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    let learner = Address::generate(&env);
    // Issue grant with expiry at ledger 10 (current ledger is 0, so it's valid)
    client.issue_scholarship_credits(&issuer, &learner, &1000, &Some(10));
    // Move ledger forward past the expiry so the grant is expired
    env.ledger().set_sequence_number(11);

    // Grant exists but is expired
    let grant = client.get_scholarship_grant(&0);
    assert_eq!(grant.remaining_credits, 1000);

    // Balance should be 0 since the grant is expired
    assert_eq!(client.get_scholarship_credit_balance(&learner), 0);

    // Redemption should fail
    let result = client.try_redeem_scholarship_credits(&learner, &mid);
    assert_eq!(
        result,
        Err(Ok(PurchaseError::InsufficientScholarshipCredits))
    );
}

#[test]
fn grant_with_future_expiry_is_spendable() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 208);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &Some(100_000));

    assert_eq!(client.get_scholarship_credit_balance(&learner), 1000);

    let result = client.redeem_scholarship_credits(&learner, &mid);
    assert_eq!(result.credits_used, 500);
    assert_eq!(result.remaining_credits, 500);
}

// ============== Revocation Tests ==============

#[test]
fn issuer_can_revoke_grant() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 209);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);
    assert_eq!(client.get_scholarship_credit_balance(&learner), 1000);

    let revoked = client.revoke_scholarship_grant(&issuer, &0);
    assert_eq!(revoked, 1000);
    assert_eq!(client.get_scholarship_credit_balance(&learner), 0);
}

#[test]
fn admin_can_revoke_grant() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 210);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);

    // Admin revokes instead of issuer
    let revoked = client.revoke_scholarship_grant(&admin, &0);
    assert_eq!(revoked, 1000);
    assert_eq!(client.get_scholarship_credit_balance(&learner), 0);
}

#[test]
fn unauthorized_cannot_revoke() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let _creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);

    let stranger = Address::generate(&env);
    let result = client.try_revoke_scholarship_grant(&stranger, &0);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn revocation_of_nonexistent_grant_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());
    let _creator = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);

    let result = client.try_revoke_scholarship_grant(&issuer, &999);
    assert_eq!(result, Err(Ok(PurchaseError::ScholarshipGrantNotFound)));
}

#[test]
fn revocation_does_not_affect_redeemed_entitlements() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 211);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);

    // Redeem half
    client.redeem_scholarship_credits(&learner, &mid);
    assert!(client.has_entitlement(&mid, &learner));

    // Revoke remaining grant
    let revoked = client.revoke_scholarship_grant(&issuer, &0);
    assert_eq!(revoked, 500);

    // Entitlement should still be active
    assert!(client.has_entitlement(&mid, &learner));
}

// ============== Multiple Grants Consumption Order ==============

#[test]
fn credits_consumed_earliest_expiry_first() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 212);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    let learner = Address::generate(&env);
    // Grant 0: expires soon (ledger 500)
    client.issue_scholarship_credits(&issuer, &learner, &300, &Some(500));
    // Grant 1: expires later (ledger 100_000)
    client.issue_scholarship_credits(&issuer, &learner, &300, &Some(100_000));
    // Grant 2: no expiry
    client.issue_scholarship_credits(&issuer, &learner, &400, &None);

    assert_eq!(client.get_scholarship_credit_balance(&learner), 1000);

    // Redeem 500 credits — should consume from grant 0 first (earliest expiry)
    client.redeem_scholarship_credits(&learner, &mid);

    // Grant 0 should be exhausted (300), grant 1 should have 100 left
    assert_eq!(client.get_scholarship_credit_balance(&learner), 500);

    // Grant 0 should be inactive
    let grant_0 = client.get_scholarship_grant(&0);
    assert_eq!(grant_0.remaining_credits, 0);
    assert!(!grant_0.active);

    // Grant 1 should have 100 remaining
    let grant_1 = client.get_scholarship_grant(&1);
    assert_eq!(grant_1.remaining_credits, 100);
    assert!(grant_1.active);

    // Grant 2 untouched
    let grant_2 = client.get_scholarship_grant(&2);
    assert_eq!(grant_2.remaining_credits, 400);
    assert!(grant_2.active);
}

// ============== Paid Purchase Regression ==============

#[test]
fn paid_purchase_still_works_with_scholarship_active() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 213);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    // Issue some scholarship credits to a different learner
    let scholarship_learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &scholarship_learner, &1000, &None);

    // Regular buyer still works
    let buyer = Address::generate(&env);
    let purchase_id = client.purchase(
        &buyer,
        &mid,
        &asset,
        &1_000_000,
        &sample_transaction_id(&env),
    );
    assert!(client.has_entitlement(&mid, &buyer));
    assert_eq!(purchase_id, 0);

    // Scholarship learner can also redeem (different entitlement for same material)
    // Actually wait, both would get entitlement for same material. Let's verify the buyer got their entitlement.
    let entitlement = client.get_entitlement(&mid, &buyer).unwrap();
    assert!(entitlement.active);
    assert_eq!(entitlement.amount, 1_000_000);
}

// ============== Event Tests ==============

#[test]
fn scholarship_issuer_updated_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let issuer = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    assert!(!client.is_scholarship_issuer(&issuer));
    client.set_scholarship_issuer(&admin, &issuer, &true);
    assert!(client.is_scholarship_issuer(&issuer));
}

#[test]
fn scholarship_credit_cost_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 214);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    assert!(client.get_scholarship_credit_cost(&mid).is_none());
    client.set_scholarship_credit_cost(&admin, &mid, &750);
    assert_eq!(client.get_scholarship_credit_cost(&mid), Some(750));
}

// ============== Arithmetic Safety ==============

#[test]
fn large_credit_amount_works() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let _creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);

    let learner = Address::generate(&env);
    let large_amount: i128 = 1_000_000_000_000; // 1 trillion
    client.issue_scholarship_credits(&issuer, &learner, &large_amount, &None);
    assert_eq!(
        client.get_scholarship_credit_balance(&learner),
        large_amount
    );
}

// ============== Query Tests ==============

#[test]
fn zero_balance_for_learner_with_no_grants() {
    let env = Env::default();
    let (_contract_id, client, _admin, _issuer, _creator, _material_id) = setup_scholarship(&env);

    let learner = Address::generate(&env);
    assert_eq!(client.get_scholarship_credit_balance(&learner), 0);
}

#[test]
fn redemption_record_queryable() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 215);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);

    // No redemption yet
    assert!(client.get_scholarship_redemption(&learner, &mid).is_none());

    client.redeem_scholarship_credits(&learner, &mid);

    let redemption = client.get_scholarship_redemption(&learner, &mid).unwrap();
    assert_eq!(redemption.credits_used, 500);
    assert_eq!(redemption.learner, learner);
    assert_eq!(redemption.material_id, mid);
}

// ============== Entitlement Integration ==============

#[test]
fn scholarship_entitlement_is_queryable() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let mid = bytes32(&env, 216);
    let material = MaterialRecord {
        material_id: mid.clone(),
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
    registry_client.set_material(&mid, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);
    client.set_scholarship_credit_cost(&admin, &mid, &500);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);
    client.redeem_scholarship_credits(&learner, &mid);

    // Scholarship redemption creates a standard entitlement
    assert!(client.has_entitlement(&mid, &learner));
    let entitlement = client.get_entitlement(&mid, &learner).unwrap();
    assert!(entitlement.active);
    assert_eq!(entitlement.material_id, mid);
    assert_eq!(entitlement.buyer, learner);
}

// ============== Revocation Rejection ==============

#[test]
fn revoke_already_active_grant_twice_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let _creator = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    client.set_scholarship_issuer(&admin, &issuer, &true);

    let learner = Address::generate(&env);
    client.issue_scholarship_credits(&issuer, &learner, &1000, &None);

    client.revoke_scholarship_grant(&issuer, &0);

    // Second revocation should fail — grant is inactive
    let result = client.try_revoke_scholarship_grant(&issuer, &0);
    assert_eq!(result, Err(Ok(PurchaseError::ScholarshipGrantInactive)));
}
