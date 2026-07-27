const COMPLETED_PURCHASE_STATUSES = ["confirmed", "settled", "completed"];

/**
 * Aggregates completed-sale counts and revenue per material from the
 * `purchases` collection. Mirrors the completed-purchase definition used by
 * GET /api/creator/analytics so "sales" means the same thing everywhere in
 * the creator dashboard.
 *
 * Returns a Map keyed by material id (string) -> { sales, revenue }.
 */
export async function getMaterialSalesTotals(db, materialIdStrings) {
  if (!materialIdStrings || materialIdStrings.length === 0) {
    return new Map();
  }

  const rows = await db
    .collection("purchases")
    .aggregate([
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
    .toArray();

  const totals = new Map();
  for (const row of rows) {
    totals.set(String(row._id), { sales: row.sales ?? 0, revenue: row.revenue ?? 0 });
  }
  return totals;
}
