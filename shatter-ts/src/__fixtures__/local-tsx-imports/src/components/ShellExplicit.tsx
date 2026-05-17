import { Button } from "./primitives.tsx";
import { UserMenu } from "./UserMenu.tsx";

export function renderShellExplicit(user: string): string {
  return `${Button("ok")} ${UserMenu(user)}`;
}
