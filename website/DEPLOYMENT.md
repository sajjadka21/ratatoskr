# Deployment record

Latest update: 2026-10-29. Worker `ratatoskr-site`, version `3f89a06b-d343-4bd3-a176-efb78fa281ec`. Release 1.3.7 with a Linux card (.deb, .rpm, AppImage, .tar.gz) in both languages; the Worker now also keeps Linux links current, labels each button with its own version (no stale desktop version, no doubled Android version) and updates checksum links by id. All 11 live download links returned 200; English and Persian checked at 1280px and 390px, light and dark.

Latest update: 2026-10-02 (Asia/Tehran). Worker `ratatoskr-site`, deployment `fa6f7ba29956470bb45d37fde9e505e6`. The verified public 1.1.1 prerelease is available in a bilingual feature/preview section; stable 1.0.2 downloads and updater remain intact. Both live pages, preview/stable links, security headers and genuine 404 verified. All five feature cards, 390px width, light/dark modes and English/Persian verified in the browser without console warnings/errors. Proof: `../ratatoskr-1.1.1-site-proof.jpg`. Full release verification: `../ratatoskr-1.1.1-release-report.md`.

The entries below record the preceding stable deployment.

Updated: 2026-10-01

- Production: https://ratatoskr.hivenest.ir/
- Persian: https://ratatoskr.hivenest.ir/fa/
- Cloudflare Worker: ratatoskr-site
- Deployment: 55509ee7-ef73-47ad-9ef4-76911957f883
- Version: 40794533-5c76-4e9d-8670-d4de712cd3a0, 100% of traffic
- Static assets only; existing domain and HTTPS retained.
- Release: https://github.com/sajjadka21/ratatoskr/releases/tag/v1.0.2

The public site was checked after deployment: all four download links point to the published 1.0.2 assets; checksum and release-note links are correct. Both languages display the release status and date. Persian licence wording distinguishes Android GPL-3.0 from desktop PolyForm Noncommercial.

Warm light rgb(231,225,213) and dark appearance work. At a 390px viewport, download buttons remain within the page width. No console errors/warnings were observed during production-page checks. Earlier header/gallery/menu checks and screenshot provenance remain recorded in APP_REFERENCE.json and prior source history.

The app publication pipeline was repaired and applied to GitHub; all release jobs succeeded. The final downloaded APK/Windows files were independently checked. See docs/RELEASE_AUDIT.md for device-test and Authenticode limits. No unrelated DNS, Parsyar Worker or IRNIC changes were needed.
