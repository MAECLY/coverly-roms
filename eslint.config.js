import { defineConfig } from 'eslint/config';
import js from '@eslint/js';
import globals from 'globals';
import jsxA11y from 'eslint-plugin-jsx-a11y';
import reactHooks from 'eslint-plugin-react-hooks';
import { reactRefresh } from 'eslint-plugin-react-refresh';
import tseslint from 'typescript-eslint';
import prettier from 'eslint-config-prettier/flat';

export default defineConfig(
  {
    ignores: [
      'dist',
      'coverage',
      'node_modules',
      'target',
      'src-tauri/target',
      'src-tauri/gen',
      'docs',
      '.claude/**',
    ],
  },
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      // The Rules of Hooks plus the React Compiler checks, as errors.
      reactHooks.configs.flat.recommended,
      prettier,
    ],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    plugins: {
      'react-refresh': reactRefresh.plugin,
    },
    rules: {
      'react-refresh/only-export-components': [
        'warn',
        { allowConstantExport: true },
      ],
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
    },
  },
  {
    // Accessibility gate: only JSX carries markup, so the plugin is scoped to
    // .tsx files. The recommended rules are errors from day one.
    files: ['**/*.tsx'],
    plugins: { 'jsx-a11y': jsxA11y },
    rules: jsxA11y.configs.recommended.rules,
  },
  {
    // shadcn-style primitives export their cva variants next to the component.
    files: ['src/components/ui/**/*.tsx'],
    rules: { 'react-refresh/only-export-components': 'off' },
  },
  {
    // Node scripts (icon generation, tooling).
    files: ['scripts/**/*.{js,mjs}'],
    languageOptions: {
      globals: globals.node,
    },
  },
);
