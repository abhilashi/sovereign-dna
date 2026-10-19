## 2024-10-19 - Accessible interactive Framer Motion components
**Learning:** Adding accessibility attributes (role, tabIndex, keyboard handlers) to a `motion.div` in Framer Motion can clutter the JSX. However, exposing them on the component's `Props` interface can create a misleading API if they are conditionally determined inside.
**Action:** Group these a11y attributes into a strongly typed `HTMLMotionProps<"div">` object and spread it onto the element (`<motion.div {...interactiveProps} >`). This keeps JSX clean, avoids prop pollution, and ensures type safety.
