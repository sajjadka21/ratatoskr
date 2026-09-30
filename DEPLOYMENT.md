# Deployment record

Published: 2026-09-30

- Production: https://ratatoskr.hivenest.ir/
- Persian: https://ratatoskr.hivenest.ir/fa/
- Cloudflare Worker: `ratatoskr-site`
- Latest deployment ID: `0fa238c7be1b4c99bce7cc62fe7eac61`
- Static assets only; no user Worker module.
- Existing custom domain and TLS preserved; unrelated DNS and Workers unchanged.

## Latest validation

- Warm light and charcoal-green dark appearance checked in a real browser.
- Actual project icon, wordmark, four theme screenshots, settings screenshot and tokens included.
- English and Persian content, language switching, RTL and linked repository verified.
- Layout checked at 375px, 768px, 1024px and 1440px without horizontal overflow.
- Menu open/close, Escape dismissal and focus restoration checked.
- Hero and gallery theme selection synchronized; theme arrow keys checked.
- Build and JavaScript syntax checks passed.
- Production Persian page checked after deployment: both platform sections present, actual image loads, no broken loaded images or console warnings/errors.
- Public download links remain pending because GitHub has no published release. Windows and Android build success is differentiated from publication and installation testing.

The release audit and proposed publishing-workflow correction are in `docs/`. They were not applied to the app repository. Screenshot capture provenance is recorded in `APP_REFERENCE.json`.
