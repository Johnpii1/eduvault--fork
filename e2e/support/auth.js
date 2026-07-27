import jwt from "jsonwebtoken";

// Must match the JWT_SECRET the Playwright webServer starts `next dev` with
// (see playwright.config.mjs) so verifyDashboardToken() in src/proxy.js accepts it.
const JWT_SECRET = process.env.JWT_SECRET || "e2e-playwright-test-secret-do-not-use-in-prod";

// Same shape/convention as MOCK_WALLET_ADDRESS in tests/mocks/stellarWalletContext.js —
// not a real/funded Stellar account, just a stand-in with the right shape.
export const E2E_WALLET_ADDRESS = "GDUMMY7TESTWALLETADDRESSFORE2ETEST0000000000000000000000";
export const E2E_USER_SUB = "e2e-test-user";

/**
 * Signs an `auth_token` cookie value identical in shape to what
 * POST /api/auth/verify issues (see src/lib/auth/tokenService.js).
 */
export function signAuthToken(overrides = {}) {
  return jwt.sign(
    { sub: E2E_USER_SUB, walletAddress: E2E_WALLET_ADDRESS, ...overrides },
    JWT_SECRET,
    { expiresIn: "15m" }
  );
}

/**
 * Seeds the auth cookie the /dashboard/* route proxy requires
 * (src/proxy.js) so tests can reach the upload wizard directly, bypassing
 * the (not-yet-wired-up, see docs/creator-publishing-guide.md) wallet
 * sign-in-with-Stellar UI flow.
 */
export async function addAuthCookie(context, baseURL) {
  const url = new URL(baseURL);
  await context.addCookies([
    {
      name: "auth_token",
      value: signAuthToken(),
      domain: url.hostname,
      path: "/",
      httpOnly: true,
      sameSite: "Lax",
    },
  ]);
}

/**
 * Simulates an already-connected Stellar wallet by injecting the test seam
 * read by src/providers/WalletProvider.jsx before any app script runs.
 * Without this, useWallet().address stays null in headless Chromium (no
 * real wallet extension is installed), which is also exactly what we want
 * for the "wallet not connected" test cases — simply don't call this.
 */
export async function connectWallet(page, address = E2E_WALLET_ADDRESS) {
  await page.addInitScript((walletAddress) => {
    window.__EDUVAULT_E2E__ = { walletAddress };
  }, address);
}
