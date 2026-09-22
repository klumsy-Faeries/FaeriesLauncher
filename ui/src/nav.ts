// The launcher's navigation map. layout.json's sidebar.items refers to these
// ids to enable/disable/reorder entries without code changes (§6).

export interface NavItem {
  id: string;
  path: string;
}

export const NAV_ITEMS: NavItem[] = [
  { id: "home", path: "/" },
  { id: "instances", path: "/instances" },
  { id: "mods", path: "/mods" },
  { id: "versions", path: "/versions" },
  { id: "downloads", path: "/downloads" },
  { id: "accounts", path: "/accounts" },
  { id: "settings", path: "/settings" },
  // Not in the default sidebar (layout.json decides); reachable from the
  // command palette, since §27 makes the dashboard optional.
  { id: "performance", path: "/performance" },
];
