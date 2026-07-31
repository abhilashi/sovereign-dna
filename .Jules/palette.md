
## 2024-05-18 - Aria-live for dynamic text
**Learning:** For UI elements that display dynamic states but do not receive focus (like a current zoom level indicator), they can be hidden from screen readers.
**Action:** Use `aria-live="polite"` on the text element to ensure screen readers announce these state changes when they happen without interrupting the user.
