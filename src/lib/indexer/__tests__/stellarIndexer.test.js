import { describe, it, expect, vi, beforeEach } from "vitest";
import { xdr, nativeToScVal, Address, Keypair } from "@stellar/stellar-sdk";

vi.mock("@/lib/email", () => ({ sendReceiptIfEligible: vi.fn().mockResolvedValue(undefined) }));

import { runIndexerBatch } from "../stellarIndexer.js";
import { COLLECTIONS } from "@/lib/backend/schemaContracts";

function toBase64(scVal) {
  return scVal.toXDR("base64");
}
function symbolTopic(name) {
  return toBase64(nativeToScVal(name, { type: "symbol" }));
}
function u64Topic(value) {
  return toBase64(nativeToScVal(BigInt(value), { type: "u64" }));
}
function bytesTopic(buffer) {
  return toBase64(nativeToScVal(buffer, { type: "bytes" }));
}
function addressTopic(strkey) {
  return toBase64(new Address(strkey).toScVal());
}
function vecValue(scVals) {
  return toBase64(xdr.ScVal.scvVec(scVals));
}

function purchaseCompletedRawEvent({ id, purchaseId, materialId, buyer, seller, asset, amount }) {
  return {
    id,
    ledger: 100,
    txHash: `tx-${id}`,
    ledgerClosedAt: "2026-07-25T00:00:00Z",
    topic: [
      symbolTopic("purchase"),
      symbolTopic("completed"),
      u64Topic(purchaseId),
      bytesTopic(materialId),
      addressTopic(buyer),
    ],
    value: vecValue([
      new Address(seller).toScVal(),
      new Address(asset).toScVal(),
      nativeToScVal(BigInt(amount), { type: "i128" }),
      nativeToScVal(0n, { type: "i128" }),
      nativeToScVal(BigInt(amount), { type: "i128" }),
      xdr.ScVal.scvBool(true),
      nativeToScVal(Buffer.alloc(16, 1), { type: "bytes" }),
    ]),
  };
}

// Minimal in-memory Mongo-like fake covering exactly the operations
// runIndexerBatch/applyIndexedEvent perform.
function createFakeDb() {
  const collections = new Map();
  let autoId = 0;

  function matches(doc, query) {
    return Object.entries(query).every(([k, v]) => doc?.[k] === v);
  }

  function applyUpdate(doc, update, isInsert) {
    if (update.$set) Object.assign(doc, update.$set);
    if (isInsert && update.$setOnInsert) {
      for (const [k, v] of Object.entries(update.$setOnInsert)) {
        if (!(k in doc)) doc[k] = v;
      }
    }
  }

  function collection(name) {
    if (!collections.has(name)) collections.set(name, new Map());
    const data = collections.get(name);

    return {
      async insertOne(doc) {
        if (data.has(doc._id)) {
          const err = new Error("E11000 duplicate key error");
          err.code = 11000;
          throw err;
        }
        data.set(doc._id, { ...doc });
        return { insertedId: doc._id };
      },
      async findOne(query = {}) {
        for (const doc of data.values()) {
          if (matches(doc, query)) return doc;
        }
        return null;
      },
      find(query = {}) {
        const results = Array.from(data.values()).filter((d) => matches(d, query));
        return { toArray: async () => results };
      },
      async updateOne(query, update, opts = {}) {
        for (const doc of data.values()) {
          if (matches(doc, query)) {
            applyUpdate(doc, update, false);
            return { matchedCount: 1, upsertedCount: 0 };
          }
        }
        if (opts.upsert) {
          const id = query._id ?? `auto-${++autoId}`;
          const doc = { ...query, _id: id };
          applyUpdate(doc, update, true);
          data.set(id, doc);
          return { matchedCount: 0, upsertedCount: 1 };
        }
        return { matchedCount: 0, upsertedCount: 0 };
      },
      async deleteOne(query) {
        for (const [key, doc] of data.entries()) {
          if (matches(doc, query)) {
            data.delete(key);
            return { deletedCount: 1 };
          }
        }
        return { deletedCount: 0 };
      },
      _all() {
        return Array.from(data.values());
      },
    };
  }

  return { collection };
}

beforeEach(() => vi.clearAllMocks());

describe("runIndexerBatch", () => {
  it("parses and applies a valid on-chain purchase.completed event", async () => {
    const db = createFakeDb();
    const buyer = Keypair.random().publicKey();
    const seller = Keypair.random().publicKey();
    const asset = Keypair.random().publicKey();
    const materialId = Buffer.alloc(32, 5);

    const rawEvent = purchaseCompletedRawEvent({
      id: "evt-1",
      purchaseId: 1,
      materialId,
      buyer,
      seller,
      asset,
      amount: 1_000_000,
    });

    const eventSource = {
      getEvents: vi.fn().mockResolvedValue({ events: [rawEvent], nextCursor: "cursor-1", lastLedger: 100 }),
    };

    const result = await runIndexerBatch({ db, eventSource });

    expect(result).toEqual({ applied: 1, skipped: 0, nextCursor: "cursor-1" });

    const purchases = db.collection(COLLECTIONS.purchases)._all();
    expect(purchases).toHaveLength(1);
    expect(purchases[0]).toMatchObject({
      materialId: materialId.toString("hex"),
      buyerAddress: buyer.toLowerCase(),
      sellerAddress: seller,
      amount: "1000000",
      status: "settled",
    });

    const entitlements = db.collection(COLLECTIONS.entitlementCache)._all();
    expect(entitlements).toHaveLength(1);
    expect(entitlements[0].active).toBe(true);
  });

  it("skips events with an unrecognized topic without touching the dead-letter collection", async () => {
    const db = createFakeDb();
    const unknownEvent = {
      id: "evt-unknown",
      ledger: 1,
      txHash: "tx-unknown",
      topic: [symbolTopic("some_other"), symbolTopic("thing")],
      value: vecValue([]),
    };

    const eventSource = {
      getEvents: vi.fn().mockResolvedValue({ events: [unknownEvent], nextCursor: null, lastLedger: 1 }),
    };

    const result = await runIndexerBatch({ db, eventSource });

    expect(result.applied).toBe(0);
    expect(result.skipped).toBe(1);
    expect(db.collection(COLLECTIONS.deadLetterEvents)._all()).toHaveLength(0);
    expect(db.collection(COLLECTIONS.syncEvents)._all()).toHaveLength(0);
  });

  it("prevents duplicate purchase rows across two batches for the same event id", async () => {
    const db = createFakeDb();
    const buyer = Keypair.random().publicKey();
    const seller = Keypair.random().publicKey();
    const asset = Keypair.random().publicKey();
    const materialId = Buffer.alloc(32, 9);

    const rawEvent = purchaseCompletedRawEvent({
      id: "evt-dup",
      purchaseId: 2,
      materialId,
      buyer,
      seller,
      asset,
      amount: 2_000_000,
    });

    const eventSource = {
      getEvents: vi.fn().mockResolvedValue({ events: [rawEvent], nextCursor: "c", lastLedger: 1 }),
    };

    const first = await runIndexerBatch({ db, eventSource });
    const second = await runIndexerBatch({ db, eventSource });

    expect(first).toEqual({ applied: 1, skipped: 0, nextCursor: "c" });
    expect(second).toEqual({ applied: 0, skipped: 1, nextCursor: "c" });
    expect(db.collection(COLLECTIONS.purchases)._all()).toHaveLength(1);
  });
});
