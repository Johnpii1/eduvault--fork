import { describe, it, expect } from "vitest";

import {
  validateThumbnail,
  MAX_THUMBNAIL_BYTES,
  THUMBNAIL_SIZE_ERROR,
  THUMBNAIL_TYPE_ERROR,
} from "../validateThumbnail";

function fakeFile(name, size) {
  return { name, size };
}

describe("validateThumbnail", () => {
  it("accepts a missing file (thumbnails are optional)", () => {
    expect(validateThumbnail(null)).toEqual({ ok: true });
    expect(validateThumbnail(undefined)).toEqual({ ok: true });
  });

  it.each(["photo.jpg", "photo.jpeg", "photo.png", "photo.webp", "PHOTO.PNG"])(
    "accepts %s within the size limit",
    (name) => {
      expect(validateThumbnail(fakeFile(name, 1024))).toEqual({ ok: true });
    },
  );

  it("accepts a file exactly at the 5MB limit", () => {
    expect(validateThumbnail(fakeFile("edge.png", MAX_THUMBNAIL_BYTES))).toEqual({ ok: true });
  });

  it("rejects a file just over the 5MB limit", () => {
    const result = validateThumbnail(fakeFile("big.png", MAX_THUMBNAIL_BYTES + 1));
    expect(result.ok).toBe(false);
    expect(result.error).toBe(THUMBNAIL_SIZE_ERROR);
  });

  it.each(["animation.gif", "vector.svg", "raw.bmp", "document.pdf"])(
    "rejects unsupported type %s",
    (name) => {
      const result = validateThumbnail(fakeFile(name, 1024));
      expect(result.ok).toBe(false);
      expect(result.error).toBe(THUMBNAIL_TYPE_ERROR);
    },
  );

  it("rejects a file with no extension", () => {
    const result = validateThumbnail(fakeFile("noextension", 1024));
    expect(result.ok).toBe(false);
    expect(result.error).toBe(THUMBNAIL_TYPE_ERROR);
  });

  it("checks size before type so oversized images report the size error", () => {
    const result = validateThumbnail(fakeFile("big.gif", MAX_THUMBNAIL_BYTES + 1));
    expect(result.error).toBe(THUMBNAIL_SIZE_ERROR);
  });
});
