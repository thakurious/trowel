-- Hyprland 0.56 Lua. Keys from hl.window_rule examples in /usr/share/hypr/hyprland.lua.
-- UI-DESIGN §7: float and center the 1040×700 search window.

-- Quickshell 0.3.1 reports class `org.quickshell`; appId is readonly, so match title.
hl.window_rule({
  name = "trowel-app",
  match = { title = "^Trowel$" },
  float = true,
  size = "1040 700",
})
hl.window_rule({
  name = "trowel-quicklook",
  match = { title = "^Trowel Quick Look$" },
  float = true,
  size = "1200 800",
})
