import { http, createConfig } from 'wagmi';
import { walletConnect, injected, coinbaseWallet } from 'wagmi/connectors';
import { SUPPORTED_CHAINS } from './chains';
export { walletConnectEnabled } from './configGuard';
import { walletConnectEnabled } from './configGuard';

// ── WalletConnect is optional. ─────────────────────────────────────────────────
// Set NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID in your environment to enable it.
// Leaving the variable unset (or keeping the placeholder) disables WalletConnect
// so that next build does NOT trigger a remote Reown/Web3Modal config fetch.
// See .env.example for details.
const _rawProjectId = process.env.NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID ?? '';

// Define supported chains — shared config from chains.js
export const chains = SUPPORTED_CHAINS;

// Build connector list — WalletConnect is omitted when no real project ID exists.
const connectors = [
  // Injected connector (MetaMask, browser wallets)
  injected({
    shimDisconnect: true,
  }),
  // Coinbase Wallet — always available regardless of WalletConnect status
  coinbaseWallet({
    appName: 'EduVault',
    appLogoUrl: 'https://eduvault.com/icon.png',
  }),
];

if (walletConnectEnabled) {
  connectors.push(
    walletConnect({
      projectId: _rawProjectId,
      metadata: {
        name: 'EduVault',
        description: 'Decentralized Educational Materials Sharing Platform',
        url: typeof window !== 'undefined' ? window.location.origin : '',
        icons: ['https://eduvault.com/icon.png'],
      },
      showQrModal: true,
    })
  );
}

// Configure wagmi — using shared SUPPORTED_CHAINS from chains.js
export const config = createConfig({
  chains: SUPPORTED_CHAINS,
  connectors,
  transports: Object.fromEntries(
    SUPPORTED_CHAINS.map((chain) => [chain.id, http()])
  ),
});
