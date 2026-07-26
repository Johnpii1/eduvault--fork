import { ACCEPTED_ASSET, NATIVE_ASSET } from "./chain";

/**
 * Stellar assets a buyer can pay with at checkout.
 *
 * Native XLM is always offered alongside the configured anchor asset
 * (`NEXT_PUBLIC_ACCEPTED_ASSET`, default USDC); the list is deduplicated when
 * a deployment accepts XLM directly.
 */
export function getSupportedPaymentAssets() {
  const assets = [
    { code: ACCEPTED_ASSET, issuer: null, label: `Stellar ${ACCEPTED_ASSET}` },
  ];

  if (ACCEPTED_ASSET !== NATIVE_ASSET) {
    assets.push({ code: NATIVE_ASSET, issuer: null, label: `Stellar ${NATIVE_ASSET}` });
  }

  return assets;
}
