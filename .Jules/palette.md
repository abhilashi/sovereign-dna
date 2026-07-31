## 2024-05-21 - Focus Management for Text Injectors
**Learning:** When users click a "quick prompt" or a dynamic finding (like a karyogram point) that populates an input field without auto-submitting, they expect to be able to immediately edit the text or press "Enter". If the input doesn't receive focus, it requires an extra manual click, causing friction.
**Action:** Always append `setTimeout(() => inputRef.current?.focus(), 0)` to the `onClick` handler of interactive text injectors. Using `setTimeout` ensures the state update has been processed and the DOM is ready to accept focus.
