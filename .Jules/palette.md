## 2024-03-24 - Dynamic Text Indicators (Zoom Controls)
**Learning:** Text elements that dynamically update their content based on state (e.g., zoom levels, counts) without receiving direct keyboard focus can be completely missed by screen reader users.
**Action:** When implementing dynamic text indicators (like map zoom level displays or search result counts), always attach the `aria-live="polite"` attribute to ensure screen readers announce these changes properly without aggressively interrupting the user.
