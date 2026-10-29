import fs from 'node:fs/promises';
import sharp from 'sharp';
const evidence='../ratatoskr-site-hero-review';
const native=evidence+'/android-current/build/ui-screenshots/ratatoskr-ui-review';
const assets='src/assets';
await sharp(native+'/approved-home-dark-fa.png').webp({quality:90}).toFile(assets+'/android-home-1.3.0.webp');
await sharp(native+'/settings-fa.png').webp({quality:90}).toFile(assets+'/android-settings-1.3.0.webp');
const desktop=await fs.readFile(evidence+'/windows-current.jpg');
const mobile=await fs.readFile(assets+'/android-home-1.3.0.webp');
const svg=`<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1050" viewBox="0 0 1600 1050" role="img" aria-labelledby="title"><title id="title">Ratatoskr 1.3.0 — Windows and Android actual interfaces</title>
<defs><filter id="shadow" x="-20%" y="-20%" width="150%" height="160%"><feDropShadow dx="0" dy="18" stdDeviation="20" flood-color="#111820" flood-opacity=".22"/></filter><clipPath id="desktop"><rect x="34" y="40" width="1344" height="840" rx="18"/></clipPath><clipPath id="phone"><rect x="1244" y="323" width="296" height="632" rx="24"/></clipPath></defs>
<rect x="33" y="39" width="1346" height="842" rx="19" fill="#24211d" stroke="#a08a65" stroke-opacity=".5" filter="url(#shadow)"/>
<image href="data:image/jpeg;base64,${desktop.toString('base64')}" x="34" y="40" width="1344" height="840" clip-path="url(#desktop)"/>
<rect x="1234" y="313" width="316" height="652" rx="34" fill="#20272b" stroke="#69716d" stroke-width="2" filter="url(#shadow)"/>
<image href="data:image/webp;base64,${mobile.toString('base64')}" x="1244" y="323" width="296" height="632" clip-path="url(#phone)"/>
</svg>`;
await fs.writeFile(assets+'/hero-1.3.0.svg',svg);
console.log('Built a single composition from unchanged current Windows and Android captures.');
