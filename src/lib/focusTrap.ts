export function focusTrap(element: HTMLElement) {
  const previous = document.activeElement as HTMLElement | null;
  const focusable = () => [
    ...element.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), select, a[href], summary, [tabindex="0"]',
    ),
  ];
  queueMicrotask(() => (focusable()[0] ?? element).focus());
  const onKey = (event: KeyboardEvent) => {
    if (event.key !== 'Tab') return;
    const items = focusable(),
      first = items[0],
      last = items.at(-1);
    if (!first) {
      event.preventDefault();
      element.focus();
    } else if (
      event.shiftKey &&
      (document.activeElement === first || document.activeElement === element)
    ) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };
  element.addEventListener('keydown', onKey);
  return {
    destroy() {
      element.removeEventListener('keydown', onKey);
      previous?.focus();
    },
  };
}
