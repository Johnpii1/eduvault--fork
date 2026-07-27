import { defineConfig, devices } from "@playwright/test";

const PORT = process.env.PLAYWRIGHT_PORT || 3100;
const BASE_URL = process.env.PLAYWRIGHT_BASE_URL || `http://localhost:${PORT}`;

/**
 * E2E env values mirror the fake CI values already used in
 * .github/workflows/frontend.yml — no real Pinata/MongoDB credentials are
 * needed because the upload flow's network calls are intercepted in-test
 * (see e2e/support/apiMocks.js).
 */
const E2E_ENV = {
  CI: "true",
  NODE_ENV: "development",
  PORT: String(PORT),
  MONGODB_URI: "mongodb://localhost:27017/eduvault-e2e",
  MONGODB_DB: "eduvault-e2e",
  JWT_SECRET: "e2e-playwright-test-secret-do-not-use-in-prod",
  NEXT_PUBLIC_APP_URL: BASE_URL,
  NEXT_PUBLIC_GATEWAY_URL: "https://gateway.pinata.cloud",
  PINATA_JWT: "e2e-pinata-token",
  NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID: "e2e-walletconnect-project-id",
};

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  timeout: 30_000,

  use: {
    baseURL: BASE_URL,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },

  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],

  webServer: {
    command: "npm run dev -- --port " + PORT,
    url: BASE_URL,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
    env: E2E_ENV,
  },
});
