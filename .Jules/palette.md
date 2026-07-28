## 2024-05-24 - Polite Screen Reader Announcements for Dynamic Text
**Learning:** Dynamic text elements that act as indicators but don't receive direct focus (like map zoom levels or live stat counts) need `aria-live="polite"` to ensure screen readers announce their changes, improving situational awareness for non-sighted users.
**Action:** Always add `aria-live="polite"` to dynamic indicator text elements that update due to user action elsewhere on the screen but are not explicitly focused.
