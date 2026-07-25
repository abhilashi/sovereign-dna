
## 2024-11-20 - Accessible Interactive Framer Motion Components
**Learning:** Passing accessibility props (like `role`, `tabIndex`, and `onKeyDown`) conditionally into `motion.div` can be cleanly achieved by grouping them into a single `HTMLMotionProps<"div">` object. We must also explicitly include focus visible styling and ensure `onKeyDown` correctly suppresses spacebar scrolling.
**Action:** When building interactive layout wrappers (e.g. `Card` components with `onClick`), extract conditional accessibility props into a `HTMLMotionProps` spread to maintain clean markup while satisfying a11y requirements.
