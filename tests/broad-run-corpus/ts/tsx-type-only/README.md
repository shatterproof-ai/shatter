# tsx-type-only

Exercises two str-jeen failure classes side by side:

- `Counter.tsx` — TSX/JSX transform; `nextCount` is a normal scan target,
  `Counter` is a component-only export classified `jsx_component_only`
  (str-jeen.29 / .22).
- `types.d.ts` — declaration-only file classified `declaration_only`
  (NoTargetReason::DeclarationOnly).
