/** @type {import('tailwindcss').Config} */
export default {
  content: ['./*.html', './src/**/*.{ts,js}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', 'sans-serif'],
        mono: ['JetBrains Mono', 'monospace'],
      },
      colors: {
        canvas:  '#070c17',
        card:    '#0d1424',
        raised:  '#111b2e',
        border:  '#1a2740',
        cyan:    '#00d4ff',
        violet:  '#8b5cf6',
        emerald: '#10b981',
        amber:   '#f59e0b',
      },
    },
  },
  plugins: [],
}
