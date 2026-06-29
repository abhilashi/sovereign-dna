## 2026-06-29 - Focus input on interactive text injection
**Learning:** When interactive elements (like quick prompts or diagram nodes) populate an input field but do not auto-submit, failing to return focus to the input creates friction, forcing users to click manually before they can edit or submit. A simple setTimeout with focus restores smooth keyboard flow.
**Action:** Automatically focus the primary input field using `setTimeout(() => inputRef.current?.focus(), 0)` when text is programmatically injected to facilitate immediate keyboard interaction.
