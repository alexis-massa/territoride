import { defineApp } from 'pi-ui/vite';

export default defineApp({
  name: 'TerritoRide',
  shortName: 'TerritoRide',
  description: 'Turn your rides and hikes into conquered territory.',
  themeColor: '#3b6fd6',
  backgroundColor: '#f4f5f7',
  icons: [
    { src: 'icons/icon-192.png', sizes: '192x192', type: 'image/png', purpose: 'any' },
    { src: 'icons/icon-512.png', sizes: '512x512', type: 'image/png', purpose: 'any' },
    { src: 'icons/maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
  ],
});
