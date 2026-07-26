"use client";

function CreatorCard({ author, creator, createdAt }) {
  const authorName = author?.name || creator || "Anonymous creator";
  const institution = author?.institution || "Independent educator";
  const level = author?.level || "All learners";
  const badgeText = author?.verified ? "Verified creator" : "Creator profile unverified";
  const badgeTone = author?.verified
    ? "text-emerald-700 bg-emerald-50 border border-emerald-200"
    : "text-amber-700 bg-amber-50 border border-amber-200";

  return (
    <div className="bg-white border border-gray-200 rounded-3xl p-5 sm:p-6 shadow-sm h-full">
      <h2 className="text-base sm:text-lg font-semibold text-gray-900 mb-4">Creator</h2>
      <div className="flex flex-col sm:flex-row sm:items-center gap-4">
        <div
          className="h-14 w-14 sm:h-16 sm:w-16 rounded-2xl bg-slate-100 text-slate-600 grid place-items-center text-xl font-semibold shrink-0"
          aria-hidden="true"
        >
          {authorName.charAt(0).toUpperCase()}
        </div>
        <div className="space-y-1 text-sm text-gray-600 min-w-0">
          <p className="text-base font-semibold text-gray-900 break-words">{authorName}</p>
          <p className="break-words">{institution}</p>
          <p className="break-words">{level}</p>
          <p
            className={`mt-2 inline-flex items-center gap-1.5 text-[10px] uppercase tracking-[0.2em] font-semibold px-2.5 py-1 rounded-full ${badgeTone}`}
          >
            {badgeText}
          </p>
        </div>
      </div>
      <div className="mt-5 grid gap-3 sm:grid-cols-2">
        <div className="rounded-2xl bg-slate-50 px-4 py-3 text-sm text-slate-700">
          <strong className="block text-[10px] uppercase tracking-[0.2em] text-slate-500">
            Uploaded
          </strong>
          <span className="mt-1 block">
            {createdAt ? new Date(createdAt).toLocaleDateString() : "Unknown date"}
          </span>
        </div>
        <div className="rounded-2xl bg-slate-50 px-4 py-3 text-sm text-slate-700">
          <strong className="block text-[10px] uppercase tracking-[0.2em] text-slate-500">
            Author type
          </strong>
          <span className="mt-1 block">{author?.department || "General"}</span>
        </div>
      </div>
    </div>
  );
}

export default CreatorCard;
