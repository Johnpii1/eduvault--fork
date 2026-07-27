import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";

const eslintConfig = defineConfig([
  ...nextVitals,
  // Override default ignores of eslint-config-next.
  globalIgnores([
    // Default ignores of eslint-config-next:
    ".next/**",
    "out/**",
    "build/**",
    "next-env.d.ts",
  ]),
  // Playwright specs and config run under Node, not the browser — declare
  // the Node globals eslint-config-next's browser-oriented ruleset doesn't.
  {
    files: ["e2e/**/*.js", "playwright.config.mjs"],
    languageOptions: {
      globals: {
        process: "readonly",
        Buffer: "readonly",
      },
    },
  },
]);

export default eslintConfig;
