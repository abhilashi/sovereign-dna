## 2024-05-18 - Quick Prompts Input Auto-focus
**Learning:** When implementing "Quick Prompts" or interactive text injectors that populate an input field but do not auto-submit, failing to focus the input field causes friction. Users have to manually click the input to press Enter or modify the query, disrupting the chat-like flow.
**Action:** Automatically focus the primary text input (using `setTimeout(() => inputRef.current?.focus(), 0)`) when a quick prompt is clicked. This allows users to immediately press Enter or seamlessly modify the query without repositioning focus.
