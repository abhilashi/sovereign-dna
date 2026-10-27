## 2024-10-27 - Keyboard Accessibility for Card Component
**Learning:** The foundational `Card` component used a non-semantic `div` for interactive states when `onClick` was provided, breaking keyboard navigation and screen reader support since `onClick` does not automatically confer keyboard interaction to `div`s.
**Action:** When a generic wrapper like `motion.div` is made interactive (conditionally receiving `onClick`), it must explicitly receive accessibility properties (`role="button"`, `tabIndex={0}`, keyboard event handlers for Enter/Space, and visible focus outlines).
