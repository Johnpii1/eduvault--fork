"use client";

function PreviewStat({ label, value, icon: Icon }) {
  return (
    <div className="rounded-2xl border border-gray-100 bg-white px-4 py-4 shadow-sm flex items-start gap-3">
      {Icon ? (
        <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-blue-50 text-blue-600">
          <Icon aria-hidden="true" />
        </span>
      ) : null}
      <div className="min-w-0">
        <p className="text-[10px] uppercase tracking-[0.24em] text-gray-400">{label}</p>
        <p className="mt-1.5 text-base font-semibold text-gray-900 break-words">{value}</p>
      </div>
    </div>
  );
}

export default PreviewStat;
