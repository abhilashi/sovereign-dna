## 2024-08-06 - [Card Component Accessibility Grouping]
**Learning:** When passing conditionally required accessibility props (like `role`, `tabIndex`, and `onKeyDown`) to an internal component (like `framer-motion`'s `motion.div`), it's cleaner to bundle them into a single typed object (e.g., `HTMLMotionProps<"div">`) and spread it into the component.
**Action:** Use this pattern to keep JSX clean and ensure that conditionally interactive components gracefully handle accessibility attributes without scattering conditional statements throughout the JSX markup.
