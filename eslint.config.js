import stylex from "@stylexjs/eslint-plugin";
import tseslintParser from "@typescript-eslint/parser";
import reactHooks from "eslint-plugin-react-hooks";

export default [
  { ignores: ["src/routeTree.gen.ts"] },
  {
    files: ["src/**/*.{js,jsx,ts,tsx}"],
    languageOptions: {
      parser: tseslintParser,
      parserOptions: {
        ecmaFeatures: { jsx: true },
      },
    },
    plugins: {
      "@stylexjs": stylex,
      "react-hooks": reactHooks,
    },
    rules: {
      // This preset contains React Compiler diagnostics and no general ESLint rules.
      ...reactHooks.configs.flat["recommended-latest"].rules,
      "@stylexjs/valid-styles": "error",
      "@stylexjs/no-unused": "error",
      "@stylexjs/valid-shorthands": "warn",
      "@stylexjs/sort-keys": "warn",
    },
  },
];
