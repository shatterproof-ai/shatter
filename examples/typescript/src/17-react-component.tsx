/**
 * React component with conditional rendering branches.
 *
 * EXPECTED BRANCHES for UserGreeting:
 *   1. user.role === "admin"  → admin badge + welcome
 *   2. user.role === "member" → member greeting
 *   3. default role           → guest prompt
 *   4. user.name is truthy    → personalized name
 *   5. user.name is falsy     → "there" fallback
 */

import React, { useMemo } from "react";

interface User {
  name: string;
  role: string;
}

export function UserGreeting(props: { user: User }) {
  const displayName = useMemo(
    () => (props.user.name ? props.user.name : "there"),
    [props.user.name],
  );

  if (props.user.role === "admin") {
    return (
      <div className="admin">
        <span className="badge">Admin</span>
        <p>Welcome back, {displayName}!</p>
      </div>
    );
  }

  if (props.user.role === "member") {
    return (
      <div className="member">
        <p>Hello, {displayName}!</p>
      </div>
    );
  }

  return (
    <div className="guest">
      <p>Hi {displayName}, please sign up.</p>
    </div>
  );
}
