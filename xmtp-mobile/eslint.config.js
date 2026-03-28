const expoConfig = require("eslint-config-expo/flat");
const { defineConfig } = require("eslint/config");
const prettierRecommended = require("eslint-plugin-prettier/recommended");

module.exports = defineConfig([
  expoConfig,
  prettierRecommended,
  {
    rules: {
      // Allow console.warn/error but warn on console.log
      "no-console": ["warn", { allow: ["warn", "error", "info"] }],
      // Allow unused vars prefixed with _
      "@typescript-eslint/no-unused-vars": [
        "warn",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  {
    ignores: ["dist/", "android/", ".expo/", "node_modules/"],
  },
]);
