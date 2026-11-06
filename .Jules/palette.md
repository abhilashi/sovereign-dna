## 2023-11-06 - Interactive Non-Semantic Elements
**Learning:** When using non-semantic elements (like `div`) as interactive components (e.g. `Card` with an `onClick` prop), they must explicitly define keyboard interaction (`onKeyDown` for Enter/Space), `role="button"`, and `tabIndex={0}`, as well as a focus state for keyboard navigation.
**Action:** Always check interactive components built on `div` or `span` for missing keyboard accessibility properties.
