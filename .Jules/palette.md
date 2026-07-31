## 2024-03-25 - Icon-only Submit Buttons Lack Context
**Learning:** Found an icon-only submit button using a unicode arrow (`→`) without an `aria-label` in the `ResearchWorkbench` input bar. This makes the primary action for submitting queries completely opaque to screen readers.
**Action:** When reviewing input forms or search bars, always verify that the associated submit button has an explicit, descriptive `aria-label` if it relies solely on an icon or unicode character for visual representation.
