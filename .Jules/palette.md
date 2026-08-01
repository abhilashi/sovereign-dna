## 2024-08-01 - Zoom Controls Accessibility
**Learning:** Icon-only buttons (like `+` and `&minus;` for zoom controls in map visualizations) lack inherent semantic meaning for screen readers. Dynamic status text adjacent to these controls (like current zoom level) also needs ARIA attributes so its updates are announced.
**Action:** Always add `aria-label` and `title` to icon-only interactive elements. Use `aria-live="polite"` and `aria-atomic="true"` on non-focusable dynamic status indicators so screen readers announce state changes (e.g., zoom level transitions) automatically.
