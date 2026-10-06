## 2025-01-28 - Zoom Control Accessibility
**Learning:** When updating UI elements that display dynamic status or state changes without a page reload (e.g., zoom level indicators), include `aria-live="polite"` and `aria-atomic="true"`. Also, icon-only buttons need descriptive `aria-label` and `title` attributes for screen readers and tooltips, along with proper `focus-visible` styles.
**Action:** Always include `aria-live` properties on dynamic status text and `aria-label`/`title`/`focus-visible` on icon-only buttons to ensure full accessibility for screen readers and keyboard users.
