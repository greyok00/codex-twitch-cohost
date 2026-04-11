const { skeleton } = require('@skeletonlabs/tw-plugin');

module.exports = {
  content: ['./index.html', './src/**/*.{svelte,ts,js}'],
  darkMode: 'class',
  theme: {
    extend: {}
  },
  plugins: [
    skeleton({
      themes: {
        preset: [{ name: 'modern', enhancements: true }]
      }
    })
  ]
};
