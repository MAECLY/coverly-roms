import type { Config } from 'tailwindcss';

/**
 * Tailwind covers layout and spacing. Colour tokens live as CSS custom
 * properties in src/styles/globals.css and are referenced via var(--cr-*).
 */
const config: Config = {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {},
  },
  plugins: [],
};

export default config;
