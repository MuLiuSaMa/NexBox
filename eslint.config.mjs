import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";

export default tseslint.config(
  {
    ignores: [
      "**/node_modules/**",
      "**/target/**",
      "**/dist/**",
      "**/out/**",
      // third-party / vendored / generated assets
      "NexBox-Android/**",
      "NexBox-Tools/**",
      "R560-developer/**",
      "Temp/**",
      "monitor/**",
      "aq_registry/**",
      "aq_registry_restore/**",
      "主播/**",
      "msix/**",
      "public/**",
      "icons/**",
      "docs/**",
      "power-plans/**",
      "src-tauri/resources/**",
      "installer/src-tauri/**",
      "uninstaller/src-tauri/**",
      "tools/bin/**",
      "tools/obj/**",
      ".cargo/**",
      ".codebuddy/**",
      ".github/**",
      ".husky/**",
      ".qoder/**",
      ".trae/**",
      ".workbuddy/**",
      ".workbuddy-ai/**",
      ".zcode/**",
      ".meowcode/**",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: reactHooks.configs.recommended.rules,
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.es2021,
      },
    },
  },
  {
    files: ["**/*.{js,mjs,cjs}"],
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
  },
  {
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "warn",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      "@typescript-eslint/no-explicit-any": "warn",
      "no-empty": ["error", { allowEmptyCatch: true }],
    },
  }
);
