## 2026-06-22 - Dynamic Text Accessibility
**Learning:** Dynamic text indicators that update without user focus (like map zoom levels) need to explicitly use `aria-live="polite"` to ensure screen readers announce the changes properly.
**Action:** Add `aria-live="polite"` to status indicators and dynamically updating labels that aren't focused.
