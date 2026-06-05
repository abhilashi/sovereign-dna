## 2024-06-05 - Seamless Prompt Editing
**Learning:** In chat-like interfaces where "quick prompts" populate the text input but don't auto-submit, failing to auto-focus the input forces the user to awkwardly click back into the field before they can hit Enter or modify the prompt.
**Action:** Always pair `setQuery(prompt)` with `setTimeout(() => inputRef.current?.focus(), 0)` to maintain cognitive flow and reduce interaction friction.
