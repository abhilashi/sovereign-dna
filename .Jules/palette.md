## 2026-11-03 - Automatic Focus for Text Injectors
**Learning:** When implementing 'Quick Prompts' or interactive text injectors that populate an input but do not auto-submit, failing to auto-focus the primary text input interrupts the keyboard workflow, forcing users to manually click or tab to the input to edit or press Enter.
**Action:** Always automatically focus the primary text input (e.g., using `setTimeout(() => inputRef.current?.focus(), 0)`) when these quick prompts or interactive injectors are clicked, to allow users to immediately interact with or submit the populated query.
