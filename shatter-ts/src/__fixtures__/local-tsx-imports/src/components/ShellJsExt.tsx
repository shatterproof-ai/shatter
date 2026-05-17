import { Button } from "./primitives.js";
import { UserMenu } from "./UserMenu.js";

export function renderShellJsExt(user: string): string {
  return `${Button("ok")} ${UserMenu(user)}`;
}
