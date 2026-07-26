import { afterEach, describe, it, expect, vi } from "vitest";

const ORIGINAL_ACCEPTED_ASSET = process.env.NEXT_PUBLIC_ACCEPTED_ASSET;

afterEach(() => {
  if (ORIGINAL_ACCEPTED_ASSET === undefined) {
    delete process.env.NEXT_PUBLIC_ACCEPTED_ASSET;
  } else {
    process.env.NEXT_PUBLIC_ACCEPTED_ASSET = ORIGINAL_ACCEPTED_ASSET;
  }
  vi.resetModules();
});

describe("getSupportedPaymentAssets", () => {
  it("offers the accepted anchor asset plus native XLM by default", async () => {
    vi.resetModules();
    delete process.env.NEXT_PUBLIC_ACCEPTED_ASSET;
    const { getSupportedPaymentAssets } = await import("../assets.js");

    const assets = getSupportedPaymentAssets();
    expect(assets.map((a) => a.code)).toEqual(["USDC", "XLM"]);
    expect(assets[0]).toEqual({ code: "USDC", issuer: null, label: "Stellar USDC" });
    expect(assets[1]).toEqual({ code: "XLM", issuer: null, label: "Stellar XLM" });
  });

  it("deduplicates when the deployment accepts XLM directly", async () => {
    vi.resetModules();
    process.env.NEXT_PUBLIC_ACCEPTED_ASSET = "XLM";
    const { getSupportedPaymentAssets } = await import("../assets.js");

    const assets = getSupportedPaymentAssets();
    expect(assets.map((a) => a.code)).toEqual(["XLM"]);
  });
});
