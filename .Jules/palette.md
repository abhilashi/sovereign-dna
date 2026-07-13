## 2024-07-13 - Card Accessibility
**Learning:** The generic `Card` component is used interactively in multiple lists (like `HealthRisks` and `Pharmacogenomics`), but relying solely on `onClick` on a `motion.div` breaks keyboard navigation and screen reader support since `div` is a non-semantic element.
**Action:** Always provide `role="button"`, `tabIndex={0}`, and `onKeyDown` handlers for custom interactive non-semantic elements, and ensure a visual focus state (e.g., `focus:outline-none focus:border-accent`) is visible.
