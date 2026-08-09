## 2025-02-27 - Interactive Card Accessibility
**Learning:** Interactive custom components (like `div` acting as a button) require manual keyboard accessibility logic, including `role`, `tabIndex`, and an `onKeyDown` handler for Enter and Space (preventing page scroll for Space).
**Action:** When adding interactivity to a non-semantic element via `onClick`, immediately add keyboard support and visible focus states (e.g., `focus:outline-none focus:ring-1 focus:ring-accent focus:border-accent`) to ensure the component is accessible.
