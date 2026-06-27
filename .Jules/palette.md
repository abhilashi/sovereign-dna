## 2024-05-18 - Add explicit ARIA labels for compact chat inputs and icon buttons
**Learning:** Compact chat interfaces (like the research workbench and ask genome feature) often omit visual labels or rely entirely on placeholder text to save space. Screen readers need explicit `aria-label` attributes on text inputs and icon-only buttons (like the submit arrow or short-text buttons like 'OK') to provide adequate context.
**Action:** Always verify that input fields without an `<label>` have an `aria-label` and ensure that icon-only buttons have an `aria-label` and a `title` for hover context.
