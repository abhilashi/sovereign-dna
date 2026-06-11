## 2024-05-17 - Add ARIA Labels to Form Elements in ResearchWorkbench
**Learning:** Found a pattern of missing ARIA labels on short-text buttons (like "OK" or '\u2192') and inputs without explicit associated `<label>` elements in `ResearchWorkbench.tsx`. These can be confusing for screen reader users as their purpose is only clear from context.
**Action:** Always ensure that icon-only, symbol-only, or very short-text buttons have descriptive `aria-label`s, and that inputs without explicit labels have `aria-label`s too.
