/**
 * Svelte action that traps keyboard focus within the node.
 * Tab / Shift+Tab cycles through focusable elements inside.
 * Escape key fires the optional `onEscape` callback.
 *
 * Usage:
 *   <div use:trapFocus={{ onEscape: () => close() }}>
 */
export function trapFocus(node: HTMLElement, options?: { onEscape?: () => void }) {
  const selector = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  function getFocusable(): HTMLElement[] {
    return Array.from(node.querySelectorAll<HTMLElement>(selector)).filter(
      (el) => !el.closest('[inert]') && el.offsetParent !== null
    );
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      options?.onEscape?.();
      return;
    }

    if (e.key !== "Tab") return;

    const focusable = getFocusable();
    if (focusable.length === 0) return;

    const first = focusable[0];
    const last = focusable[focusable.length - 1];

    if (e.shiftKey) {
      if (document.activeElement === first) {
        e.preventDefault();
        last.focus();
      }
    } else {
      if (document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  // Focus the first focusable element on mount
  const focusable = getFocusable();
  if (focusable.length > 0) {
    // Small delay to let the modal render
    requestAnimationFrame(() => {
      const autofocus = node.querySelector<HTMLElement>("[autofocus]");
      (autofocus || focusable[0]).focus();
    });
  }

  node.addEventListener("keydown", handleKey);

  return {
    destroy() {
      node.removeEventListener("keydown", handleKey);
    },
  };
}
