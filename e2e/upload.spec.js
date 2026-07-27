import { test, expect } from "@playwright/test";
import { addAuthCookie, connectWallet } from "./support/auth.js";
import {
  mockPublishSuccess,
  mockUploadFailure,
  mockUploadSuccess,
  mockCreateMaterialFailure,
} from "./support/apiMocks.js";

const DOC_FILE = {
  name: "econ-lecture-notes.pdf",
  mimeType: "application/pdf",
  buffer: Buffer.from("%PDF-1.4\n% e2e fixture document\n"),
};

// Minimal valid 1x1px JPEG — real bytes so the browser can actually decode
// the thumbnail preview <Image>, not just satisfy the extension check.
const THUMB_FILE = {
  name: "cover.jpg",
  mimeType: "image/jpeg",
  buffer: Buffer.from(
    "/9j/4AAQSkZJRgABAQEAAAAAAAD/2wBDAAMCAgICAgMCAgIDAwMDBAYEBAQEBAgGBgUGCQgKCgkICQkKDA8MCgsOCwkJDRENDg8QEBEQCgwSExIQEw8QEBD/2wBDAQMDAwQDBAgEBAgQCwkLEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBD/wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAj/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8QAFQEBAQAAAAAAAAAAAAAAAAAAAAX/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIRAxEAPwCdABmX/9k=",
    "base64"
  ),
};

async function gotoUpload(page) {
  const subjectsLoaded = page.waitForResponse((res) => res.url().includes("/api/subjects") && res.ok());
  await page.goto("/dashboard/upload");
  await subjectsLoaded;
}

async function clickNext(page) {
  await page.getByRole("button", { name: "Next" }).click();
}

/** Fills step 1 (files) and advances — assumes a valid document is required. */
async function completeStep1(page, { thumb } = {}) {
  await page.locator("#file-upload").setInputFiles(DOC_FILE);
  if (thumb) {
    await page.locator("#thumbnail-upload").setInputFiles(thumb);
  }
  await clickNext(page);
}

/** Fills step 2 (details) and advances — only title is required. */
async function completeStep2(page, { title, description, category, subject } = {}) {
  await page.locator("#material-title").fill(title ?? "ECO 304 - Development Economics Lecture Notes");
  if (description) {
    await page.locator("#material-description").fill(description);
  }
  if (category) {
    await page.locator("#material-category").selectOption(category);
    await expect(page.locator("#material-subject")).toBeEnabled();
  }
  if (subject) {
    await page.locator("#material-subject").selectOption({ label: subject });
  }
  await clickNext(page);
}

/** Fills step 3 (pricing & rights) and advances — all fields optional. */
async function completeStep3(page, { price, usageRights } = {}) {
  if (price !== undefined) {
    await page.locator("#material-price").fill(String(price));
  }
  if (usageRights) {
    await page.locator("#material-usage-rights").selectOption(usageRights);
  }
  await clickNext(page);
}

test.describe("Material upload flow", () => {
  test.beforeEach(async ({ context, baseURL }) => {
    await addAuthCookie(context, baseURL);
  });

  test("creator can publish a material end to end (happy path)", async ({ page }) => {
    await connectWallet(page);
    await mockPublishSuccess(page);
    await gotoUpload(page);

    await expect(page.getByRole("heading", { name: "Upload Your Document" })).toBeVisible();
    await completeStep1(page, { thumb: THUMB_FILE });

    await expect(page.getByRole("heading", { name: "Material Details" })).toBeVisible();
    await completeStep2(page, {
      title: "ECO 304 - Development Economics Lecture Notes",
      description: "Comprehensive notes covering key development theories and examples.",
      category: "academic",
      subject: "Physics",
    });

    await expect(page.getByRole("heading", { name: "Pricing & Usage Rights" })).toBeVisible();
    await completeStep3(page, { price: "12.5", usageRights: "Creative Commons" });

    await expect(page.getByRole("heading", { name: "Review & Publish" })).toBeVisible();
    await expect(page.getByText("econ-lecture-notes.pdf")).toBeVisible();
    await expect(page.getByText("12.5 XLM")).toBeVisible();
    await expect(page.getByText("Physics")).toBeVisible();

    await page.getByRole("button", { name: "Publish Material" }).click();

    await expect(page.getByRole("heading", { name: "Successfully Published!" })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText("ECO 304 - Development Economics Lecture Notes")).toBeVisible();
    await expect(page.getByText("12.5 XLM")).toBeVisible();
    await expect(page.getByRole("link", { name: "View My Materials" })).toHaveAttribute(
      "href",
      "/dashboard/my-materials"
    );
  });

  test("blocks documents over the 10MB limit", async ({ page }) => {
    await gotoUpload(page);
    await page.locator("#file-upload").setInputFiles({
      name: "huge-lecture.pdf",
      mimeType: "application/pdf",
      buffer: Buffer.alloc(10 * 1024 * 1024 + 1),
    });
    await clickNext(page);

    await expect(page.getByRole("alert")).toContainText("exceeds the 10MB limit");
    await expect(page.getByRole("heading", { name: "Upload Your Document" })).toBeVisible();
  });

  test("blocks unsupported document file types", async ({ page }) => {
    await gotoUpload(page);
    await page.locator("#file-upload").setInputFiles({
      name: "installer.exe",
      mimeType: "application/octet-stream",
      buffer: Buffer.from("not a real document"),
    });
    await clickNext(page);

    await expect(page.getByRole("alert")).toContainText("Unsupported file format");
  });

  test("blocks thumbnails over the 5MB limit", async ({ page }) => {
    await gotoUpload(page);
    await page.locator("#file-upload").setInputFiles(DOC_FILE);
    await page.locator("#thumbnail-upload").setInputFiles({
      name: "huge-cover.jpg",
      mimeType: "image/jpeg",
      buffer: Buffer.alloc(5 * 1024 * 1024 + 1),
    });
    await clickNext(page);

    await expect(page.getByRole("alert")).toContainText("Thumbnail size exceeds the 5MB limit");
  });

  test("requires a title before leaving the Details step", async ({ page }) => {
    await gotoUpload(page);
    await completeStep1(page);

    await expect(page.getByRole("heading", { name: "Material Details" })).toBeVisible();
    await clickNext(page); // title left blank

    await expect(page.getByRole("alert")).toContainText("Please enter a document title");
    await expect(page.getByRole("heading", { name: "Material Details" })).toBeVisible();
  });

  test("disables Publish Material until a wallet is connected", async ({ page }) => {
    // Deliberately not calling connectWallet() — mirrors a real visitor who
    // reached the dashboard but never connected a Stellar wallet.
    await gotoUpload(page);
    await completeStep1(page);
    await completeStep2(page, { title: "Untitled Lecture Notes" });
    await completeStep3(page);

    await expect(page.getByRole("heading", { name: "Review & Publish" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Publish Material" })).toBeDisabled();
  });

  test("surfaces a friendly error when the file upload API fails", async ({ page }) => {
    await connectWallet(page);
    await mockUploadFailure(page, { status: 500, error: "Server error" });
    await gotoUpload(page);
    await completeStep1(page);
    await completeStep2(page, { title: "Retry Test Material" });
    await completeStep3(page);

    await page.getByRole("button", { name: "Publish Material" }).click();

    // useUploadFile retries retriable 5xx statuses with backoff before
    // giving up (~3s across 3 attempts) — see handleSubmit in UploadWizard.jsx.
    await expect(page.getByRole("alert")).toContainText("Server error", { timeout: 10_000 });
    await expect(page.getByRole("button", { name: "Publish Material" })).toBeVisible();
  });

  test("surfaces a friendly error when saving the listing fails", async ({ page }) => {
    await connectWallet(page);
    await mockUploadSuccess(page);
    await mockCreateMaterialFailure(page, { status: 500, error: "Server error" });
    await gotoUpload(page);
    await completeStep1(page);
    await completeStep2(page, { title: "Publish Failure Test Material" });
    await completeStep3(page);

    await page.getByRole("button", { name: "Publish Material" }).click();

    await expect(page.getByRole("alert")).toContainText("Server error", { timeout: 10_000 });
    await expect(page.getByRole("button", { name: "Publish Material" })).toBeVisible();
  });

  test("redirects unauthenticated visitors away from the upload wizard", async ({ page, context }) => {
    await context.clearCookies();
    await page.goto("/dashboard/upload");
    await expect(page).toHaveURL(/\/$/);
  });
});
