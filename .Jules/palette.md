## 2024-09-18 - Automatically Focus Input After Interactive Text Injection
**Learning:** Users experience friction when clicking a quick prompt or interactive visualization item that populates an input field, only to have to manually click the field again to edit or submit it.
**Action:** When implementing 'Quick Prompts' or interactive text injectors that populate an input but do not auto-submit, use `setTimeout(() => inputRef.current?.focus(), 0)` immediately after setting the state. Ensure the `ref` is attached to the target input.
