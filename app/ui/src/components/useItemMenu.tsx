// Opening the item menu from a list (grid, details): on right-click at the
// pointer, from the Menu key or Shift+F10 on the focused item.

import { useState } from "react";

import { ItemMenu } from "./ItemMenu";

/** The Menu key or Shift+F10: open the focused item's menu. */
export const isMenuKey = (e: React.KeyboardEvent) =>
  e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey && !e.ctrlKey && !e.altKey);

/** The item menu of a list (grid, details): which item it is open on, and
 * focus back on that item when it closes. */
export function useItemMenu(
  rowAt: (i: number) => { id: string; name: string | null } | undefined,
  focusItem: (i: number) => void,
) {
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    i: number;
    id: string;
    name: string;
  } | null>(null);
  const open = (i: number, x: number, y: number) => {
    const r = rowAt(i);
    if (r) setMenu({ x, y, i, id: r.id, name: r.name ?? "Item" });
  };
  /** From the keyboard (or a contextmenu event without a pointer): on the item. */
  const openOn = (i: number, el: Element | null) => {
    const b = el?.getBoundingClientRect();
    open(i, b ? b.left + Math.min(24, b.width / 2) : 0, b ? b.top + b.height / 2 : 0);
  };
  const element = menu && (
    <ItemMenu
      x={menu.x}
      y={menu.y}
      id={menu.id}
      name={menu.name}
      onClose={() => {
        setMenu(null);
        focusItem(menu.i);
      }}
    />
  );
  return { open, openOn, element };
}
