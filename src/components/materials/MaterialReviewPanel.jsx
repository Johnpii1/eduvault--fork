"use client";

import { useMemo, useState } from "react";
import { FaCheckCircle, FaRegStar, FaStar } from "react-icons/fa";
import { formatAddress } from "@/utils/formatAddress";

const MIN_COMMENT_LENGTH = 12;
const MAX_COMMENT_LENGTH = 600;
const DEFAULT_REVIEWS = [];

function normalizeReview(review, index) {
  return {
    id: review.id || review._id || `review-${index}`,
    rating: Number(review.rating) || 0,
    comment: review.comment || review.body || "",
    reviewer: review.reviewer || review.reviewerAddress || review.walletAddress || "Anonymous",
    reviewerName: review.reviewerName || review.name || "",
    verifiedBuyer: Boolean(review.verifiedBuyer || review.verified || review.hasVerifiedPurchase),
    createdAt: review.createdAt || review.date || new Date().toISOString(),
  };
}

function getEntitlementState(entitlement, hasAddress) {
  if (!hasAddress) {
    return { canSubmit: false, status: "wallet-missing" };
  }

  if (entitlement?.isLoading || entitlement?.isFetching) {
    return { canSubmit: false, status: "checking" };
  }

  if (entitlement?.isError) {
    return { canSubmit: false, status: "unavailable" };
  }

  const data = entitlement?.data;
  const hasAccess = Boolean(data?.hasAccess || data?.owned || data?.status === "active");

  return {
    canSubmit: hasAccess,
    status: hasAccess ? "verified" : "not-verified",
    source: data?.source || data?.entitlement?.source,
  };
}

function RatingStars({ value, onChange, disabled = false, describedBy }) {
  return (
    <div className="flex flex-wrap items-center gap-2" role="radiogroup" aria-describedby={describedBy}>
      {[1, 2, 3, 4, 5].map((star) => {
        const selected = value === star;
        const filled = star <= value;
        return (
          <button
            key={star}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-label={`${star} star${star === 1 ? "" : "s"}`}
            disabled={disabled}
            onClick={() => onChange(star)}
            className="min-h-11 min-w-11 rounded-full border border-amber-100 bg-amber-50 text-xl text-amber-500 transition hover:bg-amber-100 focus:outline-none focus:ring-2 focus:ring-amber-300 disabled:cursor-not-allowed disabled:opacity-60"
          >
            {filled ? <FaStar className="mx-auto" /> : <FaRegStar className="mx-auto" />}
          </button>
        );
      })}
    </div>
  );
}

function SummaryStars({ rating }) {
  const rounded = Math.round(rating);
  return (
    <div className="flex items-center gap-1 text-amber-500" aria-label={`${rating.toFixed(1)} out of 5 stars`}>
      {[1, 2, 3, 4, 5].map((star) => (
        <FaStar key={star} className={star <= rounded ? "opacity-100" : "opacity-25"} />
      ))}
    </div>
  );
}

function VerificationNotice({ state }) {
  const messages = {
    "wallet-missing": "Connect a wallet with a synced purchase to publish a verified review.",
    checking: "Checking your purchase entitlement before enabling reviews.",
    unavailable: "Purchase verification is unavailable right now, so review submission is paused.",
    "not-verified": "Only verified buyers can publish reviews for this material.",
    verified: "Your synced purchase is verified for this material.",
  };

  const tone = state.status === "verified" ? "border-emerald-200 bg-emerald-50 text-emerald-800" : "border-slate-200 bg-slate-50 text-slate-600";

  return (
    <p className={`rounded-xl border px-4 py-3 text-sm ${tone}`}>
      {messages[state.status]}
      {state.status === "verified" && state.source ? ` Source: ${state.source}.` : ""}
    </p>
  );
}

export default function MaterialReviewPanel({
  materialId,
  initialReviews = DEFAULT_REVIEWS,
  entitlement,
  currentAddress,
}) {
  const [reviews, setReviews] = useState(() => initialReviews.map(normalizeReview));
  const [rating, setRating] = useState(0);
  const [comment, setComment] = useState("");
  const [errors, setErrors] = useState({});
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [successMessage, setSuccessMessage] = useState("");

  const entitlementState = getEntitlementState(entitlement, Boolean(currentAddress));
  const averageRating = useMemo(() => {
    if (reviews.length === 0) return 0;
    const total = reviews.reduce((sum, review) => sum + review.rating, 0);
    return total / reviews.length;
  }, [reviews]);

  function validate() {
    const nextErrors = {};
    const trimmed = comment.trim();
    if (rating < 1 || rating > 5) {
      nextErrors.rating = "Choose a rating from 1 to 5 stars.";
    }
    if (!trimmed) {
      nextErrors.comment = "Write a short review before publishing.";
    } else if (trimmed.length < MIN_COMMENT_LENGTH) {
      nextErrors.comment = `Review must be at least ${MIN_COMMENT_LENGTH} characters.`;
    } else if (trimmed.length > MAX_COMMENT_LENGTH) {
      nextErrors.comment = `Review must be ${MAX_COMMENT_LENGTH} characters or fewer.`;
    }
    setErrors(nextErrors);
    return Object.keys(nextErrors).length === 0;
  }

  async function handleSubmit(event) {
    event.preventDefault();
    setSuccessMessage("");

    if (!entitlementState.canSubmit) {
      setErrors({ form: "A verified purchase is required before publishing a review." });
      return;
    }

    if (!validate()) return;

    setIsSubmitting(true);
    await new Promise((resolve) => window.setTimeout(resolve, 300));

    const nextReview = {
      id: `${materialId || "material"}-${Date.now()}`,
      rating,
      comment: comment.trim(),
      reviewer: currentAddress,
      reviewerName: "",
      verifiedBuyer: true,
      createdAt: new Date().toISOString(),
    };

    setReviews((current) => [nextReview, ...current]);
    setRating(0);
    setComment("");
    setErrors({});
    setSuccessMessage("Review published. Thanks for helping future learners choose well.");
    setIsSubmitting(false);
  }

  return (
    <section className="mt-10 rounded-3xl border border-slate-200 bg-white p-5 shadow-sm md:p-7" aria-labelledby="material-reviews-heading">
      <div className="grid gap-8 lg:grid-cols-[0.85fr_1.15fr]">
        <div>
          <p className="text-xs font-semibold uppercase tracking-[0.18em] text-blue-600">Buyer feedback</p>
          <h2 id="material-reviews-heading" className="mt-2 text-2xl font-bold text-slate-950">
            Reviews and ratings
          </h2>
          <div className="mt-5 rounded-2xl border border-slate-100 bg-slate-50 p-5">
            <div className="flex items-end gap-3">
              <span className="text-4xl font-bold text-slate-950">{averageRating ? averageRating.toFixed(1) : "0.0"}</span>
              <span className="pb-1 text-sm text-slate-500">/ 5</span>
            </div>
            <div className="mt-3">
              <SummaryStars rating={averageRating} />
            </div>
            <p className="mt-3 text-sm text-slate-600">
              {reviews.length === 0 ? "No reviews yet" : `${reviews.length} review${reviews.length === 1 ? "" : "s"}`}
            </p>
          </div>

          <form onSubmit={handleSubmit} noValidate className="mt-6 space-y-5">
            <VerificationNotice state={entitlementState} />
            {errors.form && (
              <p role="alert" className="rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                {errors.form}
              </p>
            )}

            <div>
              <label className="mb-3 block text-sm font-semibold text-slate-800">Rating</label>
              <RatingStars
                value={rating}
                onChange={(nextRating) => {
                  setRating(nextRating);
                  setErrors((current) => ({ ...current, rating: undefined, form: undefined }));
                }}
                disabled={!entitlementState.canSubmit || isSubmitting}
                describedBy={errors.rating ? "review-rating-error" : undefined}
              />
              {errors.rating && (
                <p id="review-rating-error" role="alert" className="mt-2 text-sm text-red-600">
                  {errors.rating}
                </p>
              )}
            </div>

            <div>
              <label htmlFor="review-comment" className="mb-2 block text-sm font-semibold text-slate-800">
                Review
              </label>
              <textarea
                id="review-comment"
                value={comment}
                onChange={(event) => {
                  setComment(event.target.value);
                  setErrors((current) => ({ ...current, comment: undefined, form: undefined }));
                }}
                disabled={!entitlementState.canSubmit || isSubmitting}
                rows={5}
                maxLength={MAX_COMMENT_LENGTH}
                aria-invalid={Boolean(errors.comment)}
                aria-describedby={errors.comment ? "review-comment-error" : "review-comment-help"}
                className="w-full resize-y rounded-2xl border border-slate-200 px-4 py-3 text-sm text-slate-800 shadow-sm outline-none transition focus:border-blue-500 focus:ring-2 focus:ring-blue-100 disabled:cursor-not-allowed disabled:bg-slate-50 disabled:text-slate-400"
                placeholder="Share what helped, what could be clearer, and who this material is best for."
              />
              <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
                {errors.comment ? (
                  <p id="review-comment-error" role="alert" className="text-sm text-red-600">
                    {errors.comment}
                  </p>
                ) : (
                  <p id="review-comment-help" className="text-sm text-slate-500">
                    {MIN_COMMENT_LENGTH}-{MAX_COMMENT_LENGTH} characters
                  </p>
                )}
                <span className="text-xs text-slate-400">{comment.length}/{MAX_COMMENT_LENGTH}</span>
              </div>
            </div>

            <button
              type="submit"
              disabled={!entitlementState.canSubmit || isSubmitting}
              className="min-h-11 w-full rounded-2xl bg-slate-950 px-5 py-3 text-sm font-semibold text-white transition hover:bg-slate-800 focus:outline-none focus:ring-2 focus:ring-slate-400 disabled:cursor-not-allowed disabled:bg-slate-300"
            >
              {isSubmitting ? "Publishing review..." : "Publish review"}
            </button>
            {successMessage && (
              <p role="status" className="rounded-xl border border-emerald-200 bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
                {successMessage}
              </p>
            )}
          </form>
        </div>

        <div className="min-w-0">
          <h3 className="text-lg font-semibold text-slate-950">Review history</h3>
          {reviews.length === 0 ? (
            <div className="mt-4 rounded-2xl border border-dashed border-slate-200 bg-slate-50 px-5 py-10 text-center">
              <p className="font-semibold text-slate-800">No reviews have been published yet.</p>
              <p className="mt-2 text-sm text-slate-500">Verified buyers can add the first rating after purchase sync completes.</p>
            </div>
          ) : (
            <ol className="mt-5 space-y-5">
              {reviews.map((review) => (
                <li key={review.id} className="relative rounded-2xl border border-slate-200 bg-white p-5 shadow-sm">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                      <p className="truncate text-sm font-semibold text-slate-900">
                        {review.reviewerName || formatAddress(review.reviewer, 8, 6) || "Anonymous"}
                      </p>
                      <p className="text-xs text-slate-500">
                        {new Date(review.createdAt).toLocaleDateString(undefined, {
                          month: "short",
                          day: "numeric",
                          year: "numeric",
                        })}
                      </p>
                    </div>
                    {review.verifiedBuyer && (
                      <span className="inline-flex items-center gap-1 rounded-full border border-emerald-200 bg-emerald-50 px-3 py-1 text-xs font-semibold text-emerald-700">
                        <FaCheckCircle aria-hidden="true" />
                        Verified buyer
                      </span>
                    )}
                  </div>
                  <div className="mt-3">
                    <SummaryStars rating={review.rating} />
                  </div>
                  <p className="mt-3 overflow-wrap-anywhere break-words text-sm leading-6 text-slate-700">
                    {review.comment}
                  </p>
                </li>
              ))}
            </ol>
          )}
        </div>
      </div>
    </section>
  );
}
