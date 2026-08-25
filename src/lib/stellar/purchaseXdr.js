"use client";

import {
  Address,
  BASE_FEE,
  Contract,
  TransactionBuilder,
  nativeToScVal,
  rpc,
} from "@stellar/stellar-sdk";
import {
  NETWORK_PASSPHRASE,
  PURCHASE_MANAGER_CONTRACT_ID,
  STELLAR_RPC_URL,
} from "@/lib/config/chain";

const STROOPS_PER_UNIT = 10_000_000;

async function sha256Bytes(value) {
  const encoded = new TextEncoder().encode(String(value));
  const digest = await crypto.subtle.digest("SHA-256", encoded);
  return new Uint8Array(digest);
}

async function materialIdToBytes(materialId) {
  const value = String(materialId || "");
  const hex = value.startsWith("0x") ? value.slice(2) : value;
  if (/^[0-9a-fA-F]{64}$/.test(hex)) {
    const bytes = new Uint8Array(32);
    for (let i = 0; i < bytes.length; i += 1) {
      bytes[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    return bytes;
  }
  return sha256Bytes(value);
}

function amountToStroops(amount) {
  const parsed = Number(amount);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error("Invalid purchase amount");
  }
  return BigInt(Math.round(parsed * STROOPS_PER_UNIT));
}

function resolveAssetContractId(item) {
  return (
    item.assetContractId ||
    item.paymentAssetContractId ||
    item.stellarAssetContractId ||
    process.env.NEXT_PUBLIC_STELLAR_PAYMENT_ASSET_CONTRACT_ID ||
    process.env.NEXT_PUBLIC_XLM_SAC_CONTRACT_ID ||
    ""
  );
}

export async function buildPurchaseTransactionXdr({ buyerAddress, item, transactionReference }) {
  if (!PURCHASE_MANAGER_CONTRACT_ID) {
    throw new Error("Purchase manager contract is not configured");
  }
  if (!buyerAddress) {
    throw new Error("Buyer wallet address is required");
  }

  const materialId = item._id || item.id || item.materialId;
  if (!materialId) {
    throw new Error("Cart item is missing a material id");
  }

  const assetContractId = resolveAssetContractId(item);
  if (!assetContractId) {
    throw new Error("Payment asset contract is not configured");
  }

  const server = new rpc.Server(STELLAR_RPC_URL);
  const account = await server.getAccount(buyerAddress);
  const contract = new Contract(PURCHASE_MANAGER_CONTRACT_ID);
  const materialBytes = await materialIdToBytes(materialId);
  const txRef = new TextEncoder().encode(transactionReference || `cart-${Date.now()}`);

  const operation = contract.call(
    "purchase",
    new Address(buyerAddress).toScVal(),
    nativeToScVal(materialBytes, { type: "bytes" }),
    new Address(assetContractId).toScVal(),
    nativeToScVal(amountToStroops(item.stellarPrice ?? item.price), { type: "i128" }),
    nativeToScVal(txRef, { type: "bytes" }),
  );

  const transaction = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(operation)
    .setTimeout(60)
    .build();

  const prepared = await server.prepareTransaction(transaction);
  return prepared.toXDR();
}
