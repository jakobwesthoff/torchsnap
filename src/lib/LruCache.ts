// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Generic LRU Cache
//
// A simple Least Recently Used cache backed by a Map. The Map
// maintains insertion order, so eviction is always the first
// entry. Every `get` or `set` moves the entry to the end
// (most recently used position).
// =========================================================

export class LruCache<K, V> {
  private readonly map = new Map<K, V>();
  private readonly capacity: number;

  constructor(capacity: number) {
    this.capacity = capacity;
  }

  get(key: K): V | undefined {
    const value = this.map.get(key);
    if (value === undefined) {
      return undefined;
    }

    // Move to most-recently-used position by re-inserting.
    this.map.delete(key);
    this.map.set(key, value);
    return value;
  }

  set(key: K, value: V): void {
    // If the key already exists, delete it first so the new
    // insertion lands at the end (most recent).
    if (this.map.has(key)) {
      this.map.delete(key);
    }

    this.map.set(key, value);

    // Evict the least recently used entry if over capacity.
    if (this.map.size > this.capacity) {
      const oldest = this.map.keys().next().value!;
      this.map.delete(oldest);
    }
  }

  has(key: K): boolean {
    return this.map.has(key);
  }

  clear(): void {
    this.map.clear();
  }
}
