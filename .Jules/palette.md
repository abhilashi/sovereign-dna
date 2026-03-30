## 2024-05-24 - Interactive Component Accessibility
**Learning:** The application uses many custom interactive components built on non-semantic HTML elements like `div`. One key component is `Card`, which frequently serves as a button by passing an `onClick` prop. These non-semantic elements lack the built-in keyboard interaction and focus states that native buttons have. Simply adding `cursor-pointer` visually indicates interactivity but leaves keyboard users unable to focus or activate the element.
**Action:** When building or modifying custom interactive components (like `Card` when passed an `onClick` prop), always ensure accessibility by:
1. Adding `role="button"` and `tabIndex={0}` to make them focusable and identifiable to screen readers.
2. Adding an `onKeyDown` handler that explicitly handles 'Enter' and 'Space' key presses to trigger the same action as `onClick`. Note: call `e.preventDefault()` on 'Space' to prevent unwanted page scrolling.
3. Adding visible focus styling using Tailwind classes like `focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent` to ensure keyboard navigation is visually clear without affecting mouse interactions.
