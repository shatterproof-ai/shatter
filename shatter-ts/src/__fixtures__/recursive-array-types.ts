// Mutually recursive interfaces so the SAME array type instance (`Workspace[]`)
// is re-encountered during type conversion and trips convertType's cycle guard.
// Regression fixture for str-9cqde: the re-encountered array field must keep its
// `array` kind (element degraded to unknown) rather than collapsing to a bare
// `unknown`, which the core may realize as a non-array value and crash target
// code doing `.map` / `.find` on the field.

export interface Workspace {
  id: number;
  members: Person[];
}

export interface Person {
  name: string;
  workspaces: Workspace[];
}

export interface WorkspaceData {
  workspaces: Workspace[];
  title: string;
}

export function renderWorkspaces(data: WorkspaceData): string {
  return (
    data.workspaces
      .map((w) => w.members.map((m) => m.name).join(","))
      .join(";") + data.title
  );
}
