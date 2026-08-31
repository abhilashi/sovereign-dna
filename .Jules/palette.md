
## 2026-08-31 - Interactive Card Accessibility
**Learning:** In a design system where a wrapper component like `Card` conditionally functions as an interactive element (e.g., via `onClick`), it's critical to add `role="button"`, `tabIndex={0}`, and keyboard event handlers (Enter/Space) to the underlying generic `div`. This avoids inaccessible "div buttons" which keyboard and screen reader users cannot interact with.
**Action:** When creating generic wrapper components that accept `onClick` handlers, dynamically generate and spread interactive accessibility props (like `HTMLMotionProps<"div">`) alongside visual focus states (`focus-visible:ring`).
