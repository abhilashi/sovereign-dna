## 2024-08-08 - Accessible Zoom Controls and Dynamic Text
**Learning:** Icon-only zoom buttons (+/-) must have `aria-label` and `title` for screen readers and tooltips. Additionally, dynamic text indicators (like map zoom levels) that update without user focus need `aria-live="polite"` to ensure screen readers announce the changes properly.
**Action:** Always add `aria-label`/`title` to icon-only controls and `aria-live="polite"` to dynamically updating status indicators.
