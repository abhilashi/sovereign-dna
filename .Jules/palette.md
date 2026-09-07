## 2023-10-27 - Quick Prompt Auto-Focus
**Learning:** In chat-like interfaces where Quick Prompts populate an input but do not auto-submit, failing to auto-focus the input forces the user to manually click the input to continue editing or pressing Enter.
**Action:** Always include a `setTimeout(() => inputRef.current?.focus(), 0)` when clicking quick prompts that populate an input in the future.
