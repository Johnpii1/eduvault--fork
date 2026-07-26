import { COLLECTIONS } from "../backend/schemaContracts.js";
import { sendReceiptIfEligible } from "../email.js";
import { parseContractEvent } from "./eventParser.js";
import { invalidateEntitlement } from "../entitlement.js";


function duplicateKey(error) {
  return error?.code === 11000;
}

export function eventId(event) {
  return (
    event.id ||
    event.eventId ||
    [event.ledger, event.transactionHash || event.txHash, event.topic, event.index]
      .filter(Boolean)
      .join(":")
  );
}

export async function applyIndexedEvent(db, event, { now = new Date() } = {}) {
  const id = eventId(event);
  if (!id) {
    throw new Error("Indexed event is missing a stable id");
  }

  try {
    await db.collection(COLLECTIONS.syncEvents).insertOne({
      _id: id,
      type: event.type,
      source: event.source || "stellar",
      raw: event,
      createdAt: now,
    });
  } catch (error) {
    if (duplicateKey(error)) {
      // event already recorded; mark and continue to ensure downstream
      // side-effects (purchases/entitlement/materials) are applied on reprocess.
      var alreadyIndexed = true;
    } else {
      throw error;
    }
  }

  if (event.type === "material.registered") {
    await db.collection(COLLECTIONS.materials).updateOne(
      { materialId: event.materialId },
      {
        $set: {
          materialId: event.materialId,
          chainContractId: event.contractId || null,
          chainLedger: event.ledger || null,
          chainTxHash: event.transactionHash || event.txHash || null,
          syncStatus: "indexed",
          updatedAt: now,
        },
        $setOnInsert: {
          createdAt: now,
          visibility: "public",
        },
      },
      { upsert: true }
    );
  }

  if (event.type === "purchase.completed") {
    const buyerAddress = String(event.buyerAddress || "").toLowerCase();
    await db.collection(COLLECTIONS.purchases).updateOne(
      { materialId: event.materialId, buyerAddress },
      {
        $set: {
          materialId: event.materialId,
          buyerAddress,
          purchaseId: event.purchaseId ?? null,
          sellerAddress: event.sellerAddress || null,
          chainTxHash: event.transactionHash || event.txHash || null,
          amount: event.amount || null,
          asset: event.asset || null,
          status: "settled",
          settlementState: "Pending",
          updatedAt: now,
        },
        $setOnInsert: { createdAt: now },
      },
      { upsert: true }
    );

    await db.collection(COLLECTIONS.entitlementCache).updateOne(
      { materialId: event.materialId, buyerAddress },
      {
        $set: {
          materialId: event.materialId,
          buyerAddress,
          state: "finalized",
          active: true,
          source: "stellar",
          purchaseId: event.purchaseId ?? null,
          settlementState: "Pending",
          chainTxHash: event.transactionHash || event.txHash || null,
          checkedAt: now,
          updatedAt: now,
        },
        $setOnInsert: { createdAt: now },
      },
      { upsert: true }
    );

    const purchase = await db.collection(COLLECTIONS.purchases).findOne({
      materialId: event.materialId,
      buyerAddress,
    });
    if (purchase) {
      const { enqueueSideEffect } = await import('../backend/outbox.js');
      enqueueSideEffect({
        sourceAggregate: 'purchase',
        sourceId: String(purchase._id),
        intent: {
          type: 'email',
          channel: 'purchase_receipt',
          payload: { source: 'indexer', purchaseId: purchase._id },
        },
      }).catch(err => console.error(err));
    }
  }

  if (event.type === "purchase.refunded") {
    const buyerAddress = String(event.buyerAddress || "").toLowerCase();

    await db.collection(COLLECTIONS.purchases).updateOne(
      { materialId: event.materialId, buyerAddress },
      {
        $set: {
          settlementState: "Refunded",
          refundedAt: now,
          refundTransactionHash: event.transactionHash || event.txHash || null,
          updatedAt: now,
        },
      }
    );

    // Chain-confirmed, terminal revocation — invalidate immediately so no
    // in-flight cached "active" entitlement outlives the refund.
    await invalidateEntitlement(event.materialId, buyerAddress, "chain-refunded", {
      db,
      purchaseId: event.purchaseId,
      settlementState: "Refunded",
    });
  }

  if (event.type === "dispute.opened") {
    const purchase = await db.collection(COLLECTIONS.purchases).findOne({
      materialId: event.materialId,
      purchaseId: event.purchaseId,
    });
    const buyerAddress = purchase?.buyerAddress || null;

    await db.collection(COLLECTIONS.purchases).updateOne(
      { materialId: event.materialId, purchaseId: event.purchaseId },
      { $set: { settlementState: "Disputed", disputedAt: now, updatedAt: now } }
    );

    if (buyerAddress) {
      await invalidateEntitlement(event.materialId, buyerAddress, "chain-disputed", {
        db,
        purchaseId: event.purchaseId,
        settlementState: "Disputed",
      });
    }
  }

  return { eventId: id, skipped: !!alreadyIndexed };
}

export async function runIndexerBatch({ db, eventSource, source = "stellar", limit = 100 }) {
  const stateId = `${source}:events`;
  const state = await db.collection(COLLECTIONS.syncState).findOne({ _id: stateId });
  const batch = await eventSource.getEvents({ cursor: state?.cursor || null, limit });
  const events = batch.events || [];
  let applied = 0;
  let skipped = 0;

  const maxRetries = Number(process.env.INDEXER_MAX_RETRIES || 3);

  for (const event of events) {
    const parsedEvent = parseContractEvent(event);
    if (!parsedEvent) {
      // Unrecognized topic or undecodable payload (e.g. a future event type
      // this indexer doesn't know about yet) — skip rather than fail the batch.
      skipped += 1;
      continue;
    }

    try {
      const result = await applyIndexedEvent(db, { ...parsedEvent, source });
      if (result.skipped) {
        skipped += 1;
        // If a dead-letter exists for this event, increment its retry count
        try {
          const dlCol = db.collection(COLLECTIONS.deadLetterEvents);
          const existing = await dlCol.findOne({ _id: result.eventId });
          if (existing) {
            const retryCount = (existing.retryCount || 0) + 1;
            const status = retryCount > maxRetries ? 'failed' : 'retryable';
            await dlCol.updateOne(
              { _id: result.eventId },
              { $set: { retryCount, lastAttemptedAt: new Date(), status } },
              { upsert: true }
            );
          }
        } catch (e) {
          // ignore
        }
      } else applied += 1;

      // on success (not skipped), remove any dead-letter record
      try {
        if (!result.skipped && result.eventId) await db.collection(COLLECTIONS.deadLetterEvents).deleteOne({ _id: result.eventId });
      } catch (err) {
        // ignore cleanup errors
      }
    } catch (err) {
      const id = eventId(event) || `${source}:unknown:${Math.random().toString(36).slice(2, 8)}`;
      const dlCol = db.collection(COLLECTIONS.deadLetterEvents);
      const existing = await dlCol.findOne({ _id: id });
      const retryCount = (existing?.retryCount || 0) + 1;
      const status = retryCount > maxRetries ? 'failed' : 'retryable';
      await dlCol.updateOne(
        { _id: id },
        {
          $set: {
            raw: event,
            lastError: String(err?.message || err),
            retryCount,
            lastAttemptedAt: new Date(),
            status,
            source,
          },
          $setOnInsert: { createdAt: new Date() },
        },
        { upsert: true }
      );
    }
  }

  await db.collection(COLLECTIONS.syncState).updateOne(
    { _id: stateId },
    {
      $set: {
        _id: stateId,
        source,
        cursor: batch.nextCursor || state?.cursor || null,
        lastLedger: batch.lastLedger || state?.lastLedger || null,
        updatedAt: new Date(),
      },
      $setOnInsert: { createdAt: new Date() },
    },
    { upsert: true }
  );

  return { applied, skipped, nextCursor: batch.nextCursor || null };
}

export function createJsonRpcEventSource({ rpcUrl, contractId, fetchImpl = fetch }) {
  const contractIds = Array.isArray(contractId)
    ? contractId.filter(Boolean)
    : contractId
      ? [contractId]
      : [];

  return {
    async getEvents({ cursor, limit }) {
      const response = await fetchImpl(rpcUrl, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          method: "getEvents",
          params: {
            filters: contractIds.length > 0 ? [{ contractIds }] : [],
            pagination: { cursor, limit },
          },
        }),
      });
      const payload = await response.json();
      if (payload.error) {
        throw new Error(payload.error.message || "Stellar RPC getEvents failed");
      }

      return {
        events: payload.result?.events || [],
        nextCursor: payload.result?.cursor || null,
        lastLedger: payload.result?.latestLedger || null,
      };
    },
  };
}

export async function reprocessDeadLetters(db, { statuses = ['retryable', 'failed'], limit = 100 } = {}) {
  const dlCol = db.collection(COLLECTIONS.deadLetterEvents);
  const items = [];

  if (typeof dlCol.find === 'function') {
    const cursor = dlCol.find({ status: { $in: statuses } }).limit(limit);
    for await (const doc of cursor) items.push(doc);
  } else {
    const records = dlCol.records instanceof Map ? Array.from(dlCol.records.values()) : [];
    for (const r of records) if (statuses.includes(r.status)) items.push(r);
  }

  const reprocessed = [];
  for (const entry of items.slice(0, limit)) {
    try {
      await applyIndexedEvent(db, entry.raw);
      await dlCol.deleteOne({ _id: entry._id });
      reprocessed.push({ id: entry._id });
    } catch (err) {
      // attempt one more immediate retry (helps transient failures during reprocess)
      try {
        await applyIndexedEvent(db, entry.raw);
        await dlCol.deleteOne({ _id: entry._id });
        reprocessed.push({ id: entry._id });
        continue;
      } catch (err2) {
        await dlCol.updateOne({ _id: entry._id }, { $set: { lastError: String(err2?.message || err2), lastAttemptedAt: new Date() } }, { upsert: true });
      }
    }
  }

  return { reprocessed };
}
