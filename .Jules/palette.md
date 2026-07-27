## 2026-07-27 - Improve accessibility of symbol-only buttons and unlabeled inputs
**Learning:** Input fields that use placeholder text but lack a dedicated label element, and buttons that only display icons or short text (like 'OK' or an arrow symbol), cause accessibility issues for screen readers.
**Action:** Consistently add descriptive `aria-label` attributes to icon-only buttons, small action buttons without clear context, and inputs missing explicit labels to ensure the interface is accessible to all users.
