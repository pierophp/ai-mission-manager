import { defineConfig, lazyPlugins } from "vite-plus";
import react from "@vitejs/plugin-react";

export default defineConfig({
  fmt: {
    ignorePatterns: [
      ".agents/**",
      "docs/**",
      "idea.md",
      "package.json",
      "README.md",
      "spikes/**",
    ],
    printWidth: 80,
  },
  lint: {
    ignorePatterns: [
      ".agents/**",
      ".claude/**",
      "docs/**",
      "idea.md",
      "package.json",
      "README.md",
      "spikes/**",
    ],
    jsPlugins: [{ name: "vite-plus", specifier: "vite-plus/oxlint-plugin" }],
    rules: { "vite-plus/prefer-vite-plus-imports": "error" },
    options: { typeAware: true, typeCheck: true },
  },
  check: {
    fmt: false,
  },
  test: {
    include: ["src/**/*.{test,spec}.{js,ts,jsx,tsx}"],
    passWithNoTests: true,
  },
  plugins: lazyPlugins(() => [react()]),
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
});
