
## 2024-05-18 - [Framer Motion and Accessibility attributes]
**Learning:** When using Framer Motion (`motion.div`) for custom interactive components that serve as buttons, passing standard DOM accessibility attributes like `role`, `tabIndex`, and `onKeyDown` works natively but requires correct type definitions (`HTMLMotionProps<"div">`) to avoid type conflicts (such as with `onDrag`) when organizing these properties in an object.
**Action:** When conditionally applying a11y properties to a `motion.div`, group them into a typed `HTMLMotionProps<"div">` variable to cleanly spread them onto the component without breaking Framer Motion's complex type signatures or cluttering the JSX.
