## 2024-05-18 - Making generic layout components semantic
**Learning:** Found that basic generic container components (`Card` using `motion.div`) are sometimes used as fully interactive trigger elements via an optional `onClick` prop, without proper semantics or keyboard bindings.
**Action:** Always check standard layout/wrapper components (like Card or Panel) to see if they conditionally accept interaction handlers, and if so, dynamically inject accessibility attributes (`role="button"`, `tabIndex`, and `onKeyDown` for Space/Enter) based on the presence of those props.
