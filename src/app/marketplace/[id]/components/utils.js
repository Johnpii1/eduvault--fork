import { FaCheckCircle, FaClock, FaExclamationTriangle } from "react-icons/fa";

export const FALLBACK_IMAGE = "/images/image2.jpg";

export function getPreviewImage(material) {
  return material.coverImageUrl || material.thumbnailUrl || material.image || FALLBACK_IMAGE;
}

export function getPreviewCounts(material) {
  return {
    outcomes: Array.isArray(material.learningOutcomes) ? material.learningOutcomes.length : 0,
    sections: Array.isArray(material.tableOfContents) ? material.tableOfContents.length : 0,
    notes: Array.isArray(material.sampleNotes) ? material.sampleNotes.length : 0,
  };
}

export function hasCoverImage(material) {
  return Boolean(material.coverImageUrl || material.thumbnailUrl || material.image);
}

export function getAverageScore(material) {
  const score = Number(material.averageScore ?? material.rating);
  return Number.isFinite(score) && score > 0 ? score.toFixed(1) : "New";
}

export function getFeedbackCount(material) {
  return Number(material.feedbackCount ?? material.reviewsCount ?? 0) || 0;
}

export function getAccessCopy(status, isLoading) {
  if (isLoading) {
    return {
      label: "Checking access",
      message: "We are checking your payment and entitlement status.",
      className: "border-slate-200 bg-slate-50 text-slate-700",
      icon: FaClock,
    };
  }
  switch (status) {
    case "active":
      return {
        label: "Access granted",
        message: "Payment is complete. This material is unlocked for your wallet.",
        className: "border-emerald-200 bg-emerald-50 text-emerald-800",
        icon: FaCheckCircle,
      };
    case "pending":
      return {
        label: "Payment pending",
        message: "Your access request started, but payment still needs to be completed.",
        className: "border-amber-200 bg-amber-50 text-amber-800",
        icon: FaClock,
      };
    case "payment_failed":
      return {
        label: "Payment incomplete",
        message: "The previous payment attempt did not complete, so access is still locked.",
        className: "border-rose-200 bg-rose-50 text-rose-800",
        icon: FaExclamationTriangle,
      };
    case "wallet_required":
      return {
        label: "Wallet required",
        message: "Connect your wallet to request access and complete payment.",
        className: "border-blue-200 bg-blue-50 text-blue-800",
        icon: FaClock,
      };
    default:
      return {
        label: "Payment required",
        message: "Start an access request from this page, then complete payment to unlock the file.",
        className: "border-slate-200 bg-white text-slate-700",
        icon: FaClock,
      };
  }
}
