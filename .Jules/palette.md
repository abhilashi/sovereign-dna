## 2024-11-05 - Accessibility improvements for Genome Map Zoom Controls
**Learning:** Icon-only interactive elements (like + and - zoom controls) require explicit `aria-label` attributes to be accessible to screen readers. Elements updating status (like the current zoom level display) without page reloads need `aria-live` and `aria-atomic` attributes so screen readers can dynamically announce state changes.
**Action:** Consistently apply `aria-label` and `title` to icon-only buttons, and use `aria-live="polite"` alongside `aria-atomic="true"` on elements that display real-time status updates.
