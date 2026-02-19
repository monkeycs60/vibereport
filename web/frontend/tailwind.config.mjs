/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{astro,html,js,jsx,md,mdx,svelte,ts,tsx,vue}'],
  theme: {
    extend: {
      colors: {
        tokyo: {
          bg: '#1a1b26',
          surface: '#24283b',
          border: '#3b4261',
          text: '#c0caf5',
          dimmed: '#565f89',
          cyan: '#7aa2f7',
          green: '#9ece6a',
          yellow: '#e0af68',
          red: '#f7768e',
        },
      },
      fontFamily: {
        mono: ['"JetBrains Mono"', 'monospace'],
      },
    },
  },
  plugins: [],
};
