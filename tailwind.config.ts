import type { Config } from 'tailwindcss';
import animate from 'tailwindcss-animate';
import { readTokens, tailwindColors } from './scripts/brand/build_tokens.mjs';

/**
 * design/tokens.json is the single source of truth. The token build
 * (scripts/brand/build_tokens.mjs) emits src/styles/tokens.css; this config
 * reads the same JSON, so the Tailwind names below always point at variables
 * that exist.
 *
 * Colours: every role is emitted twice, as a full colour (`--cr-primary`) for
 * plain CSS and as RGB channels (`--cr-primary-rgb: 14 94 111`) for Tailwind,
 * so opacity modifiers work: `bg-primary/90` →
 * `rgb(var(--cr-primary-rgb) / 0.9)`. shadcn/ui names (`background`,
 * `primary`, `muted`, `accent` = the neutral hover fill ...) map onto the
 * `--cr-*` roles; every role is also reachable as `cr-<role>` (`bg-cr-manila`).
 */
const tokens = readTokens();

const durations = ['instant', 'fast', 'base', 'slow', 'slower'];
const easings = ['standard', 'enter', 'exit', 'ink', 'spring'];

const config: Config = {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  // src/lib/theme.ts marks <html data-theme>, so `dark:` follows the real
  // theme setting rather than only the OS.
  darkMode: ['selector', '[data-theme="dark"]'],
  theme: {
    extend: {
      colors: tailwindColors(tokens),
      borderRadius: {
        sm: 'var(--cr-radius-sm)',
        DEFAULT: 'var(--cr-radius-md)',
        md: 'var(--cr-radius-md)',
        lg: 'var(--cr-radius-lg)',
        xl: 'var(--cr-radius-xl)',
        full: 'var(--cr-radius-full)',
      },
      fontFamily: {
        display: 'var(--cr-font-display)',
        sans: 'var(--cr-font-body)',
        body: 'var(--cr-font-body)',
        mono: 'var(--cr-font-mono)',
      },
      // The app type scale (brand §5, 13px base). `text-ui` is the default
      // UI text; `text-heading` / `text-display` pair with `font-display`.
      fontSize: {
        caption: ['12px', { lineHeight: '1.4' }],
        ui: ['13px', { lineHeight: '1.45' }],
        dense: ['14px', { lineHeight: '1.45' }],
        dialog: ['16px', { lineHeight: '1.5' }],
        title: ['20px', { lineHeight: '1.3' }],
        heading: ['24px', { lineHeight: '1.15' }],
        display: ['32px', { lineHeight: '1.05' }],
      },
      transitionDuration: {
        DEFAULT: 'var(--cr-duration-base)',
        ...Object.fromEntries(
          durations.map((d) => [d, `var(--cr-duration-${d})`]),
        ),
      },
      transitionTimingFunction: {
        DEFAULT: 'var(--cr-ease-standard)',
        ...Object.fromEntries(easings.map((e) => [e, `var(--cr-ease-${e})`])),
      },
      boxShadow: {
        raised: 'var(--cr-shadow-raised)',
        overlay: 'var(--cr-shadow-overlay)',
      },
      ringColor: { DEFAULT: 'rgb(var(--cr-ring-rgb) / <alpha-value>)' },
      height: {
        'control-sm': 'var(--cr-control-sm)',
        control: 'var(--cr-control-md)',
        'control-lg': 'var(--cr-control-lg)',
        'control-landing': 'var(--cr-control-landing)',
      },
      // src/components/ui: Radix height-driven disclosure, the marching slot
      // dash (Spinner) and the indeterminate shelf sweep (Progress).
      keyframes: {
        'collapsible-down': {
          from: { height: '0' },
          to: { height: 'var(--radix-collapsible-content-height)' },
        },
        'collapsible-up': {
          from: { height: 'var(--radix-collapsible-content-height)' },
          to: { height: '0' },
        },
        'slot-march': {
          to: { strokeDashoffset: '-14' },
        },
        'shelf-sweep': {
          from: { transform: 'translateX(-100%)' },
          to: { transform: 'translateX(250%)' },
        },
      },
      animation: {
        'collapsible-down':
          'collapsible-down var(--cr-duration-base) var(--cr-ease-standard)',
        'collapsible-up':
          'collapsible-up var(--cr-duration-base) var(--cr-ease-standard)',
        'slot-march': 'slot-march 700ms linear infinite',
        'shelf-sweep': 'shelf-sweep 1400ms var(--cr-ease-standard) infinite',
      },
    },
  },
  plugins: [animate],
};

export default config;
