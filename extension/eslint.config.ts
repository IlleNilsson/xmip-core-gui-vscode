// The shell's lint, run by `npm run lint` and by the TypeScript job of the
// verify workflow (ADR-0052 clause 6). Type-aware, over the same tsconfig the
// compile uses, so a rule sees what the compiler sees. TypeScript, as the
// estate holds no hand-written JavaScript (the owner, 2026-10-22: *But not
// JavaScript.*; ADR-0014, amendment 2026-10-22): ESLint reads it through
// Node's own type stripping, which ESLint's flag unstable_native_nodejs_ts_config
// turns on, so no loader of a third party is needed.

import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["out/**", "eslint.config.ts"] },
  eslint.configs.recommended,
  ...tseslint.configs.recommendedTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
);
