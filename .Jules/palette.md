## 2026-07-29 - Added ARIA labels and aria-live to zoom controls
**Learning:** Found that the custom interactive components like zoom buttons in visualizations (e.g. GenomeMap) lacked tooltips (title), screen reader labels (aria-label) for the icon-only (+/-) buttons, and focus-visible states. The zoom level text indicator was missing aria-live, which means it wasn't announced when updated via zoom buttons without focus.
**Action:** Always add aria-label and title to icon-only buttons, focus-visible states, and aria-live='polite' to dynamic text indicators that update without focus.
