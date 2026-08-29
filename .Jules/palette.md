## 2026-07-21 - Auto-focus Input for Text Injectors
**Learning:** Interactive text injectors (like Quick Prompts and Karyogram clicks) that populate inputs without auto-submitting them should automatically focus the primary input field. This prevents users from having to manually reposition their cursor or focus just to review or submit the text.
**Action:** When implementing 'Quick Prompts' or similar UI elements, add `setTimeout(() => inputRef.current?.focus(), 0)` to automatically focus the text input after setting its value.
