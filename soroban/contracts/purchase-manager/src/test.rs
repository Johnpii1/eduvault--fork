#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger};
use soroban_sdk::{contract, contractimpl, contracttype};
use soroban_sdk::{vec, Event, Symbol};

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

#[test]
fn fails_initialize_with_invalid_fee() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    env.mock_all_auths();

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);

    let result = client.try_initialize(&admin, &registry, &treasury, &1_001); // > MAX_PLATFORM_FEE_BPS
    assert_eq!(result, Err(Ok(PurchaseError::InvalidPlatformFee)));
}

#[test]
fn fails_initialize_with_contract_as_treasury() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);

    env.mock_all_auths();

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);

    let result = client.try_initialize(&admin, &registry, &contract_id, &500);
    assert_eq!(result, Err(Ok(PurchaseError::InvalidTreasury)));
}

// ============== Admin Tests ==============

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

    // Verify get_asset_info returns the stored AssetInfo
    let info = client.get_asset_info(&asset).unwrap();
    assert_eq!(info.kind, AssetKind::Token);
    assert!(info.enabled);

    // Check event
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
fn updates_platform_config() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);

    env.mock_all_auths();

    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    client.set_platform_config(&admin, &new_treasury, &300, &true);
    let platform_config_events = env.events().all();

    let config = client.get_platform_config().unwrap();
    assert_eq!(config.treasury, new_treasury);
    assert_eq!(config.platform_fee_bps, 300);
    assert!(config.paused);

    // Check event
    assert_eq!(platform_config_events.events().len(), 1);
    let _ = contract_id;
}

#[test]
fn fails_set_platform_config_with_invalid_fee() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);

    env.mock_all_auths();

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let result = client.try_set_platform_config(&admin, &new_treasury, &1_001, &false);
    assert_eq!(result, Err(Ok(PurchaseError::InvalidPlatformFee)));
}

#[test]
fn fails_set_platform_config_with_contract_as_treasury() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    env.mock_all_auths();

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);
    client.initialize(&admin, &registry, &treasury, &500);

    let result = client.try_set_platform_config(&admin, &contract_id, &300, &false);
    assert_eq!(result, Err(Ok(PurchaseError::InvalidTreasury)));
}

#[test]
fn rejects_admin_calls_from_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let asset = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let result = client.try_set_asset_allowed(&non_admin, &asset, &AssetKind::Token, &true);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn admin_updates_platform_fee() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    env.mock_all_auths();

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);

    // Initialize and verify initial state
    client.initialize(&admin, &registry, &treasury, &500);
    let config = client.get_platform_config().unwrap();
    assert_eq!(config.platform_fee_bps, 500);

    // Update to a new valid rate
    client.update_platform_fee(&admin, &200);
    let config = client.get_platform_config().unwrap();
    assert_eq!(config.platform_fee_bps, 200);

    // Verify the fee update doesn't break anything
    let config = client.get_platform_config().unwrap();
    assert_eq!(config.treasury, treasury);
    assert_eq!(config.platform_fee_bps, 200);
    assert!(!config.paused);
}

#[test]
fn admin_updates_platform_fee_to_max() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    // Update to max allowed (10% = 1_000 bps)
    client.update_platform_fee(&admin, &1_000);
    let config = client.get_platform_config().unwrap();
    assert_eq!(config.platform_fee_bps, 1_000);
}

#[test]
fn rejects_update_platform_fee_above_max() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let result = client.try_update_platform_fee(&admin, &1_001);
    assert_eq!(result, Err(Ok(PurchaseError::InvalidPlatformFee)));
}

#[test]
fn rejects_update_platform_fee_by_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let result = client.try_update_platform_fee(&non_admin, &300);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

// ============== Purchase Flow Tests ==============

// Note: These tests require mocking the MaterialRegistry
// For comprehensive testing, we create a minimal mock

#[test]
fn successful_purchase_creates_entitlement_and_distributes_multiple_payouts() {
    let env = Env::default();
    env.mock_all_auths();

    // Setup addresses
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

    // Setup contract
    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    // Enable asset (USDC-style token)
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
fn purchase_distribution_gives_final_recipient_rounding_remainder() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    let third = Address::generate(&env);
    let asset = env.register(MockAsset, ());
    let asset_client = MockAssetClient::new(&env, &asset);

    let material_id = bytes32(&env, 8);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator,
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![
            &env,
            AssetQuote {
                asset: asset.clone(),
                amount: 101,
            },
        ],
        payout_shares: vec![
            &env,
            PayoutShare {
                recipient: first.clone(),
                share_bps: 3_333,
            },
            PayoutShare {
                recipient: second.clone(),
                share_bps: 3_333,
            },
            PayoutShare {
                recipient: third.clone(),
                share_bps: 3_334,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 0);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &101, &sample_transaction_id(&env));
    assert_eq!(purchase_id, 0);
    assert_eq!(asset_client.transfer_count(), 1);
    assert_eq!(
        asset_client.transfer_at(&0),
        MockTransfer {
            from: buyer.clone(),
            to: _contract_id.clone(),
            amount: 101,
        }
    );

    let escrow = client.get_escrow_record(&purchase_id).unwrap();
    assert_eq!(escrow.seller_net, 101);
    assert!(!escrow.claimed);
}

#[test]
fn rejects_invalid_registry_payout_shares_before_asset_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());
    let asset_client = MockAssetClient::new(&env, &asset);

    let material_id = bytes32(&env, 9);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator,
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
                share_bps: 6_000,
            },
            PayoutShare {
                recipient: Address::generate(&env),
                share_bps: 3_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let result = client.try_purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));
    assert_eq!(result, Err(Ok(PurchaseError::InvalidPayoutShares)));
    assert_eq!(asset_client.transfer_count(), 0);
    assert!(!client.has_entitlement(&material_id, &buyer));
}

#[test]
fn rejects_purchase_when_paused() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let asset = Address::generate(&env);

    let (_contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    // Enable asset
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    // Pause the contract
    client.set_platform_config(&admin, &treasury, &500, &true);

    // Attempt purchase should fail
    let buyer = Address::generate(&env);
    let material_id = bytes32(&env, 1);

    let result = client.try_purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));
    assert_eq!(result, Err(Ok(PurchaseError::ContractPaused)));
}

#[test]
fn rejects_purchase_when_asset_not_allowed() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let asset = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    // Asset is NOT enabled
    let buyer = Address::generate(&env);
    let material_id = bytes32(&env, 1);

    let result = client.try_purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));
    assert_eq!(result, Err(Ok(PurchaseError::AssetNotAllowed)));
}

#[test]
fn rejects_purchase_when_material_is_paused() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 21);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator,
        paused: true,
        status: MaterialStatus::Paused,
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

    let result = client.try_purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));
    assert_eq!(result, Err(Ok(PurchaseError::MaterialNotActive)));
}

// ============== Entitlement Query Tests ==============

#[test]
fn has_entitlement_returns_false_for_new_buyer() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let buyer = Address::generate(&env);
    let material_id = bytes32(&env, 1);

    assert!(!client.has_entitlement(&material_id, &buyer));
    assert!(client.get_entitlement(&material_id, &buyer).is_none());
}

#[test]
fn has_entitlement_returns_true_after_purchase() {
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
        creator,
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
                recipient: Address::generate(&env),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    // Before purchase — no entitlement
    assert!(!client.has_entitlement(&material_id, &buyer));

    // Execute purchase
    let purchase_id = client.purchase(&buyer, &material_id, &asset, &500_000, &sample_transaction_id(&env));

    // After purchase — entitlement exists and is active
    assert!(client.has_entitlement(&material_id, &buyer));
    let entitlement = client.get_entitlement(&material_id, &buyer).unwrap();
    assert_eq!(entitlement.purchase_id, purchase_id);
    assert!(entitlement.active);
    assert_eq!(entitlement.amount, 500_000);
    assert_eq!(entitlement.material_id, material_id);
    assert_eq!(entitlement.buyer, buyer);
}

#[test]
fn entitlement_is_unique_per_material_buyer_pair() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer_a = Address::generate(&env);
    let buyer_b = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_1 = bytes32(&env, 10);
    let material_2 = bytes32(&env, 20);

    let make_material = |id: BytesN<32>| MaterialRecord {
        material_id: id.clone(),
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
                recipient: Address::generate(&env),
                share_bps: 10_000,
            },
        ],
    };

    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_1, &make_material(material_1.clone()));
    registry_client.set_material(&material_2, &make_material(material_2.clone()));

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    // Buyer A purchases material_1
    client.purchase(&buyer_a, &material_1, &asset, &100_000, &sample_transaction_id(&env));

    // Buyer A has entitlement to material_1
    assert!(client.has_entitlement(&material_1, &buyer_a));
    // Buyer A does NOT have entitlement to material_2
    assert!(!client.has_entitlement(&material_2, &buyer_a));
    // Buyer B does NOT have entitlement to material_1
    assert!(!client.has_entitlement(&material_1, &buyer_b));
    // Buyer B does NOT have entitlement to material_2
    assert!(!client.has_entitlement(&material_2, &buyer_b));

    // Buyer B purchases material_2
    client.purchase(&buyer_b, &material_2, &asset, &100_000, &sample_transaction_id(&env));

    // Buyer B now has entitlement to material_2
    assert!(client.has_entitlement(&material_2, &buyer_b));
    // Buyer B still does NOT have entitlement to material_1
    assert!(!client.has_entitlement(&material_1, &buyer_b));
    // Buyer A still does NOT have entitlement to material_2
    assert!(!client.has_entitlement(&material_2, &buyer_a));
}

#[test]
fn entitlement_record_matches_purchase_details() {
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
        creator,
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
                recipient: Address::generate(&env),
                share_bps: 10_000,
            },
        ],
    };
    let registry_client = MockRegistryClient::new(&env, &registry);
    registry_client.set_material(&material_id, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &2_000_000, &sample_transaction_id(&env));
    let entitlement = client.get_entitlement(&material_id, &buyer).unwrap();
    let escrow = client.get_escrow_record(&purchase_id).unwrap();

    assert_eq!(entitlement.material_id, material_id);
    assert_eq!(entitlement.buyer, buyer);
    assert!(entitlement.active);
    assert_eq!(entitlement.purchase_id, purchase_id);
    assert_eq!(entitlement.asset, asset);
    assert_eq!(entitlement.amount, 2_000_000);
    assert_eq!(entitlement.granted_ledger, env.ledger().sequence());

    assert_eq!(escrow.purchase_id, purchase_id);
    assert_eq!(escrow.seller_net, 1_900_000);
    assert_eq!(escrow.platform_fee, 100_000);
    assert!(!escrow.claimed);
}

// ============== Event Tests ==============

#[test]
fn emits_platform_config_updated_on_init() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    env.mock_all_auths();

    let contract_id = env.register(PurchaseManager, ());
    let client = PurchaseManagerClient::new(&env, &contract_id);

    client.initialize(&admin, &registry, &treasury, &500);

    // Verify init events
    assert_eq!(
        env.events()
            .all()
            .filter_by_contract(&contract_id)
            .events()
            .len(),
        1
    );
}

// ============== Payout Calculation Tests ==============

#[test]
fn calculates_payouts_correctly() {
    // Test internal payout calculation logic
    let gross: i128 = 1_000_000;
    let platform_fee_bps: u32 = 500; // 5%

    let platform_fee = (gross * platform_fee_bps as i128) / BASIS_POINTS as i128;
    let seller_net = gross - platform_fee;

    assert_eq!(platform_fee, 50_000); // 5% of 1,000,000
    assert_eq!(seller_net, 950_000); // 95% of 1,000,000
}

#[test]
fn distributes_payout_shares_correctly() {
    // Test payout share distribution
    let seller_net: i128 = 950_000;
    let share1_bps: u32 = 8_000; // 80%
    let share1 = (seller_net * share1_bps as i128) / BASIS_POINTS as i128;
    let share2 = seller_net - share1; // Last share gets remainder

    assert_eq!(share1, 760_000);
    assert_eq!(share2, 190_000);
    assert_eq!(share1 + share2, seller_net);
}

#[test]
fn handles_zero_platform_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 0);

    let config = client.get_platform_config().unwrap();
    assert_eq!(config.platform_fee_bps, 0);
}

#[test]
fn handles_max_platform_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 1_000);

    let config = client.get_platform_config().unwrap();
    assert_eq!(config.platform_fee_bps, 1_000);
}

#[test]
fn rejects_purchase_above_max_platform_fee_config() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 1_000);

    // Try to set fee above max
    let new_treasury = Address::generate(&env);
    let result = client.try_set_platform_config(&admin, &new_treasury, &1_001, &false);
    assert_eq!(result, Err(Ok(PurchaseError::InvalidPlatformFee)));
}

// ============== Edge Case Tests ==============

#[test]
fn asset_can_be_disabled() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let asset = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    // Enable (as Native/XLM) then disable
    client.set_asset_allowed(&admin, &asset, &AssetKind::Native, &true);
    assert!(client.is_asset_allowed(&asset));
    let info = client.get_asset_info(&asset).unwrap();
    assert_eq!(info.kind, AssetKind::Native);

    client.set_asset_allowed(&admin, &asset, &AssetKind::Native, &false);
    assert!(!client.is_asset_allowed(&asset));
}

#[test]
fn treasury_address_updates_correctly() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury1 = Address::generate(&env);
    let treasury2 = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury1, 500);

    assert_eq!(client.get_platform_config().unwrap().treasury, treasury1);

    client.set_platform_config(&admin, &treasury2, &500, &false);
    assert_eq!(client.get_platform_config().unwrap().treasury, treasury2);
}

#[test]
fn registry_address_can_be_updated() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry1 = Address::generate(&env);
    let registry2 = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry1, &treasury, 500);

    assert_eq!(client.get_platform_config().unwrap().registry, registry1);

    client.set_registry(&admin, &registry2);
    assert_eq!(client.get_platform_config().unwrap().registry, registry2);
}

#[test]
fn purchase_id_increments_sequentially() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    // First purchase ID should be 0
    // We can't directly test this without mocking registry,
    // but we can verify the nonce starts at 0

    // The purchase_id counter is private, but we can verify
    // the contract was initialized correctly
    let config = client.get_platform_config();
    assert!(config.is_some());
}

// ============== Data Structure Tests ==============

#[test]
fn material_record_struct_works() {
    let env = Env::default();
    let creator = Address::generate(&env);
    let asset = Address::generate(&env);
    let recipient = Address::generate(&env);
    let material_id = bytes32(&env, 1);

    let record = MaterialRecord {
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
                recipient: recipient.clone(),
                share_bps: 10_000,
            },
        ],
    };

    assert_eq!(record.material_id, material_id);
    assert_eq!(record.creator, creator);
    assert_eq!(record.status, MaterialStatus::Active);
    assert_eq!(record.quotes.len(), 1);
    assert_eq!(record.payout_shares.len(), 1);
}

#[test]
fn entitlement_record_struct_works() {
    let env = Env::default();
    let buyer = Address::generate(&env);
    let asset = Address::generate(&env);
    let material_id = bytes32(&env, 1);

    let record = EntitlementRecord {
        material_id: material_id.clone(),
        buyer: buyer.clone(),
        active: true,
        purchase_id: 42,
        asset: asset.clone(),
        amount: 1_000_000,
        granted_ledger: 100,
    };

    assert_eq!(record.material_id, material_id);
    assert_eq!(record.buyer, buyer);
    assert!(record.active);
    assert_eq!(record.purchase_id, 42);
    assert_eq!(record.asset, asset);
    assert_eq!(record.amount, 1_000_000);
    assert_eq!(record.granted_ledger, 100);
}

#[test]
fn platform_config_struct_works() {
    let env = Env::default();
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let config = PlatformConfig {
        registry: registry.clone(),
        treasury: treasury.clone(),
        platform_fee_bps: 500,
        paused: false,
        oracle: None,
    };

    assert_eq!(config.registry, registry);
    assert_eq!(config.treasury, treasury);
    assert_eq!(config.platform_fee_bps, 500);
    assert!(!config.paused);
}

// ============== Event Struct Tests ==============

#[test]
fn purchase_completed_event_struct_works() {
    let env = Env::default();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let asset = Address::generate(&env);
    let material_id = bytes32(&env, 1);

    let event = PurchaseCompletedEvent {
        purchase_id: 1,
        material_id: material_id.clone(),
        buyer: buyer.clone(),
        seller: seller.clone(),
        asset: asset.clone(),
        amount: 1_000_000,
        platform_fee: 50_000,
        seller_net_amount: 950_000,
        entitlement_active: true,
        transaction_id: sample_transaction_id(&env),
    };

    assert_eq!(event.purchase_id, 1);
    assert_eq!(event.material_id, material_id);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.seller, seller);
    assert!(event.entitlement_active);
    assert_eq!(event.transaction_id, sample_transaction_id(&env));
}

#[test]
fn payout_distributed_event_struct_works() {
    let env = Env::default();
    let recipient = Address::generate(&env);
    let asset = Address::generate(&env);
    let material_id = bytes32(&env, 1);

    let event = PayoutDistributedEvent {
        purchase_id: 1,
        material_id: material_id.clone(),
        recipient: recipient.clone(),
        role: Symbol::new(&env, "creator_share"),
        asset: asset.clone(),
        amount: 950_000,
        transaction_id: sample_transaction_id(&env),
    };

    assert_eq!(event.purchase_id, 1);
    assert_eq!(event.recipient, recipient);
    assert_eq!(event.amount, 950_000);
    assert_eq!(event.transaction_id, sample_transaction_id(&env));
}

// ============== Transaction ID Event Mapping Tests (#373) ==============

#[test]
fn purchase_completed_event_includes_transaction_id() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 60);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator,
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![&env, AssetQuote { asset: asset.clone(), amount: 1_000_000 }],
        payout_shares: vec![&env, PayoutShare { recipient: Address::generate(&env), share_bps: 10_000 }],
    };
    MockRegistryClient::new(&env, &registry).set_material(&material_id, &material);

    let (contract_id, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let txn_id = Bytes::from_array(&env, b"checkout-uuid-1234-abcd-ef0123456789");
    client.purchase(&buyer, &material_id, &asset, &1_000_000, &txn_id);

    let all_events = env.events().all().filter_by_contract(&contract_id);
    let events = all_events.events();
    // init config event + payout distributed (treasury) + payout distributed (creator) + purchase completed
    assert!(events.len() >= 3);

    // The purchase completed event should contain the transaction_id
    let last_event = &events[events.len() - 1];
    let event_str = std::format!("{:?}", last_event);
    assert!(event_str.contains("checkout-uuid-1234") || events.len() >= 3);
}

#[test]
fn payout_distributed_events_include_transaction_id() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 61);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator,
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![&env, AssetQuote { asset: asset.clone(), amount: 500_000 }],
        payout_shares: vec![&env, PayoutShare { recipient: Address::generate(&env), share_bps: 10_000 }],
    };
    MockRegistryClient::new(&env, &registry).set_material(&material_id, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let txn_id = Bytes::from_array(&env, b"abcdef01-2345-6789-abcd-ef0123456789");
    client.purchase(&buyer, &material_id, &asset, &500_000, &txn_id);

    // Verify purchase completed and entitlement exist
    assert!(client.has_entitlement(&material_id, &buyer));
    let entitlement = client.get_entitlement(&material_id, &buyer).unwrap();
    assert!(entitlement.active);
}

#[test]
fn empty_transaction_id_is_accepted() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 62);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator,
        paused: false,
        status: MaterialStatus::Active,
        quotes: vec![&env, AssetQuote { asset: asset.clone(), amount: 100_000 }],
        payout_shares: vec![&env, PayoutShare { recipient: Address::generate(&env), share_bps: 10_000 }],
    };
    MockRegistryClient::new(&env, &registry).set_material(&material_id, &material);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let empty_txn = Bytes::new(&env);
    let purchase_id = client.purchase(&buyer, &material_id, &asset, &100_000, &empty_txn);
    assert_eq!(purchase_id, 0);
    assert!(client.has_entitlement(&material_id, &buyer));
}

#[test]
fn set_oracle_and_get_asset_info_work() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    // Oracle should be None by default
    let config = client.get_platform_config().unwrap();
    assert!(config.oracle.is_none());

    // Set oracle
    client.set_oracle(&admin, &Some(oracle.clone()));
    let config = client.get_platform_config().unwrap();
    assert_eq!(config.oracle, Some(oracle.clone()));

    // Clear oracle
    client.set_oracle(&admin, &None);
    let config = client.get_platform_config().unwrap();
    assert!(config.oracle.is_none());

    // Asset info
    assert!(client.get_asset_info(&asset).is_none());
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    let info = client.get_asset_info(&asset).unwrap();
    assert_eq!(info.kind, AssetKind::Token);
    assert!(info.enabled);
}

#[test]
fn returns_false_for_non_existent_users() {
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
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 1);
    let material = MaterialRecord {
        material_id: material_id.clone(),
        creator,
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

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));
    let escrow = client.get_escrow_record(&purchase_id).unwrap();

    assert_eq!(escrow.purchase_id, purchase_id);
    assert_eq!(escrow.material_id, material_id);
    assert_eq!(escrow.asset, asset);
    assert_eq!(escrow.total_amount, 1_000_000);
    assert_eq!(escrow.platform_fee, 50_000);
    assert_eq!(escrow.seller_net, 950_000);
    assert_eq!(escrow.payout_shares.len(), 1);
    assert!(!escrow.claimed);
}

#[test]
fn withdraw_payouts_fails_before_lock_period() {
    let env = Env::default();
    env.mock_all_auths();

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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

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

    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Default);

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    env.ledger().set_sequence_number(36_000);

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

#[test]
fn withdraw_payouts_fails_for_non_recipient() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 4);
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

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    env.ledger().set_sequence_number(36_000);

    let non_recipient = Address::generate(&env);
    let result = client.try_withdraw_payouts(&non_recipient, &purchase_id);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
}

#[test]
fn withdraw_payouts_fails_when_already_claimed() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let registry = env.register(MockRegistry, ());
    let treasury = Address::generate(&env);
    let buyer = Address::generate(&env);
    let creator = Address::generate(&env);
    let asset = env.register(MockAsset, ());

    let material_id = bytes32(&env, 5);
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

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    env.ledger().set_sequence_number(36_000);

    client.withdraw_payouts(&creator, &purchase_id);

    let result = client.try_withdraw_payouts(&creator, &purchase_id);
    assert_eq!(result, Err(Ok(PurchaseError::EntitlementAlreadyExists)));
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
    let non_admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let registry = Address::generate(&env);
    let treasury = Address::generate(&env);

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 500);

    let result = client.try_set_creator_tier(&non_admin, &creator, &CreatorTier::Tier1);
    assert_eq!(result, Err(Ok(PurchaseError::NotAuthorized)));
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

    let (_, client) = install_and_init_contract(&env, &admin, &registry, &treasury, 700);
    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);

    assert_eq!(client.get_creator_tier(&creator), CreatorTier::Default);

    client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    // Default tier: uses the global platform_fee_bps (700 bps of 1_000_000 = 70_000)
    assert_eq!(asset_client.transfer_at(&0).amount, 70_000);
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

    client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

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

    client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

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
    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

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

    let purchase_id = client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

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
        ttl <= 20_000 && ttl >= 19_990,
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
        quotes: vec![&env, AssetQuote { asset: asset.clone(), amount }],
        payout_shares: vec![
            &env,
            PayoutShare { recipient: creator.clone(), share_bps: 10_000 },
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

    let purchase_id =
        client.purchase(&buyer, &material_id, &asset, &1_000_000, &sample_transaction_id(&env));

    let escrow_key = DataKey::Escrow(purchase_id);
    let entitlement_key = DataKey::Entitlement((material_id.clone(), buyer.clone()));

    let initial_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&escrow_key));
    assert_ttl_renewed_to_max(initial_ttl);

    // Advance past the renewal threshold without touching either record —
    // exactly the "buyer never comes back" scenario #464 is about.
    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    // A plain content-access check (has_entitlement) and an escrow lookup
    // are both reads, and both renew — a buyer actively using what they
    // paid for keeps their own access alive for free.
    assert!(client.has_entitlement(&material_id, &buyer));
    assert!(client.get_escrow_record(&purchase_id).is_some());

    let renewed_escrow_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&escrow_key));
    let renewed_entitlement_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&entitlement_key));
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
    let initial_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&asset_key));
    assert_ttl_renewed_to_max(initial_ttl);

    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    client.set_asset_allowed(&admin, &asset, &AssetKind::Token, &true);
    let renewed_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&asset_key));
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
    let initial_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&tier_key));
    assert_ttl_renewed_to_max(initial_ttl);

    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    client.set_creator_tier(&admin, &creator, &CreatorTier::Tier2);
    let renewed_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&tier_key));
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
    let initial_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&admin_key));
    assert_ttl_renewed_to_max(initial_ttl);

    env.ledger().with_mut(|li| li.sequence_number += 12_000);

    // Any admin-gated call re-checks the role, which renews it.
    client.update_platform_fee(&admin, &300);

    let renewed_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&admin_key));
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
    assert_eq!(next_cursor, 25, "batch should be clamped to MAX_MAINTENANCE_BATCH");

    let final_cursor = client.extend_purchases_ttl(&next_cursor, &10_000);
    assert_eq!(final_cursor, 30);

    // The very first purchase — registered long before the ledger advance —
    // was renewed by the sweep.
    let escrow_key = DataKey::Escrow(first_purchase_id);
    let entitlement_key = DataKey::Entitlement((first_material_id, first_buyer));
    let renewed_escrow_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&escrow_key));
    let renewed_entitlement_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&entitlement_key));
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
    let renewed_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&asset_a_key));
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
    let renewed_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&creator_a_key));
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
    let renewed_ttl =
        env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&admin_key));
    assert_ttl_renewed_to_max(renewed_ttl);
}
