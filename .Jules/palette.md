## 2024-05-09 - GenomeMap Zoom Controls
**Learning:** Dynamic text indicators that update without receiving user focus (like map zoom level displays) need to be announced to screen readers.
**Action:** Attach the `aria-live="polite"` attribute to ensure screen readers announce these changes properly, and add ARIA labels to zoom icon buttons.
