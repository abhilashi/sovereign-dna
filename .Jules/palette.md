## 2024-05-24 - Auto-focus inputs on quick prompt click
**Learning:** In chat-like interfaces, populating an input field via a "Quick Prompt" or an interactive visual element without automatically focusing the text input introduces UX friction. The user has to manually click the input to press Enter or edit the query.
**Action:** When setting input text programmatically via UI buttons (like quick prompts), always immediately focus the input element (e.g. `setTimeout(() => inputRef.current?.focus(), 0)`) so users can interact right away.
