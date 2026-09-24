## 2026-09-24 - Interactive Divs with Framer Motion
**Learning:** When building custom interactive components using framer-motion `motion.div`, conditionally applying accessibility attributes like `role`, `tabIndex`, and `onKeyDown` can clutter the JSX. Grouping them into an `HTMLMotionProps<"div">` object and spreading it keeps the markup clean. Framer-motion elements natively accept standard HTML DOM attributes.
**Action:** Use an `HTMLMotionProps<"div">` object to group and conditionally spread accessibility attributes on interactive motion components.
