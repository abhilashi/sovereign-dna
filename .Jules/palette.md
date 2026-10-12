## 2024-10-12 - Making framer-motion Divs Accessible

**Learning:** When adding standard interactive capabilities (e.g. `role="button"`, `tabIndex`, `onKeyDown`) conditionally to internal UI components leveraging non-semantic elements like `<motion.div>`, passing them individually can clutter JSX. Furthermore, conditionally applying structural accessibility props mixed with styling directly inside the return can get messy.

**Action:** Group related structural accessibility props into an explicitly typed config object (e.g., `HTMLMotionProps<"div">`) and apply them with object spread `{...interactiveProps}`. This keeps the JSX clean. Also, always remember to combine this with visual focus indicators like `focus-visible:ring-1` so keyboard users can actually see the interaction state, which doesn't come by default.
