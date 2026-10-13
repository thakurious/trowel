-- Hyprland 0.56 Lua. Keys verified against /usr/share/hypr/stubs/hl.meta.lua
-- HL.LayerRuleSpec: match, blur, ignore_alpha, no_anim, animation.
-- Production plugin id io.github.thakurious.trowel; IPC target is `trowel`.

hl.layer_rule({
  match = { namespace = "^trowel-(panel|popup)$" },
  no_anim = true,
  animation = "none",
  blur = true,
  ignore_alpha = 0.3,
})

-- Quickshell reports class org.quickshell; match the window title.
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

o.bind("SUPER + D", "Trowel search", "omarchy-shell trowel toggle")
