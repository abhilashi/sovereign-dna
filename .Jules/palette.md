
## 2024-05-24 - Interactive Card Accessibility
**Learning:** Interactive components built with non-semantic elements (like `motion.div`) need extra attention to accessibility. Without `role="button"`, `tabIndex={0}`, and `onKeyDown` handlers for Space/Enter, they are entirely inaccessible to keyboard users, preventing them from interacting with items like health risk expandable cards or dashboard elements.
**Action:** When making a `div` or `motion.div` interactive via `onClick`, always conditionally inject accessibility props (role, tabIndex, onKeyDown) and add visual focus states (e.g., `focus-visible:ring-1 focus-visible:ring-accent focus-visible:outline-none`). Group these into an object and spread them to keep JSX clean.
