# Deployment record

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
