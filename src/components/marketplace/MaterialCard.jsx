"use client";

import { useState } from "react";
import Image from "next/image";
import Link from "next/link";
import { FaExchangeAlt, FaFilePdf, FaFilePowerpoint, FaFileWord, FaHeart, FaRegClock, FaShoppingCart, FaStar } from "react-icons/fa";
import SaveMaterialButton from "@/components/materials/SaveMaterialButton";
import ResourceStatusBadge from "@/components/materials/ResourceStatusBadge";

const FALLBACK_IMAGE = "/images/image1.jpg";

const LEVEL_MAP = {
  beginner: "Beginner", intermediate: "Intermediate",
  advanced: "Advanced", "all-levels": "All Levels",
};

function getPreviewImage(material) {
  return material.coverImageUrl || material.thumbnailUrl || material.image || FALLBACK_IMAGE;
}

function getAverageScore(material) {
  const score = Number(material.averageScore ?? material.rating);
  return Number.isFinite(score) && score > 0 ? score.toFixed(1) : "New";
}

function getFeedbackCount(material) {
  return Number(material.feedbackCount ?? material.reviewsCount ?? 0) || 0;
}

function getFileIcon(type) {
  switch (type) {
    case "pdf": return <FaFilePdf className="text-red-500" />;
    case "doc": return <FaFileWord className="text-blue-500" />;
    case "ppt": return <FaFilePowerpoint className="text-orange-500" />;
    default: return <FaFilePdf className="text-gray-500" />;
  }
}

function MaterialThumbnail({ material }) {
  const [src, setSrc] = useState(() => getPreviewImage(material));
  return (
    <Image
      src={src}
      alt={material.title}
      fill
      className="object-cover group-hover:scale-105 transition-transform duration-500"
      onError={() => setSrc(FALLBACK_IMAGE)}
      sizes="(max-width: 640px) 100vw, (max-width: 1024px) 50vw, 25vw"
    />
  );
}

export default function MaterialCard({ material, isInCart, isInComparison, onAddToCart, onAddToComparison }) {
  const materialId = material._id || material.id;
  const creatorAddress = material.userAddress || material.ownerAddress || "default";

  return (
    <article
      role="listitem"
      className="relative bg-white border border-gray-200 rounded-xl overflow-hidden shadow-sm hover:shadow-md hover:border-blue-300 transition-all duration-300 flex flex-col group"
    >
      <SaveMaterialButton material={material} />

      <Link href={`/marketplace/${materialId}`} className="relative w-full h-36 bg-gray-100 overflow-hidden block">
        <MaterialThumbnail material={material} />
        {material.subject && (
          <span className="absolute top-2 left-2 bg-white/90 backdrop-blur-sm text-gray-700 font-semibold text-[10px] px-2 py-0.5 rounded-md border border-gray-200 shadow-sm">
            {material.subject}
          </span>
        )}
        {material.level && (
          <span className="absolute top-2 right-2 bg-white/90 backdrop-blur-sm text-blue-700 font-semibold text-[10px] px-2 py-0.5 rounded-md border border-blue-200 shadow-sm">
            {LEVEL_MAP[material.level] || material.level}
          </span>
        )}
      </Link>

      <div className="p-4 flex-1 flex flex-col gap-2">
        <Link href={`/marketplace/${materialId}`}>
          <h3 className="text-sm font-bold text-gray-900 line-clamp-2 leading-snug hover:text-blue-600 transition-colors">
            {material.title}
          </h3>
        </Link>

        <p className="text-xs text-gray-500">
          by{" "}
          <Link href={`/creator/${creatorAddress}`} className="font-semibold text-blue-600 hover:underline">
            {material.author || "Anonymous"}
          </Link>
        </p>

        <ResourceStatusBadge material={material} max={3} />

        <p className="text-xs text-gray-400 line-clamp-2 leading-relaxed">
          {material.shortSummary || material.description || "No description"}
        </p>

        <div className="mt-auto pt-3 border-t border-gray-100 space-y-3">
          <div className="flex justify-between items-center">
            <div className="flex items-center gap-1">
              <FaStar className="text-yellow-400 w-3.5 h-3.5" />
              <span className="text-xs font-semibold text-gray-700">{getAverageScore(material)}</span>
              <span className="text-[11px] text-gray-400">({getFeedbackCount(material)})</span>
            </div>
            <div className="bg-blue-50 text-blue-700 px-2.5 py-1 rounded-lg text-sm font-bold border border-blue-100">
              {material.price}
              <span className="text-[10px] font-medium text-blue-500 ml-1">XLM</span>
            </div>
          </div>

          <div className="flex items-center justify-between text-[11px] text-gray-400">
            <div className="flex items-center gap-3">
              <span className="flex items-center gap-1">
                {getFileIcon(material.fileType)}
                <span className="uppercase font-medium">{material.fileType || "pdf"}</span>
              </span>
              <span className="flex items-center gap-1">
                <FaHeart className="w-3 h-3" />
                {material.likes || 0}
              </span>
              {material.pages && (
                <span className="flex items-center gap-1">
                  <FaRegClock className="w-3 h-3" />
                  {material.pages} pgs
                </span>
              )}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-2">
            <button
              onClick={() => onAddToComparison(material)}
              className={`flex items-center justify-center gap-1 py-2 rounded-lg font-bold text-[11px] border transition-all focus-visible:ring-2 focus-visible:ring-blue-500 ${
                isInComparison
                  ? "bg-amber-500 border-amber-600 text-white"
                  : "bg-white border-gray-200 text-gray-600 hover:bg-gray-50"
              }`}
            >
              <FaExchangeAlt className="w-3 h-3" />
              {isInComparison ? "Contrasted" : "Contrast"}
            </button>

            <button
              onClick={() => onAddToCart(material)}
              className={`flex items-center justify-center gap-1 py-2 rounded-lg font-bold text-[11px] border transition-all focus-visible:ring-2 focus-visible:ring-blue-500 ${
                isInCart
                  ? "bg-emerald-600 border-emerald-700 text-white"
                  : "bg-blue-600 hover:bg-blue-700 border-blue-700 text-white"
              }`}
            >
              <FaShoppingCart className="w-3 h-3" />
              {isInCart ? "In Cart" : "Add to Cart"}
            </button>
          </div>
        </div>
      </div>
    </article>
  );
}
