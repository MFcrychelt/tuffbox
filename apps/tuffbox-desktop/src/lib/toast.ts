import { writable } from "svelte/store";

export type ToastType = "success" | "error" | "info" | "warning";
export interface Toast { id: number; message: string; type: ToastType; duration: number; timestamp: number; }

let nextId = 0;

function createToasts() {
  const { subscribe, update } = writable<Toast[]>([]);
  return {
    subscribe,
    show(msg: string, type: ToastType = "info", duration = 5000) {
      const id = nextId++;
      const toast: Toast = { id, message: msg, type, duration, timestamp: Date.now() };
      update(t => [...t, toast]);
      if (duration > 0) setTimeout(() => { update(t => t.filter(x => x.id !== id)); }, duration);
    },
    dismiss(id: number) { update(t => t.filter(x => x.id !== id)); },
    success(msg: string, d = 4000) { this.show(msg, "success", d); },
    error(msg: string, d = 8000) { this.show(msg, "error", d); },
    info(msg: string, d = 4000) { this.show(msg, "info", d); },
    warning(msg: string, d = 6000) { this.show(msg, "warning", d); },
  };
}

export const toasts = createToasts();
