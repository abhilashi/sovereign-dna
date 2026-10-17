## YYYY-MM-DD - [Title]
**Learning:** [UX/a11y insight]
**Action:** [How to apply next time]
## 2024-10-17 - Accessible Interactive Cards
**Learning:** Custom interactive components built with non-semantic elements like `div` (e.g., `Card.tsx` with an `onClick` prop) must manually implement full accessibility support, as they don't natively receive focus or trigger on Enter/Space like standard `<button>` tags do.
**Action:** When creating click-able containers in React/framer-motion, group accessibility props (`role: "button"`, `tabIndex: 0`, `onKeyDown` handling Enter and Space [with `e.preventDefault()`]) into an object to keep markup clean. Also, always add visual focus styles (e.g., `focus-visible:ring-1 focus-visible:ring-accent focus:outline-none`) to provide keyboard users with clear visual feedback.
