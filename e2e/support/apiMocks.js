/**
 * The upload wizard's two backend calls (POST /api/upload -> Pinata,
 * POST /api/materials -> MongoDB) are intercepted at the network layer
 * rather than hit for real. Reasons:
 *   - /api/upload is rate-limited to 5 requests/hour/IP in src/proxy.js,
 *     which real E2E runs would blow through in a single CI run.
 *   - Hitting real Pinata/MongoDB would require live credentials in CI
 *     and make tests flaky/non-hermetic.
 * Route-level API behavior (validation, Pinata pinning, Mongo writes) is
 * already covered by test/integration/upload-ipfs.test.js and
 * test/integration/publishing.test.js — this suite verifies the browser
 * flow: wizard navigation, client-side validation, and how the UI reacts
 * to each backend outcome.
 */

const UPLOAD_SUCCESS_BODY = {
  success: true,
  storageKey: "bafybeigdyrzt5e2e00000000000000000000000000000000000000",
  fileUrl: "https://gateway.pinata.cloud/ipfs/bafybeigdyrzt5e2e-doc",
  image: "https://gateway.pinata.cloud/ipfs/bafybeigdyrzt5e2e-thumb",
  metadata: "https://gateway.pinata.cloud/ipfs/bafybeigdyrzt5e2e-metadata",
};

const MATERIAL_CREATE_SUCCESS_BODY = {
  success: true,
  materialId: "e2e0000000000000000test",
};

export async function mockUploadSuccess(page) {
  await page.route("**/api/upload", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(UPLOAD_SUCCESS_BODY),
    });
  });
}

export async function mockUploadFailure(page, { status = 500, error = "Server error" } = {}) {
  await page.route("**/api/upload", async (route) => {
    await route.fulfill({
      status,
      contentType: "application/json",
      body: JSON.stringify({ error }),
    });
  });
}

export async function mockCreateMaterialSuccess(page) {
  await page.route("**/api/materials", async (route) => {
    if (route.request().method() !== "POST") {
      return route.continue();
    }
    await route.fulfill({
      status: 201,
      contentType: "application/json",
      body: JSON.stringify(MATERIAL_CREATE_SUCCESS_BODY),
    });
  });
}

export async function mockCreateMaterialFailure(page, { status = 500, error = "Server error" } = {}) {
  await page.route("**/api/materials", async (route) => {
    if (route.request().method() !== "POST") {
      return route.continue();
    }
    await route.fulfill({
      status,
      contentType: "application/json",
      body: JSON.stringify({ error }),
    });
  });
}

/** Wires both calls to succeed — the happy path through the whole wizard. */
export async function mockPublishSuccess(page) {
  await mockUploadSuccess(page);
  await mockCreateMaterialSuccess(page);
}
