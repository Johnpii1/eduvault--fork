"use client";

import Image from "next/image";
import { useEffect, useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import {
  FaCheckCircle,
  FaExternalLinkAlt,
  FaShoppingBag,
  FaTimes,
} from "react-icons/fa";
import { useAccount } from "wagmi";
import ConnectWalletModal from "./ConnectWalletModal";
import TransactionStatusPanel from "@/components/transactions/TransactionStatusPanel";
import { useCreatePurchase } from "@/hooks/api/usePurchases";
import { useTransactionCenter } from "@/providers/TransactionProvider";
import { TransactionStatus } from "@/lib/transactions/transaction";
import { ACCEPTED_ASSET, getExplorerTxUrl } from "@/lib/config/chain";

const SUPPORTED_ASSETS = [
  { code: ACCEPTED_ASSET, issuer: null, label: `Stellar ${ACCEPTED_ASSET}` },
];

function useQuote(materialId, asset, price) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [quote, setQuote] = useState(null);

  useEffect(() => {
    if (!materialId || !asset) return undefined;

    const loadingTimer = window.setTimeout(() => {
      setLoading(true);
      setError(null);
    }, 0);

    const timeout = window.setTimeout(() => {
      const numericPrice = Number(price || 0);
      if (!Number.isFinite(numericPrice) || numericPrice < 0) {
        setError(new Error("Invalid price"));
        setQuote(null);
      } else {
        setQuote({
          amount: numericPrice.toFixed(2),
          asset: asset.code,
          fee: asset.code === "XLM" ? 0.1 : 0.05,
        });
      }
      setLoading(false);
    }, 500);

    return () => {
      window.clearTimeout(loadingTimer);
      window.clearTimeout(timeout);
    };
  }, [materialId, asset, price]);

  return {
    loading,
    error,
    quote,
    refresh: () => setQuote(null),
  };
}

function createLocalTxHash() {
  return `tx_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 10)}`;
}

export default function BuyNowModal({
  isOpen,
  onClose,
  price,
  materialId,
  materialTitle,
  materialCreator,
}) {
  const { address } = useAccount();
  const createPurchaseMutation = useCreatePurchase();
  const {
    activeTransaction,
    beginTransaction,
    markStatus,
    confirmTransaction,
    failTransaction,
    clearTransaction,
  } = useTransactionCenter();

  const [showWallet, setShowWallet] = useState(false);
  const [email, setEmail] = useState("");
  const [purchased, setPurchased] = useState(false);
  const [web3Error, setWeb3Error] = useState(null);
  const [selectedAsset, setSelectedAsset] = useState(SUPPORTED_ASSETS[0]);

  const {
    loading: quoteLoading,
    error: quoteError,
    quote,
    refresh,
  } = useQuote(materialId, selectedAsset, price);

  const explorerHint = useMemo(
    () => activeTransaction?.explorerUrl || null,
    [activeTransaction?.explorerUrl],
  );

  const handleClose = () => {
    setShowWallet(false);
    setPurchased(false);
    setWeb3Error(null);
    clearTransaction();
    onClose();
  };

  const handlePay = async () => {
    if (!address) {
      beginTransaction({
        scope: "purchase",
        title: "Wallet approval required",
        message: "Connect your wallet to finish this purchase.",
      });
      setShowWallet(true);
      return;
    }

    const txHash = createLocalTxHash();

    try {
      setWeb3Error(null);
      beginTransaction({
        scope: "purchase",
        title: "Submitting purchase",
        message: "Recording the purchase and preparing backend reconciliation.",
      });

      markStatus(TransactionStatus.Submitting, {
        title: "Submitting purchase",
        message: "Saving the purchase request and confirming the payment intent.",
      });

      const result = await createPurchaseMutation.mutateAsync({
        buyerAddress: address,
        materialId,
        transactionHash: txHash,
        email,
      });

      const confirmedHash =
        result?.purchase?.transactionHash || result?.transactionHash || txHash;

      markStatus(TransactionStatus.PendingConfirmation, {
        txHash: confirmedHash,
        title: "Awaiting confirmation",
        message:
          "The purchase request has been submitted. We are waiting for entitlement reconciliation.",
      });

      await new Promise((resolve) => window.setTimeout(resolve, 700));

      confirmTransaction({
        txHash: confirmedHash,
        explorerUrl: getExplorerTxUrl(confirmedHash),
        title: "Purchase confirmed",
        message:
          "Your access is ready and the material has been added to your library.",
      });

      setPurchased(true);
    } catch (err) {
      const error =
        err instanceof Error
          ? err
          : new Error("Purchase failed. Please try again.");
      setWeb3Error(error);
      failTransaction(error, {
        title: "Purchase failed",
        message: error.message || "We could not complete the purchase.",
        retryable: true,
      });
    }
  };

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 0.5 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 z-40 bg-black backdrop-blur-sm"
            onClick={handleClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.92, y: 50 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.92, y: 50 }}
            className="fixed inset-0 z-50 flex items-center justify-center p-4"
          >
            <div className="relative w-full max-w-lg rounded-3xl border border-slate-200 bg-white p-6 shadow-2xl">
              <button
                type="button"
                onClick={handleClose}
                className="absolute right-4 top-4 rounded-full p-2 text-slate-400 transition hover:bg-slate-100 hover:text-slate-700"
                aria-label="Close purchase dialog"
              >
                <FaTimes />
              </button>

              {purchased ? (
                <div className="py-8 text-center">
                  <FaCheckCircle className="mx-auto mb-4 text-5xl text-emerald-500" />
                  <h2 className="mb-2 text-xl font-bold text-slate-900">
                    Purchase successful
                  </h2>
                  <p className="text-sm text-slate-600">
                    The material is now in your dashboard and ready to download.
                  </p>
                  <div className="mt-6 flex flex-col gap-2 sm:flex-row">
                    <a
                      href="/dashboard/purchases"
                      className="inline-flex flex-1 items-center justify-center gap-2 rounded-xl bg-blue-600 px-4 py-3 text-sm font-semibold text-white hover:bg-blue-700"
                    >
                      <FaShoppingBag size={14} />
                      My purchases
                    </a>
                    {explorerHint ? (
                      <a
                        href={explorerHint}
                        target="_blank"
                        rel="noreferrer"
                        className="inline-flex flex-1 items-center justify-center gap-2 rounded-xl border border-slate-200 px-4 py-3 text-sm font-semibold text-blue-600 hover:bg-slate-50"
                      >
                        Explorer <FaExternalLinkAlt size={12} />
                      </a>
                    ) : null}
                  </div>
                </div>
              ) : (
                <>
                  <div className="mb-6 pr-8">
                    <p className="text-xs font-semibold uppercase text-slate-400">
                      Checkout
                    </p>
                    <h2 className="mt-1 text-2xl font-bold text-slate-900">
                      Buy now
                    </h2>
                    <p className="mt-2 text-sm text-slate-600">
                      We will keep transaction status visible through
                      confirmation.
                    </p>
                  </div>

                  {materialTitle ? (
                    <div className="mb-4 rounded-2xl border border-slate-100 bg-slate-50 p-4">
                      <p className="text-xs font-semibold uppercase text-slate-400">
                        Material
                      </p>
                      <p className="mt-1 text-sm font-semibold text-slate-900">
                        {materialTitle}
                      </p>
                      {materialCreator ? (
                        <p className="mt-1 text-xs text-slate-500">
                          by {materialCreator}
                        </p>
                      ) : null}
                    </div>
                  ) : null}

                  <div className="mb-4">
                    <label className="mb-2 block text-xs font-semibold text-slate-600">
                      EMAIL ADDRESS
                    </label>
                    <input
                      type="email"
                      value={email}
                      onChange={(event) => setEmail(event.target.value)}
                      placeholder="Enter your email"
                      className="w-full rounded-xl border border-slate-300 px-3 py-2.5 text-sm focus:border-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-100"
                    />
                  </div>

                  <div className="mb-4">
                    <label className="mb-2 block text-xs font-semibold text-slate-600">
                      PAYMENT ASSET
                    </label>
                    <select
                      value={selectedAsset.code}
                      onChange={(event) =>
                        setSelectedAsset(
                          SUPPORTED_ASSETS.find(
                            (asset) => asset.code === event.target.value,
                          ) || SUPPORTED_ASSETS[0],
                        )
                      }
                      className="w-full rounded-xl border border-slate-300 px-3 py-2.5 text-sm focus:outline-none"
                    >
                      {SUPPORTED_ASSETS.map((asset) => (
                        <option key={asset.code} value={asset.code}>
                          {asset.label}
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="mb-5 flex items-center justify-between gap-3 rounded-2xl border border-slate-100 bg-slate-50 px-4 py-3 text-sm">
                    <span className="text-slate-600">You will pay</span>
                    {quoteLoading ? (
                      <span className="text-slate-400">Loading quote...</span>
                    ) : quoteError ? (
                      <span className="text-rose-500">Error loading quote</span>
                    ) : quote ? (
                      <div className="flex items-center gap-2 font-semibold text-slate-900">
                        <Image
                          src={
                            selectedAsset.code === "XLM"
                              ? "/images/stellar.png"
                              : "/images/celo.png"
                          }
                          alt={selectedAsset.label}
                          width={20}
                          height={20}
                        />
                        {quote.amount} {quote.asset}
                        {quote.fee ? (
                          <span className="text-xs text-slate-400">
                            +{quote.fee} fee
                          </span>
                        ) : null}
                      </div>
                    ) : (
                      <span className="text-slate-400">No quote available</span>
                    )}
                    <button
                      type="button"
                      onClick={refresh}
                      className="ml-2 text-xs font-medium text-blue-600 underline"
                    >
                      Refresh
                    </button>
                  </div>

                  <TransactionStatusPanel
                    transaction={activeTransaction}
                    onRetry={handlePay}
                    onClear={clearTransaction}
                  />

                  {web3Error ? (
                    <div className="mt-4 rounded-2xl border border-rose-200 bg-rose-50 p-4 text-sm text-rose-800">
                      <p className="font-semibold">Purchase failed</p>
                      <p className="mt-1 leading-6">{web3Error.message}</p>
                    </div>
                  ) : null}

                  <button
                    type="button"
                    onClick={handlePay}
                    disabled={
                      createPurchaseMutation.isPending || quoteLoading || !quote
                    }
                    className="mt-5 w-full rounded-2xl bg-blue-600 py-3 text-sm font-semibold text-white transition hover:bg-blue-700 disabled:opacity-60"
                  >
                    {activeTransaction?.status ===
                    TransactionStatus.PendingConfirmation
                      ? "Waiting for confirmation..."
                      : createPurchaseMutation.isPending
                        ? "Processing..."
                        : "Pay with wallet"}
                  </button>
                </>
              )}
            </div>
          </motion.div>

          <ConnectWalletModal
            isOpen={showWallet}
            onClose={() => {
              setShowWallet(false);
              if (
                !address &&
                activeTransaction?.status === TransactionStatus.WaitingWallet
              ) {
                failTransaction(new Error("Wallet connection required"), {
                  title: "Wallet connection required",
                  message: "Connect your wallet to complete this purchase.",
                  retryable: true,
                });
              }
            }}
          />
        </>
      )}
    </AnimatePresence>
  );
}
