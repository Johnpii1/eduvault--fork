export const dynamic = "force-dynamic";

import { NextResponse } from "next/server";
import { ObjectId } from "mongodb";
import { auditLog } from "@/lib/api/audit";
import { getUserFromCookie } from "@/lib/api/auth";
import { withApiHardening } from "@/lib/api/hardening";
import { getDb } from "@/lib/mongodb";

export const runtime = "nodejs";

const VALID_REASONS = [
  "Copyright violation",
  "Broken link",
  "Incorrect metadata",
  "Inappropriate content",
  "Low quality / Unreadable",
  "Spam / Advertising",
  "Other",
];

/**
 * POST /api/reports
 *
 * Body:
 *   materialId  {string}  Required. The ID of the material being reported.
 *   reason      {string}  Required. Must be one of VALID_REASONS.
 *   description {string}  Optional. Additional context from the reporter.
 */
export async function POST(request) {
  return withApiHardening(
    request,
    { route: "reports", rateLimit: { limit: 10, windowMs: 60_000 } },
    async () => {
      try {
        const user = await getUserFromCookie(request);
        if (!user) {
          auditLog({ event: "auth_failed", route: "reports", method: "POST", status: 401 });
          return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
        }

        const body = await request.json();
        const { materialId, reason, description = "" } = body ?? {};

        // Validate materialId
        if (!materialId || !ObjectId.isValid(materialId)) {
          return NextResponse.json({ error: "Invalid or missing materialId" }, { status: 400 });
        }

        // Validate reason
        if (!reason || !VALID_REASONS.includes(reason)) {
          return NextResponse.json(
            { error: `Reason is required and must be one of: ${VALID_REASONS.join(", ")}` },
            { status: 400 }
          );
        }

        const db = await getDb();

        // Verify material exists
        const material = await db
          .collection("materials")
          .findOne({ _id: new ObjectId(materialId) });

        if (!material) {
          return NextResponse.json({ error: "Material not found" }, { status: 404 });
        }

        // Resolve reporter address
        let reporterAddress =
          user.walletAddress || user.address || user.walletAddressLower || user.id || "";
        if (!reporterAddress && user.sub && ObjectId.isValid(user.sub)) {
          const dbUser = await db
            .collection("users")
            .findOne({ _id: new ObjectId(user.sub) });
          reporterAddress =
            dbUser?.walletAddress || dbUser?.address || dbUser?.walletAddressLower || "";
        }

        const now = new Date();
        const reportDoc = {
          materialId,
          materialTitle: material.title || "",
          reason,
          description: String(description).slice(0, 2000),
          reporterAddress,
          reporterId: user.sub || user.id || null,
          reporterName: user.name || "Anonymous",
          status: "pending_review",
          moderationStatus: "pending_review",
          createdAt: now,
          updatedAt: now,
        };

        const result = await db.collection("reports").insertOne(reportDoc);

        auditLog({
          event: "material_reported",
          route: "reports",
          method: "POST",
          status: 201,
          actor: user.sub,
          materialId,
        });

        return NextResponse.json(
          {
            success: true,
            reportId: result.insertedId,
            message:
              "Your report has been successfully submitted and is under admin review.",
            moderation: {
              status: "pending_review",
            },
          },
          { status: 201 }
        );
      } catch (err) {
        auditLog({
          event: "report_create_failed",
          route: "reports",
          method: "POST",
          status: 500,
          reason: err?.message,
        });
        return NextResponse.json({ error: "Server error" }, { status: 500 });
      }
    }
  );
}

/**
 * GET /api/reports
 *
 * Admin-only: list all reports, optionally filtered by ?materialId=...&status=...
 */
export async function GET(request) {
  return withApiHardening(
    request,
    { route: "reports", rateLimit: { limit: 40, windowMs: 60_000 } },
    async () => {
      try {
        const user = await getUserFromCookie(request);
        if (!user) {
          auditLog({ event: "auth_failed", route: "reports", method: "GET", status: 401 });
          return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
        }

        if (user.role !== "admin") {
          return NextResponse.json({ error: "Forbidden" }, { status: 403 });
        }

        const url = new URL(request.url);
        const filter = {};
        const materialId = url.searchParams.get("materialId");
        const status = url.searchParams.get("status");

        if (materialId && ObjectId.isValid(materialId)) {
          filter.materialId = materialId;
        }
        if (status) {
          filter.status = status;
        }

        const db = await getDb();
        const reports = await db
          .collection("reports")
          .find(filter)
          .sort({ createdAt: -1 })
          .limit(200)
          .toArray();

        return NextResponse.json(reports);
      } catch (err) {
        return NextResponse.json({ error: "Server error" }, { status: 500 });
      }
    }
  );
}
