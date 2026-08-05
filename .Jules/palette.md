## 2024-08-05 - Add aria-live to dynamic text indicators
**Learning:** Dynamic text indicators (like zoom level displays) that update without receiving user focus need `aria-live="polite"` so screen readers can announce changes properly.
**Action:** Always add `aria-live="polite"` to dynamically updating status text elements.
