# Deployment record

Published: 2026-09-30

- Production: https://ratatoskr.hivenest.ir/
- Persian: https://ratatoskr.hivenest.ir/fa/
- Cloudflare Worker: `ratatoskr-site`
- Latest deployment ID: `9400e38d9e434a068d784d09b152e3ba`
- Delivery mode: static assets only, no user Worker module.
- Custom domain and managed TLS configured through the authorized Cloudflare plugin.
- Existing DNS and unrelated Workers were preserved.

Validation:

- Floating header, mobile navigation, Escape dismissal, section focus, and language switching verified.
- Hero theme controls synchronize the preview, theme name, frame colors, and theme tabs.
- Latest layout checked at 375px, 768px, 1024px and 1440px without horizontal overflow.
- Entry, menu, FAQ and theme transitions respect reduced-motion preferences in CSS and JavaScript.
- Latest production Persian page checked in light and dark modes without console errors.

- Redesigned light landing verified at 375px, 390px, 768px, 1280px and 1440px widths with no horizontal overflow.
- Manrope and Vazirmatn served locally; Ant Design icons embedded as decorative SVGs with license included.
- New theme keyboard selection, FAQ and language switching verified. Live Persian page shows the redesign without console warnings or errors.

- English and Persian pages verified in a real browser.
- Desktop 1280px and mobile 390px layouts checked for horizontal overflow.
- Theme clicks, keyboard navigation, RTL navigation and saved selection checked.
- FAQ expansion and language switch checked.
- Current download section accurately shows an unreleased product.
- Production Persian page loads without console errors or broken loaded images.
- JavaScript syntax and static build checked.

The application itself is still being completed separately. Replace design previews and update product claims when confirmed builds are available.
