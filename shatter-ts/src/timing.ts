import type { TimingPhaseSummary, TimingSummary } from "./protocol.js";

interface ActivePhase {
  readonly phasePath: string;
  readonly startMs: number;
  childMs: number;
}

/**
 * Lightweight per-request timing collector for frontend protocol handlers.
 * Aggregates stable phase names into the shared timing summary schema.
 */
export class TimingCollector {
  private readonly active: ActivePhase[] = [];
  private readonly phases = new Map<string, TimingPhaseSummary>();

  sync<T>(phasePath: string, fn: () => T): T {
    const phase: ActivePhase = {
      phasePath,
      startMs: performance.now(),
      childMs: 0,
    };
    this.active.push(phase);
    try {
      return fn();
    } finally {
      this.finish(phase);
    }
  }

  async async<T>(phasePath: string, fn: () => Promise<T>): Promise<T> {
    const phase: ActivePhase = {
      phasePath,
      startMs: performance.now(),
      childMs: 0,
    };
    this.active.push(phase);
    try {
      return await fn();
    } finally {
      this.finish(phase);
    }
  }

  toSummary(): TimingSummary | undefined {
    if (this.phases.size === 0) {
      return undefined;
    }

    return {
      phases: Array.from(this.phases.values()).sort((left, right) =>
        left.phase_path.localeCompare(right.phase_path),
      ),
    };
  }

  private finish(phase: ActivePhase): void {
    const finished = this.active.pop();
    if (finished !== phase) {
      throw new Error(`timing phase stack mismatch for ${phase.phasePath}`);
    }

    const totalMs = performance.now() - phase.startMs;
    const selfMs = Math.max(0, totalMs - phase.childMs);
    const existing = this.phases.get(phase.phasePath);
    if (existing) {
      existing.total_ms += totalMs;
      existing.self_ms += selfMs;
      existing.count += 1;
    } else {
      this.phases.set(phase.phasePath, {
        phase_path: phase.phasePath,
        total_ms: totalMs,
        self_ms: selfMs,
        count: 1,
      });
    }

    const parent = this.active[this.active.length - 1];
    if (parent) {
      parent.childMs += totalMs;
    }
  }
}
