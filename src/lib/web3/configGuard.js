const PLACEHOLDER = 'YOUR_PROJECT_ID';

export function isWalletConnectEnabled(projectId) {
  return Boolean(projectId && projectId.length > 0 && projectId !== PLACEHOLDER);
}

export const walletConnectEnabled = isWalletConnectEnabled(
  process.env.NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID ?? '',
);
