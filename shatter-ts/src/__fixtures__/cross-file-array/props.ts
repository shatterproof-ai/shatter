import type { Widget } from "./other/widget.js";

export interface Props {
  items: Widget[];
  title: string;
}

export function renderWidgets(props: Props): string {
  return props.items.map((w) => w.label).join(", ") + props.title;
}
