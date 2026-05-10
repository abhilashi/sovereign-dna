## 2024-11-20 - Auto-focus for interactive text injectors
**Learning:** When interactive elements (like "Quick Prompts" or Karyogram findings) are used to populate a main query input without auto-submitting, users must manually click back into the input field to append or hit "Enter". This creates friction.
**Action:** When implementing text injectors that do not auto-submit, use `setTimeout(() => inputRef.current?.focus(), 0)` to seamlessly transition focus to the input field, allowing immediate editing or submission.
