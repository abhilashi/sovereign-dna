## 2023-10-27 - Keyboard Accessibility for Interactive Motion Divs
**Learning:** When using Framer Motion's `motion.div` as interactive elements (like clickable Cards), they require explicit a11y attributes because they are inherently non-interactive semantic HTML.
**Action:** Always provide `role="button"`, `tabIndex={0}`, an `onKeyDown` handler for 'Enter' and 'Space' (including `e.preventDefault()` for 'Space' to prevent page scrolling), and visible focus styling using Tailwind `focus-visible` classes.
