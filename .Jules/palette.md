## 2024-05-22 - [Add aria-live to Dynamic Text Indicators]
**Learning:** Dynamic text indicators that update without receiving user focus (like map zoom levels) are not announced by screen readers.
**Action:** Attach the `aria-live="polite"` attribute to ensure screen readers announce these changes properly.
