import { getDb } from '@/lib/mongodb';
import { PURCHASE_MANAGER_CONTRACT_ID, STELLAR_RPC_URL } from '@/lib/config/chain';
import { isCompletedPurchaseStatus, normalizeBuyerAddress } from '@/lib/purchases/access';

async function getCachedEntitlement(db, materialId, buyerAddress) {
  return db.collection('entitlement_cache').findOne({
    materialId,
    buyerAddress: buyerAddress.toLowerCase(),
  });
}

async function setCachedEntitlement(db, materialId, buyerAddress, active, source = 'chain') {
  await db.collection('entitlement_cache').updateOne(
    { materialId, buyerAddress: buyerAddress.toLowerCase() },
    {
      $set: {
        materialId,
        buyerAddress: buyerAddress.toLowerCase(),
        active,
        source: active ? source : `${source}-miss`,
        updatedAt: new Date(),
      },
      $setOnInsert: { createdAt: new Date() },
    },
    { upsert: true }
  );
}

async function checkChainEntitlement(materialId, buyerAddress) {
  if (!PURCHASE_MANAGER_CONTRACT_ID || !STELLAR_RPC_URL) return null;

  try {
    const body = {
      jsonrpc: '2.0',
      id: 1,
      method: 'simulateTransaction',
      params: {
        transaction: buildHasEntitlementXdr(materialId, buyerAddress),
      },
    };

    const res = await fetch(STELLAR_RPC_URL, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(8_000),
    });

    const payload = await res.json();
    if (payload.error) return null;

    const retval = payload.result?.results?.[0]?.xdr;
    if (!retval) return null;

    return decodeBoolean(retval);
  } catch {
    return null;
  }
}

/**
 * Check the on-chain settlement state for a purchase.
 * Returns the settlement state string or null if unavailable.
 *
 * Settlement states: Pending, Released, Disputed, Refunded, Expired
 */
async function checkChainSettlementState(purchaseId) {
  if (!PURCHASE_MANAGER_CONTRACT_ID || !STELLAR_RPC_URL) return null;

  try {
    const body = {
      jsonrpc: '2.0',
      id: 1,
      method: 'simulateTransaction',
      params: {
        transaction: buildSettlementStateXdr(purchaseId),
      },
    };

    const res = await fetch(STELLAR_RPC_URL, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(8_000),
    });

    const payload = await res.json();
    if (payload.error) return null;

    const retval = payload.result?.results?.[0]?.xdr;
    if (!retval) return null;

    return decodeSettlementState(retval);
  } catch {
    return null;
  }
}

function buildHasEntitlementXdr(_materialId, _buyerAddress) {
  return '';
}

function buildSettlementStateXdr(_purchaseId) {
  return '';
}

function decodeBoolean(xdrBase64) {
  return xdrBase64.includes('AAAE') || xdrBase64.includes('true');
}

function decodeSettlementState(xdrBase64) {
  if (xdrBase64.includes('Pending')) return 'Pending';
  if (xdrBase64.includes('Released')) return 'Released';
  if (xdrBase64.includes('Disputed')) return 'Disputed';
  if (xdrBase64.includes('Refunded')) return 'Refunded';
  if (xdrBase64.includes('Expired')) return 'Expired';
  return null;
}

/**
 * Create an entitlement record for a buyer after a successful purchase.
 * Writes to the entitlement_cache collection for fast subsequent lookups.
 *
 * @param {object} db - MongoDB database instance (optional; will be fetched if omitted)
 * @param {string} materialId - The material identifier
 * @param {string} buyerAddress - The buyer's Stellar public key
 * @param {object} [purchaseData] - Optional purchase metadata to store
 * @param {string} [purchaseData.purchaseId] - The purchase record ID
 * @param {string} [purchaseData.transactionHash] - On-chain transaction hash
 * @param {string} [purchaseData.amount] - Purchase amount
 * @param {string} [purchaseData.asset] - Payment asset code
 * @returns {Promise<{success: boolean, source: string}>}
 */
export async function createEntitlement(materialId, buyerAddress, purchaseData = {}) {
  if (!materialId || !buyerAddress) {
    return { success: false, source: 'invalid-params' };
  }

  const db = await getDb();
  const normalised = buyerAddress.toLowerCase();

  const entry = {
    materialId,
    buyerAddress: normalised,
    active: true,
    source: 'purchase-api',
    purchaseId: purchaseData.purchaseId || null,
    transactionHash: purchaseData.transactionHash || null,
    amount: purchaseData.amount || null,
    asset: purchaseData.asset || null,
    settlementState: 'Pending',
    updatedAt: new Date(),
    createdAt: new Date(),
  };

  await db.collection('entitlement_cache').updateOne(
    { materialId, buyerAddress: normalised },
    { $set: entry },
    { upsert: true }
  );

  return { success: true, source: 'purchase-api' };
}

/**
 * Revoke (deactivate) an entitlement.
 *
 * @param {string} materialId - The material identifier
 * @param {string} buyerAddress - The buyer's wallet address
 * @returns {Promise<{success: boolean}>}
 */
export async function revokeEntitlement(materialId, buyerAddress) {
  if (!materialId || !buyerAddress) {
    return { success: false };
  }

  const db = await getDb();
  const normalised = buyerAddress.toLowerCase();

  await db.collection('entitlement_cache').updateOne(
    { materialId, buyerAddress: normalised },
    {
      $set: {
        active: false,
        source: 'revoked',
        settlementState: 'Refunded',
        updatedAt: new Date(),
      },
      $setOnInsert: {
        materialId,
        buyerAddress: normalised,
        createdAt: new Date(),
      },
    },
    { upsert: true }
  );

  return { success: true };
}

export async function verifyEntitlement(materialId, buyerAddress) {
  if (!materialId || !buyerAddress) {
    return { hasAccess: false, source: 'invalid-params' };
  }

  const db = await getDb();
  const normalised = normalizeBuyerAddress(buyerAddress);

  // Check cache first
  const cached = await getCachedEntitlement(db, materialId, normalised);
  if (cached) {
    // If cache says active, verify it hasn't been refunded on-chain
    if (cached.active) {
      // If we have a purchaseId, check settlement state on-chain
      if (cached.purchaseId) {
        const settlementState = await checkChainSettlementState(cached.purchaseId);
        if (settlementState === 'Refunded') {
          // Cache is stale — entitlement was revoked on-chain
          await setCachedEntitlement(db, materialId, normalised, false, 'chain-refunded');
          return { hasAccess: false, source: 'chain-refunded' };
        }
        // Update cached settlement state
        if (settlementState && settlementState !== cached.settlementState) {
          await db.collection('entitlement_cache').updateOne(
            { materialId, buyerAddress: normalised },
            { $set: { settlementState, updatedAt: new Date() } }
          );
        }
      }
      return { hasAccess: true, source: 'cache' };
    }
    // Cache says inactive — check if on-chain says otherwise
    const onChain = await checkChainEntitlement(materialId, buyerAddress);
    if (onChain === true) {
      await setCachedEntitlement(db, materialId, normalised, true, 'chain');
      return { hasAccess: true, source: 'chain' };
    }
    return { hasAccess: false, source: 'cache-inactive' };
  }

  // No cache — check purchases collection
  const purchase = await db.collection('purchases').findOne({
    materialId,
    buyerAddress: normalised,
  });

  if (purchase && isCompletedPurchaseStatus(purchase.status)) {
    // Check if purchase has been refunded
    if (purchase.settlementState === 'Refunded') {
      await setCachedEntitlement(db, materialId, normalised, false, 'purchases-db-refunded');
      return { hasAccess: false, source: 'purchases-db-refunded' };
    }

    // Check on-chain settlement state if we have a purchaseId
    if (purchase.purchaseId) {
      const settlementState = await checkChainSettlementState(purchase.purchaseId);
      if (settlementState === 'Refunded') {
        await setCachedEntitlement(db, materialId, normalised, false, 'chain-refunded');
        return { hasAccess: false, source: 'chain-refunded' };
      }
    }

    await setCachedEntitlement(db, materialId, normalised, true, 'purchases-db');
    return { hasAccess: true, source: 'purchases-db' };
  }

  // Fall back to on-chain check
  const onChain = await checkChainEntitlement(materialId, buyerAddress);
  if (onChain === true) {
    await setCachedEntitlement(db, materialId, normalised, true, 'chain');
    return { hasAccess: true, source: 'chain' };
  }

  return { hasAccess: false, source: 'not-found' };
}

export function requireEntitlement(handler, getMaterialId) {
  return async function protectedHandler(request, context) {
    const { searchParams } = new URL(request.url);
    const buyerAddress = searchParams.get('buyerAddress') ?? '';
    const materialId =
      typeof getMaterialId === 'function'
        ? getMaterialId(request, context)
        : searchParams.get('materialId') ?? '';

    if (!buyerAddress || !materialId) {
      const { NextResponse } = await import('next/server');
      return NextResponse.json(
        { error: 'Missing buyerAddress or materialId' },
        { status: 400 }
      );
    }

    const { hasAccess, source } = await verifyEntitlement(
      materialId,
      buyerAddress
    );

    if (!hasAccess) {
      const { NextResponse } = await import('next/server');
      return NextResponse.json(
        {
          error: 'Unlicensed Access',
          detail:
            'You do not hold an active entitlement for this material. Please purchase it first.',
        },
        { status: 403 }
      );
    }

    return handler(request, context, { materialId, buyerAddress, source });
  };
}