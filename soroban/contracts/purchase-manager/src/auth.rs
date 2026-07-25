use crate::{extend_instance_ttl, extend_persistent_ttl, PurchaseError};
use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
pub enum AuthDataKey {
    AdminRole(Address),
    /// TTL maintenance index (#464): sequential slot -> admin address,
    /// populated the first time an address is granted the admin role, so
    /// `extend_admin_role_ttl` can page through every admin without
    /// depending on any off-chain enumeration. The slot count lives in
    /// instance storage (see `crate::DataKey`) alongside the other
    /// maintenance counters, since it must never expire itself.
    AdminRoleIndex(u64),
}

pub fn has_admin_role(env: &Env, admin: &Address) -> bool {
    let key = AuthDataKey::AdminRole(admin.clone());
    let has = env.storage().persistent().has(&key);
    if has {
        extend_persistent_ttl(env, &key);
    }
    has
}

pub fn set_admin_role(env: &Env, admin: &Address) {
    let key = AuthDataKey::AdminRole(admin.clone());
    let is_new_admin = !env.storage().persistent().has(&key);
    env.storage().persistent().set(&key, &true);
    extend_persistent_ttl(env, &key);

    if is_new_admin {
        index_admin_role(env, admin);
    }
}

pub fn require_admin(env: &Env, caller: &Address) -> Result<(), PurchaseError> {
    caller.require_auth();
    if !has_admin_role(env, caller) {
        return Err(PurchaseError::NotAuthorized);
    }
    Ok(())
}

fn index_admin_role(env: &Env, admin: &Address) {
    let count: u64 = env
        .storage()
        .instance()
        .get(&crate::DataKey::AdminRoleIndexCount)
        .unwrap_or(0);
    let index_key = AuthDataKey::AdminRoleIndex(count);
    env.storage().persistent().set(&index_key, admin);
    extend_persistent_ttl(env, &index_key);
    env.storage()
        .instance()
        .set(&crate::DataKey::AdminRoleIndexCount, &(count + 1));
    extend_instance_ttl(env);
}

/// Bump the TTL of up to `limit` (capped at `MAX_MAINTENANCE_BATCH`) admin
/// role grants, starting at `cursor`. Permissionless, cursor-based — see
/// `crate::PurchaseManager::extend_admin_role_ttl` for the public entrypoint
/// this backs.
pub fn extend_admin_role_ttl(env: &Env, cursor: u64, limit: u32) -> u64 {
    let limit = (limit.min(crate::MAX_MAINTENANCE_BATCH)) as u64;
    let count: u64 = env
        .storage()
        .instance()
        .get(&crate::DataKey::AdminRoleIndexCount)
        .unwrap_or(0);
    extend_instance_ttl(env);

    let end = cursor.saturating_add(limit).min(count);
    let mut i = cursor;
    while i < end {
        let index_key = AuthDataKey::AdminRoleIndex(i);
        if let Some(admin) = env.storage().persistent().get::<_, Address>(&index_key) {
            extend_persistent_ttl(env, &index_key);
            let admin_key = AuthDataKey::AdminRole(admin);
            if env.storage().persistent().has(&admin_key) {
                extend_persistent_ttl(env, &admin_key);
            }
        }
        i += 1;
    }

    end
}
