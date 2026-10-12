-- Hyprland 0.56 Lua. Keys verified against /usr/share/hypr/stubs/hl.meta.lua
-- HL.LayerRuleSpec: match, blur, ignore_alpha, no_anim, animation.
-- Spike plugin id is io.github.thakurious.trowel-dev; IPC target is `trowel`.

hl.layer_rule({
  match = { namespace = "^trowel-(panel|popup)$" },
  no_anim = true,
  animation = "none",
  blur = true,
  ignore_alpha = 0.3,
})

o.bind("SUPER + D", "Trowel search", "omarchy-shell trowel toggle")
