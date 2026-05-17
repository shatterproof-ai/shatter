import { Button } from "./primitives";
import { UserMenu } from "./UserMenu";

export function renderShell(user: string): string {
  return `${Button("ok")} ${UserMenu(user)}`;
}
