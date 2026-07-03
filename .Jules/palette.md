## 2024-05-18 - Init
## 2026-07-03 - Auto-focus inputs on text insertion
**Learning:** When implementing 'Quick Prompts' or interactive text injectors (like Karyogram findings) in chat-like interfaces that populate an input but do not auto-submit, users expect to be able to immediately edit the text or press Enter. If focus remains on the clicked button, they have to manually click back into the input field, causing friction.
**Action:** Automatically focus the primary text input (e.g., using `setTimeout(() => inputRef.current?.focus(), 0)`) when a prompt button is clicked. Always verify that `inputRef` is correctly attached to the target `<input>` element to prevent errors.
