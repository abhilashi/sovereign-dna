## 2024-05-18 - Input Focus Flow
**Learning:** When using Quick Prompts or interactive elements that populate an input but do not auto-submit (like Karyogram elements), the user flow is interrupted if the input does not receive focus automatically. This forces the user to manually click the input to edit the query or press enter, which reduces usability.
**Action:** Use `setTimeout(() => inputRef.current?.focus(), 0)` after setting the state of the input value on quick prompt selection or similar interactions to maintain a fluid user interaction flow.
