
## 2024-05-19 - Accessible Interactive Framer Motion Elements
**Learning:** When using Framer Motion's `motion.div` for interactive components (like cards) instead of semantic `<button>` elements, they inherently lack keyboard accessibility and screen reader support, even if an `onClick` prop is provided. Furthermore, mixing conditionally hardcoded accessibility attributes directly onto the element can become messy.
**Action:** When creating custom interactive components with `motion.div` that accept an `onClick` prop:
1. Conditionally generate a typed `HTMLMotionProps<"div">` object containing `role="button"`, `tabIndex={0}`, and an `onKeyDown` handler (handling 'Enter' and 'Space', ensuring `e.preventDefault()` for 'Space').
2. Spread this object onto the `motion.div`.
3. Ensure explicit visible focus styles (`focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent focus-visible:border-accent`) are applied only when the component is interactive.
