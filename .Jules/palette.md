## 2024-04-12 - Auto-focus for Quick Prompts
**Learning:** In chat interfaces, when users click a "quick prompt" pill that populates an input without auto-submitting, they are often left confused if they have to click the input again to type or submit.
**Action:** Always automatically focus the primary text input (e.g. `setTimeout(() => inputRef.current?.focus(), 0)`) when a prompt pill is clicked to allow immediate editing or submission. Additionally, ensure inputs and short-text/icon buttons have appropriate `aria-label`s for screen readers.
