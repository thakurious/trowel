-- Hyprland 0.56 Lua. Keys from hl.window_rule examples in /usr/share/hypr/hyprland.lua.
-- UI-DESIGN §7: float and center the 1040×700 search window.

hl.window_rule({
  name = "trowel-app",
  match = { class = "trowel-app", title = "Trowel" },
  float = true,
  size = "1040 700",
})
