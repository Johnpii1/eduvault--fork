export const dynamic = "force-dynamic";

import { NextResponse } from "next/server";
import { getDb } from "@/lib/mongodb";
import { verifyDashboardToken } from "@/lib/auth/session";
import {
  buildPurchaseHistoryRecords,
  fetchHorizonTransactions,
} from "@/lib/transactions/historyFeed";

async function getUserFromCookie(request) {
  const cookieHeader = request.headers.get("cookie") || "";
  const cookieMatch = cookieHeader.match(/auth_token=([^;]+)/);
  const token = cookieMatch ? decodeURIComponent(cookieMatch[1]) : null;
  if (!token) return null;
  const verification = await verifyDashboardToken(token, process.env.JWT_SECRET);
  if (!verification.valid) return null;
  return verification.payload;
}

function parsePage(value) {
  const page = Number(value);
  return Number.isFinite(page) && page > 0 ? Math.floor(page) : 1;
}

function parseLimit(value) {
  const limit = Number(value);
  if (!Number.isFinite(limit) || limit <= 0) return 20;
  return Math.min(Math.floor(limit), 100);
}

export async function GET(request) {
  try {
    const user = await getUserFromCookie(request);
    if (!user) {
      return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
    }

    const buyerAddress = user.walletAddress || user.address || user.id;
    if (!buyerAddress) {
      return NextResponse.json({ error: "No wallet address on account" }, { status: 400 });
    }

    const { searchParams } = new URL(request.url);
    const page = parsePage(searchParams.get("page"));
    const limit = parseLimit(searchParams.get("limit"));

    const db = await getDb();
    const skip = (page - 1) * limit;

    const [purchases, purchaseTotal, onchain] = await Promise.all([
      db
        .collection("purchases")
        .find({ buyerAddress: String(buyerAddress).toLowerCase() })
        .sort({ purchasedAt: -1, createdAt: -1, updatedAt: -1 })
        .skip(skip)
        .limit(limit)
        .toArray(),
      db.collection("purchases").countDocuments({ buyerAddress: String(buyerAddress).toLowerCase() }),
      fetchHorizonTransactions(String(buyerAddress), { page, limit }),
    ]);

    const purchaseRecords = buildPurchaseHistoryRecords(purchases);
    const combined = [...purchaseRecords, ...onchain.records].sort((a, b) => {
      const left = new Date(a.createdAt || 0).getTime();
      const right = new Date(b.createdAt || 0).getTime();
      return right - left;
    });

    return NextResponse.json({
      page,
      limit,
      hasMore: purchaseTotal > page * limit || onchain.hasMore,
      totals: {
        purchases: purchaseTotal,
      },
      records: combined,
    });
  } catch (error) {
    console.error("Transaction history error:", error);
    return NextResponse.json({ error: "Internal Server Error" }, { status: 500 });
  }
}
