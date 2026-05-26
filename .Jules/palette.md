## 2024-05-26 - Interactive Cards Accessibility
**Learning:** Found that custom non-semantic wrapper elements like `Card` components used throughout the application missed semantic roles (`role="button"`) and keyboard operability when bound to interactive behaviors (`onClick`).
**Action:** When a generic wrapper receives an interaction handler, explicitly add `tabIndex`, visible focus styles, semantic role, and keydown listeners for standard trigger keys to ensure identical functionality for keyboard and screen reader users as for mouse users.
