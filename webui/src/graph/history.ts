// History manager for undo/redo — maintains a capped snapshot ring buffer.

const MAX_HISTORY_SIZE = 50;

export class HistoryManager<T> {
  private stack: T[] = [];
  private index = -1;

  /** Reset history with an initial state (called on tab load). */
  reset(state: T): void {
    this.stack = [deepClone(state)];
    this.index = 0;
  }

  /**
   * Push a new state onto the history stack. Discards any forward (redo) states.
   * Deep-clones the state before storing to prevent aliasing.
   */
  push(state: T): void {
    if (this.index < 0) {
      // Not initialised yet — treat as reset
      this.reset(state);
      return;
    }
    // Discard forward states
    this.stack.splice(this.index + 1);
    this.stack.push(deepClone(state));
    this.index = this.stack.length - 1;

    // Cap size: remove oldest entries
    if (this.stack.length > MAX_HISTORY_SIZE) {
      const excess = this.stack.length - MAX_HISTORY_SIZE;
      this.stack.splice(0, excess);
      this.index = this.stack.length - 1;
    }
  }

  canUndo(): boolean {
    return this.index > 0;
  }

  canRedo(): boolean {
    return this.index < this.stack.length - 1;
  }

  /** Move back one step and return the previous state, or null if already at start. */
  undo(): T | null {
    if (!this.canUndo()) return null;
    this.index--;
    return deepClone(this.stack[this.index]);
  }

  /** Move forward one step and return the next state, or null if already at end. */
  redo(): T | null {
    if (!this.canRedo()) return null;
    this.index++;
    return deepClone(this.stack[this.index]);
  }

  currentState(): T | null {
    return this.index >= 0 ? deepClone(this.stack[this.index]) : null;
  }
}

function deepClone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}
