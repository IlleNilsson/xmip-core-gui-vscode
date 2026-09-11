// The shell's lint, run by `npm run lint` and by the TypeScript job of the
// verify workflow (ADR-0052 clause 6). Type-aware, over the same tsconfig the
// compile uses, so a rule sees what the compiler sees.

import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["out/**", "eslint.config.mjs"] },
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
