## 2025-02-28 - Focus management for interactive prompts
**Learning:** When users click prompt "pills" or interactive elements (like Karyogram findings) that populate a chat input but do not auto-submit, failing to auto-focus the input forces the user to manually click it before they can hit Enter or edit the text. This breaks the seamless interaction loop.
**Action:** Always add `setTimeout(() => inputRef.current?.focus(), 0)` inside the `onClick` handlers for quick prompts and similar dynamic text injectors so users can immediately interact with the updated input.
