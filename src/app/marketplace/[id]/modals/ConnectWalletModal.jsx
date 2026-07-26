"use client";

import { motion, AnimatePresence } from "framer-motion";
import { FaTimes } from "react-icons/fa";
import Image from "next/image";
import { useConnectModal } from "@rainbow-me/rainbowkit";
import { useWallet } from "@/hooks/useWallet";

export default function ConnectWalletModal({ isOpen, onClose }) {
  const { openConnectModal } = useConnectModal();
  const { connect: connectStellar, state: stellarState } = useWallet();
  const isStellarConnected = stellarState.status === "Connected";

  const wallets = [
    {
      name: "Stellar Wallet",
      icon: "/images/stellar.png",
      description: "Freighter, Albedo, xBull",
      onClick: () => {
        if (!isStellarConnected) {
          connectStellar();
        }
        onClose();
      },
    },
    {
      name: "MetaMask",
      icon: "/images/metamask.png",
      description: "Browser extension",
      onClick: () => {
        openConnectModal?.();
        onClose();
      },
    },
    {
      name: "Coinbase Wallet",
      icon: "/images/coinbase.png",
      description: "Coinbase extension or app",
      onClick: () => {
        openConnectModal?.();
        onClose();
      },
    },
    {
      name: "WalletConnect",
      icon: "/images/walletconnect.png",
      description: "Mobile wallets via QR",
      onClick: () => {
        openConnectModal?.();
        onClose();
      },
    },
  ];

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 0.5 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 bg-black backdrop-blur-sm z-40"
            onClick={onClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.9, y: 50 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.9, y: 50 }}
            className="fixed inset-0 flex items-center justify-center z-50"
          >
            <div className="bg-white rounded-2xl shadow-lg w-[90%] max-w-sm p-6 relative">
              <button
                onClick={onClose}
                className="absolute top-4 right-4 text-gray-400 hover:text-gray-600"
              >
                <FaTimes />
              </button>

              <h2 className="text-lg font-bold text-gray-900 mb-1 text-center">
                Connect Wallet
              </h2>
              <p className="text-sm text-gray-500 mb-6 text-center">
                {isStellarConnected
                  ? "Your Stellar wallet is connected. Connect an EVM wallet for additional features."
                  : "Get started by connecting your preferred wallet below."}
              </p>

              <div className="space-y-3">
                {wallets.map((wallet, i) => (
                  <button
                    key={i}
                    onClick={wallet.onClick}
                    className="flex justify-between items-center w-full border border-gray-200 hover:border-blue-500 hover:bg-blue-50 transition-all rounded-lg py-2.5 px-4"
                  >
                    <div className="flex items-center gap-3">
                      <Image
                        src={wallet.icon}
                        alt={wallet.name}
                        width={26}
                        height={26}
                      />
                      <div className="text-left">
                        <span className="font-medium text-sm text-gray-800 block">
                          {wallet.name}
                        </span>
                        {wallet.description && (
                          <span className="text-xs text-gray-400">{wallet.description}</span>
                        )}
                      </div>
                    </div>
                    <span className="text-gray-400">→</span>
                  </button>
                ))}
              </div>

              <div className="flex items-center gap-2 mt-5 text-xs text-gray-500 justify-center">
                <input type="checkbox" id="no-wallet" />
                <label htmlFor="no-wallet">I don&apos;t have a wallet</label>
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
