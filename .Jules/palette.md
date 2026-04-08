## 2024-03-24 - Screen Reader Labels for Icon Buttons
**Learning:** Icon-only buttons (like the `→` arrow for submitting a query) must have explicitly provided `aria-label` attributes to be perceivable by screen readers. A character like `→` or `-` is not interpreted natively as a semantic action by assistive tech. Focus rings (`focus-visible:ring-2`) are crucial alongside this to cater to visual keyboard navigators.
**Action:** Always verify `aria-label` exists on buttons that do not have text nodes, and ensure keyboard focus states are explicitly visible via Tailwind classes.
