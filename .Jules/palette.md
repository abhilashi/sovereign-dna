## 2026-07-09 - Auto-focus inputs for interactive text injectors
**Learning:** When implementing 'Quick Prompts' or interactive text injectors that populate an input but do not auto-submit, failing to auto-focus the input forces the user to manually click it before they can type or press Enter, creating friction.
**Action:** Automatically focus the primary text input (e.g., using setTimeout(() => inputRef.current?.focus(), 0)) when such elements are clicked, allowing users to immediately submit or edit the query seamlessly.
