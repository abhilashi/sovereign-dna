## 2026-06-02 - Text Injector Focus Handling
**Learning:** When implementing 'Quick Prompts' or interactive elements (like Karyogram findings) that populate a chat input without auto-submitting, failing to focus the input causes friction. Users have to manually click the input to press 'Enter' or modify the query.
**Action:** Use `setTimeout(() => inputRef.current?.focus(), 0)` when injecting text into an input to allow immediate keyboard interaction.
