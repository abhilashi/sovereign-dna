
## 2026-08-04 - Adding accessibility props to Framer Motion components
**Learning:** When conditionally applying multiple accessibility attributes (like `role`, `tabIndex`, `onKeyDown`) to an internal `framer-motion` element in a custom component, wrapping them into a typed `HTMLMotionProps<"div">` object and spreading it (`{...interactiveProps}`) ensures types align properly. Doing this on a `motion.div` is necessary to prevent runtime type conflicts and keeps JSX tidy. Do not expose these a11y props on the component interface if the component handles them internally.
**Action:** Always group conditional accessibility props inside an explicitly typed `HTMLMotionProps<"div">` object when building custom interactive `framer-motion` wrapper components.
