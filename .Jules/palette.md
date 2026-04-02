## 2026-04-02 - [Auto-Focus in Chat Interfaces]
**Learning:** Automatically focusing the primary text input when users select pre-defined prompts (like Quick Prompts) significantly improves chat interaction UX. It allows users to immediately press Enter to submit or seamlessly modify the query without having to reposition their focus manually.
**Action:** When implementing 'Quick Prompts' or similar clickable query suggestions in chat-like interfaces, add an input auto-focus (e.g. `setTimeout(() => inputRef.current?.focus(), 0)`) within their click handlers.
