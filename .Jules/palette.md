
## 2026-07-17 - Focus Management for Text Injectors
**Learning:** When using interactive elements (like "Quick Prompts" or clickable visual findings in a Karyogram) that populate a query input without auto-submitting, users expect to be able to immediately type to modify the query or press Enter to submit. Without auto-focusing the input, users are forced to manually click into the input field, causing friction and breaking the flow.
**Action:** Automatically focus the primary text input (e.g., using `setTimeout(() => inputRef.current?.focus(), 0)`) when these elements are clicked to create a seamless UX and reduce manual clicks.
