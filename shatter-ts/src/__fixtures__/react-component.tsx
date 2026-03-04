/**
 * React component fixture — tests that executor can run components with
 * actual JSX elements via the React shim.
 *
 * StatusCard: 4 branches via conditional rendering.
 *   - status === "active" && count > 10 → "high" label in active div
 *   - status === "active" && count > 5  → "medium" label in active div
 *   - status === "active" && count <= 5 → "low" label in active div
 *   - status !== "active"               → inactive div
 *
 * InitCounter: 2 branches testing useState with function initializer.
 *   - start > 0  → positive message
 *   - start <= 0 → non-positive message
 */

import React, { useState, useMemo } from "react";

export function StatusCard(props: { status: string; count: number }) {
  const [_value] = useState(0);

  const label = useMemo(() => {
    if (props.count > 10) return "high";
    if (props.count > 5) return "medium";
    return "low";
  }, [props.count]);

  if (props.status === "active") {
    return <div className="active">{label}: {props.count}</div>;
  }
  return <div className="inactive">Inactive</div>;
}

export function InitCounter(props: { start: number }) {
  const [count] = useState(() => props.start * 2);

  if (count > 0) {
    return <span>Positive: {count}</span>;
  }
  return <span>Non-positive: {count}</span>;
}
