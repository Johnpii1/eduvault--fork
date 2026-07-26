"use client";

import Image from "next/image";

function PriceLine({ price, currency, rating, reviewsCount }) {
  return (
    <div className="flex flex-wrap items-center gap-3 sm:gap-4">
      <div className="flex items-center gap-2">
        <Image
          src="/images/stellar.png"
          alt="Stellar"
          width={28}
          height={28}
          className="rounded-full"
        />
        <span className="text-xl sm:text-2xl font-bold text-gray-900">
          {price} {currency || "XLM"}
        </span>
      </div>
      <div className="flex items-center gap-2 text-sm">
        {rating && rating !== "New" ? (
          <span className="text-yellow-500">{rating}</span>
        ) : (
          <span className="text-gray-400">No ratings yet</span>
        )}
        <span className="text-gray-400">({reviewsCount || 0} reviews)</span>
      </div>
    </div>
  );
}

export default PriceLine;
