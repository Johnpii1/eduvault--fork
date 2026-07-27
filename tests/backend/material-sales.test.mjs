/**
 * Backend tests for the per-material sales aggregation used by
 * GET /api/materials (src/lib/backend/materialSales.js), which powers the
 * "My Materials" creator dashboard's sales/revenue columns.
 *
 * Mirrors the aggregation logic using an in-memory collection double, the
 * same pattern used by tests/backend/analytics.test.mjs and
 * tests/backend/creator-payouts.test.mjs, so the tests stay fast and
 * deterministic without a real MongoDB.
 */

import assert from "node:assert/strict";
import { describe, test } from "node:test";

const COMPLETED_PURCHASE_STATUSES = ["confirmed", "settled", "completed"];

function matchesQuery(doc, query = {}) {
  return Object.entries(query).every(([key, value]) => {
    if (value && typeof value === "object" && "$in" in value) {
      return value.$in.includes(doc[key]);
    }
    return doc[key] === value;
  });
}

function makeCursor(results) {
  return { toArray: async () => results };
}

function makeCollection(docs = []) {
  const store = [...docs];
  return {
    async aggregate(pipeline) {
      const matchStage = pipeline.find((stage) => stage.$match)?.$match ?? {};
      const matched = store.filter((doc) => matchesQuery(doc, matchStage));

      const groupStage = pipeline.find((stage) => stage.$group)?.$group;
      if (!groupStage) return makeCursor(matched);

      const groups = new Map();
      for (const doc of matched) {
        const key = doc[String(groupStage._id).replace("$", "")];
        const existing = groups.get(key) ?? { _id: key };
        for (const [field, expression] of Object.entries(groupStage)) {
          if (field === "_id") continue;
          if (expression.$sum) {
            const value =
              typeof expression.$sum === "number"
                ? expression.$sum
                : Number(doc[String(expression.$sum.$toDouble ?? expression.$sum).replace("$", "")]) || 0;
            existing[field] = (existing[field] ?? 0) + value;
          }
        }
        groups.set(key, existing);
      }
      return makeCursor([...groups.values()]);
    },
  };
}

function makeDb(collections = {}) {
  return { collection: (name) => collections[name] ?? makeCollection() };
}

/** Mirrors getMaterialSalesTotals in src/lib/backend/materialSales.js */
async function getMaterialSalesTotals(db, materialIdStrings) {
  if (!materialIdStrings || materialIdStrings.length === 0) return new Map();

  const rows = await (
    await db.collection("purchases").aggregate([
      {
        $match: {
          materialId: { $in: materialIdStrings },
          status: { $in: COMPLETED_PURCHASE_STATUSES },
        },
      },
      {
        $group: {
          _id: "$materialId",
          sales: { $sum: 1 },
          revenue: { $sum: { $toDouble: "$amount" } },
        },
      },
    ])
  ).toArray();

  const totals = new Map();
  for (const row of rows) {
    totals.set(String(row._id), { sales: row.sales ?? 0, revenue: row.revenue ?? 0 });
  }
  return totals;
}

describe("getMaterialSalesTotals", () => {
  test("returns an empty map when the creator has no materials", async () => {
    const db = makeDb({ purchases: makeCollection([{ materialId: "mat-1", status: "confirmed", amount: "10" }]) });
    const totals = await getMaterialSalesTotals(db, []);
    assert.equal(totals.size, 0);
  });

  test("aggregates sales count and revenue per material for completed purchases only", async () => {
    const purchases = makeCollection([
      { materialId: "mat-1", status: "confirmed", amount: "10" },
      { materialId: "mat-1", status: "settled", amount: "5.5" },
      { materialId: "mat-1", status: "pending", amount: "999" },
      { materialId: "mat-2", status: "completed", amount: "20" },
      { materialId: "mat-2", status: "refunded", amount: "20" },
    ]);
    const db = makeDb({ purchases });

    const totals = await getMaterialSalesTotals(db, ["mat-1", "mat-2"]);

    assert.deepEqual(totals.get("mat-1"), { sales: 2, revenue: 15.5 });
    assert.deepEqual(totals.get("mat-2"), { sales: 1, revenue: 20 });
  });

  test("excludes purchases for materials outside the requested id set", async () => {
    const purchases = makeCollection([
      { materialId: "mat-1", status: "confirmed", amount: "10" },
      { materialId: "someone-elses-material", status: "confirmed", amount: "999" },
    ]);
    const db = makeDb({ purchases });

    const totals = await getMaterialSalesTotals(db, ["mat-1"]);

    assert.deepEqual(totals.get("mat-1"), { sales: 1, revenue: 10 });
    assert.equal(totals.has("someone-elses-material"), false);
  });

  test("returns no entry for a material with zero completed sales", async () => {
    const purchases = makeCollection([{ materialId: "mat-1", status: "pending", amount: "10" }]);
    const db = makeDb({ purchases });

    const totals = await getMaterialSalesTotals(db, ["mat-1"]);

    assert.equal(totals.has("mat-1"), false);
  });
});
