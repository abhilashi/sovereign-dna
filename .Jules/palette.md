## 2024-05-30 - Added ARIA labels to zoom controls
**Learning:** Icon-only buttons for zoom controls in visualization components like `GenomeMap` often lack screen reader accessibility.
**Action:** Always include descriptive `aria-label` and `title` attributes on icon-only UI elements, particularly in data-heavy visualizations where interactive controls are not visually labeled.
