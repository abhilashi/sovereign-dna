## 2026-08-10 - Adding accessibility to custom Card components
**Learning:** Adding interactive accessibility attributes conditionally when 'onClick' is present using framer-motion requires spreading 'HTMLMotionProps<"div">' because framer-motion doesn't perfectly extend standard React.HTMLAttributes when it comes to draggable props.
**Action:** Use an explicit object with 'HTMLMotionProps<"div">' typing for conditional spreading, explicitly adding role, tabIndex, onKeyDown and focus-visible utilities to make 'div' buttons fully accessible.
