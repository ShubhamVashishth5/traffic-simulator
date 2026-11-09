import { useSyncExternalStore } from "react";

/** A minimal observable value for React (no external state library). */
export class Store<T> {
  private listeners = new Set<() => void>();
  constructor(private value: T) {}
  get(): T {
    return this.value;
  }
  set(next: T | ((v: T) => T)): void {
    this.value = typeof next === "function" ? (next as (v: T) => T)(this.value) : next;
    for (const l of this.listeners) l();
  }
  subscribe = (l: () => void): (() => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };
}

export function useStore<T>(s: Store<T>): T {
  return useSyncExternalStore(s.subscribe, () => s.get());
}
