declare -A __omarchy_pkgs=()
mapfile -t __omarchy_pkg_names < <({ pacman -Qq; LC_ALL=C pacman -Qi | awk '/^[A-Za-z]/ { provides = ($0 ~ /^Provides/); sub(/^[^:]*: /, "") } provides && $0 != "None" { n = split($0, p, " "); for (i = 1; i <= n; i++) { sub(/[<>=].*/, "", p[i]); print p[i] } }'; } 2>/dev/null)
for __omarchy_pkg in "${__omarchy_pkg_names[@]}"; do __omarchy_pkgs[$__omarchy_pkg]=1; done
__omarchy_pkg_has() { [[ -n ${__omarchy_pkgs[$1]-} ]] && return 0; [[ $1 == *[\<\>=]* ]] && { pacman -Q "$1" &>/dev/null; return; }; return 1; }
omarchy-pkg-present() { local p; for p in "$@"; do __omarchy_pkg_has "$p" || return 1; done; return 0; }
omarchy-pkg-missing() { local p; for p in "$@"; do __omarchy_pkg_has "$p" || return 0; done; return 1; }
omarchy-cmd-present() { local c; for c in "$@"; do command -v "$c" &>/dev/null || return 1; done; return 0; }
omarchy-cmd-missing() { local c; for c in "$@"; do command -v "$c" &>/dev/null || return 0; done; return 1; }
__omarchy_read_0=$(omarchy-channel-current 2>/dev/null) || :
__omarchy_read_1=$(omarchy-default-agent 2>/dev/null) || :
__omarchy_read_2=$(omarchy-default-browser 2>/dev/null) || :
__omarchy_read_3=$(omarchy-default-editor 2>/dev/null) || :
__omarchy_read_4=$(omarchy-default-terminal 2>/dev/null) || :
__omarchy_read_5=$(omarchy-dns 2>/dev/null) || :
if { ! omarchy-toggle-enabled suspend-off; } >/dev/null 2>&1; then echo system.suspend:w:1; else echo system.suspend:w:0; fi
if { omarchy-hibernation-available; } >/dev/null 2>&1; then echo system.hibernate:w:1; else echo system.hibernate:w:0; fi
if { pgrep -f '^gpu-screen-recorder'; } >/dev/null 2>&1; then echo trigger.capture.screenrecord.stop:w:1; else echo trigger.capture.screenrecord.stop:w:0; fi
if { omarchy-hw-webcam; } >/dev/null 2>&1; then echo trigger.capture.screenrecord.webcam:w:1; else echo trigger.capture.screenrecord.webcam:w:0; fi
if { omarchy-hw-laptop; } >/dev/null 2>&1; then echo trigger.hardware.laptop-display:w:1; else echo trigger.hardware.laptop-display:w:0; fi
if { omarchy-hw-laptop; } >/dev/null 2>&1; then echo trigger.hardware.mirror-display:w:1; else echo trigger.hardware.mirror-display:w:0; fi
if { omarchy-hw-hybrid-gpu; } >/dev/null 2>&1; then echo trigger.hardware.hybrid-gpu:w:1; else echo trigger.hardware.hybrid-gpu:w:0; fi
if { omarchy-hw-touchpad; } >/dev/null 2>&1; then echo trigger.hardware.touchpad:w:1; else echo trigger.hardware.touchpad:w:0; fi
if { omarchy-hw-dell-xps-haptic-touchpad && omarchy-cmd-present dell-xps-touchpad-haptics; } >/dev/null 2>&1; then echo trigger.hardware.touchpad-haptics.low:w:1; else echo trigger.hardware.touchpad-haptics.low:w:0; fi
if { [[ "$(dell-xps-touchpad-haptics get)" == "low" ]]; } >/dev/null 2>&1; then echo trigger.hardware.touchpad-haptics.low:c:1; else echo trigger.hardware.touchpad-haptics.low:c:0; fi
if { omarchy-hw-dell-xps-haptic-touchpad && omarchy-cmd-present dell-xps-touchpad-haptics; } >/dev/null 2>&1; then echo trigger.hardware.touchpad-haptics.mid:w:1; else echo trigger.hardware.touchpad-haptics.mid:w:0; fi
if { [[ "$(dell-xps-touchpad-haptics get)" == "mid" ]]; } >/dev/null 2>&1; then echo trigger.hardware.touchpad-haptics.mid:c:1; else echo trigger.hardware.touchpad-haptics.mid:c:0; fi
if { omarchy-hw-dell-xps-haptic-touchpad && omarchy-cmd-present dell-xps-touchpad-haptics; } >/dev/null 2>&1; then echo trigger.hardware.touchpad-haptics.high:w:1; else echo trigger.hardware.touchpad-haptics.high:w:0; fi
if { [[ "$(dell-xps-touchpad-haptics get)" == "high" ]]; } >/dev/null 2>&1; then echo trigger.hardware.touchpad-haptics.high:c:1; else echo trigger.hardware.touchpad-haptics.high:c:0; fi
if { omarchy-hw-touchscreen; } >/dev/null 2>&1; then echo trigger.hardware.touchscreen:w:1; else echo trigger.hardware.touchscreen:w:0; fi
if { omarchy-hw-laptop; } >/dev/null 2>&1; then echo trigger.toggle.battery-percentage:w:1; else echo trigger.toggle.battery-percentage:w:0; fi
if { [[ -f ~/.config/hypr/bindings.lua ]]; } >/dev/null 2>&1; then echo setup.keybindings:w:1; else echo setup.keybindings:w:0; fi
if { [[ -f ~/.config/hypr/input.lua ]]; } >/dev/null 2>&1; then echo setup.input:w:1; else echo setup.input:w:0; fi
if { [[ "${__omarchy_read_5}" == "DHCP" ]]; } >/dev/null 2>&1; then echo setup.network.dns.dhcp:c:1; else echo setup.network.dns.dhcp:c:0; fi
if { [[ "${__omarchy_read_5}" == "Cloudflare" ]]; } >/dev/null 2>&1; then echo setup.network.dns.cloudflare:c:1; else echo setup.network.dns.cloudflare:c:0; fi
if { [[ "${__omarchy_read_5}" == "Google" ]]; } >/dev/null 2>&1; then echo setup.network.dns.google:c:1; else echo setup.network.dns.google:c:0; fi
if { [[ "${__omarchy_read_5}" == "Custom" ]]; } >/dev/null 2>&1; then echo setup.network.dns.custom:c:1; else echo setup.network.dns.custom:c:0; fi
if { [[ $(omarchy-network-status) == wifi* ]]; } >/dev/null 2>&1; then echo setup.network.qr:w:1; else echo setup.network.qr:w:0; fi
if { [[ "${__omarchy_read_1}" == "claude" ]]; } >/dev/null 2>&1; then echo setup.default.agent.claude:c:1; else echo setup.default.agent.claude:c:0; fi
if { [[ "${__omarchy_read_1}" == "codex" ]]; } >/dev/null 2>&1; then echo setup.default.agent.codex:c:1; else echo setup.default.agent.codex:c:0; fi
if { [[ "${__omarchy_read_1}" == "copilot" ]]; } >/dev/null 2>&1; then echo setup.default.agent.copilot:c:1; else echo setup.default.agent.copilot:c:0; fi
if { [[ "${__omarchy_read_1}" == "crush" ]]; } >/dev/null 2>&1; then echo setup.default.agent.crush:c:1; else echo setup.default.agent.crush:c:0; fi
if { [[ "${__omarchy_read_1}" == "cursor-agent" ]]; } >/dev/null 2>&1; then echo setup.default.agent.cursor-agent:c:1; else echo setup.default.agent.cursor-agent:c:0; fi
if { [[ "${__omarchy_read_1}" == "gemini" ]]; } >/dev/null 2>&1; then echo setup.default.agent.gemini:c:1; else echo setup.default.agent.gemini:c:0; fi
if { [[ "${__omarchy_read_1}" == "grok" ]]; } >/dev/null 2>&1; then echo setup.default.agent.grok:c:1; else echo setup.default.agent.grok:c:0; fi
if { [[ "${__omarchy_read_1}" == "hermes" ]]; } >/dev/null 2>&1; then echo setup.default.agent.hermes:c:1; else echo setup.default.agent.hermes:c:0; fi
if { [[ "${__omarchy_read_1}" == "muse" ]]; } >/dev/null 2>&1; then echo setup.default.agent.muse:c:1; else echo setup.default.agent.muse:c:0; fi
if { [[ "${__omarchy_read_1}" == "omp" ]]; } >/dev/null 2>&1; then echo setup.default.agent.omp:c:1; else echo setup.default.agent.omp:c:0; fi
if { [[ "${__omarchy_read_1}" == "openclaw" ]]; } >/dev/null 2>&1; then echo setup.default.agent.openclaw:c:1; else echo setup.default.agent.openclaw:c:0; fi
if { [[ "${__omarchy_read_1}" == "opencode" ]]; } >/dev/null 2>&1; then echo setup.default.agent.opencode:c:1; else echo setup.default.agent.opencode:c:0; fi
if { [[ "${__omarchy_read_1}" == "pi" ]]; } >/dev/null 2>&1; then echo setup.default.agent.pi:c:1; else echo setup.default.agent.pi:c:0; fi
if { omarchy-cmd-present chromium; } >/dev/null 2>&1; then echo setup.default.browser.chromium:w:1; else echo setup.default.browser.chromium:w:0; fi
if { [[ "${__omarchy_read_2}" == "chromium" ]]; } >/dev/null 2>&1; then echo setup.default.browser.chromium:c:1; else echo setup.default.browser.chromium:c:0; fi
if { omarchy-cmd-present google-chrome-stable; } >/dev/null 2>&1; then echo setup.default.browser.chrome:w:1; else echo setup.default.browser.chrome:w:0; fi
if { [[ "${__omarchy_read_2}" == "chrome" ]]; } >/dev/null 2>&1; then echo setup.default.browser.chrome:c:1; else echo setup.default.browser.chrome:c:0; fi
if { omarchy-cmd-present brave; } >/dev/null 2>&1; then echo setup.default.browser.brave:w:1; else echo setup.default.browser.brave:w:0; fi
if { [[ "${__omarchy_read_2}" == "brave" ]]; } >/dev/null 2>&1; then echo setup.default.browser.brave:c:1; else echo setup.default.browser.brave:c:0; fi
if { omarchy-cmd-present brave-origin; } >/dev/null 2>&1; then echo setup.default.browser.brave-origin:w:1; else echo setup.default.browser.brave-origin:w:0; fi
if { [[ "${__omarchy_read_2}" == "brave-origin" ]]; } >/dev/null 2>&1; then echo setup.default.browser.brave-origin:c:1; else echo setup.default.browser.brave-origin:c:0; fi
if { omarchy-cmd-present microsoft-edge-stable; } >/dev/null 2>&1; then echo setup.default.browser.edge:w:1; else echo setup.default.browser.edge:w:0; fi
if { [[ "${__omarchy_read_2}" == "edge" ]]; } >/dev/null 2>&1; then echo setup.default.browser.edge:c:1; else echo setup.default.browser.edge:c:0; fi
if { omarchy-cmd-present firefox; } >/dev/null 2>&1; then echo setup.default.browser.firefox:w:1; else echo setup.default.browser.firefox:w:0; fi
if { [[ "${__omarchy_read_2}" == "firefox" ]]; } >/dev/null 2>&1; then echo setup.default.browser.firefox:c:1; else echo setup.default.browser.firefox:c:0; fi
if { omarchy-cmd-present zen-browser; } >/dev/null 2>&1; then echo setup.default.browser.zen:w:1; else echo setup.default.browser.zen:w:0; fi
if { [[ "${__omarchy_read_2}" == "zen" ]]; } >/dev/null 2>&1; then echo setup.default.browser.zen:c:1; else echo setup.default.browser.zen:c:0; fi
if { omarchy-cmd-present alacritty; } >/dev/null 2>&1; then echo setup.default.terminal.alacritty:w:1; else echo setup.default.terminal.alacritty:w:0; fi
if { [[ "${__omarchy_read_4}" == "alacritty" ]]; } >/dev/null 2>&1; then echo setup.default.terminal.alacritty:c:1; else echo setup.default.terminal.alacritty:c:0; fi
if { omarchy-cmd-present foot; } >/dev/null 2>&1; then echo setup.default.terminal.foot:w:1; else echo setup.default.terminal.foot:w:0; fi
if { [[ "${__omarchy_read_4}" == "foot" ]]; } >/dev/null 2>&1; then echo setup.default.terminal.foot:c:1; else echo setup.default.terminal.foot:c:0; fi
if { omarchy-cmd-present ghostty; } >/dev/null 2>&1; then echo setup.default.terminal.ghostty:w:1; else echo setup.default.terminal.ghostty:w:0; fi
if { [[ "${__omarchy_read_4}" == "ghostty" ]]; } >/dev/null 2>&1; then echo setup.default.terminal.ghostty:c:1; else echo setup.default.terminal.ghostty:c:0; fi
if { omarchy-cmd-present kitty; } >/dev/null 2>&1; then echo setup.default.terminal.kitty:w:1; else echo setup.default.terminal.kitty:w:0; fi
if { [[ "${__omarchy_read_4}" == "kitty" ]]; } >/dev/null 2>&1; then echo setup.default.terminal.kitty:c:1; else echo setup.default.terminal.kitty:c:0; fi
if { omarchy-cmd-present nvim; } >/dev/null 2>&1; then echo setup.default.editor.neovim:w:1; else echo setup.default.editor.neovim:w:0; fi
if { [[ "${__omarchy_read_3}" == "nvim" ]]; } >/dev/null 2>&1; then echo setup.default.editor.neovim:c:1; else echo setup.default.editor.neovim:c:0; fi
if { omarchy-cmd-present code; } >/dev/null 2>&1; then echo setup.default.editor.vscode:w:1; else echo setup.default.editor.vscode:w:0; fi
if { [[ "${__omarchy_read_3}" == "code" ]]; } >/dev/null 2>&1; then echo setup.default.editor.vscode:c:1; else echo setup.default.editor.vscode:c:0; fi
if { omarchy-cmd-present cursor; } >/dev/null 2>&1; then echo setup.default.editor.cursor:w:1; else echo setup.default.editor.cursor:w:0; fi
if { [[ "${__omarchy_read_3}" == "cursor" ]]; } >/dev/null 2>&1; then echo setup.default.editor.cursor:c:1; else echo setup.default.editor.cursor:c:0; fi
if { omarchy-cmd-present zeditor; } >/dev/null 2>&1; then echo setup.default.editor.zed:w:1; else echo setup.default.editor.zed:w:0; fi
if { [[ "${__omarchy_read_3}" == "zeditor" ]]; } >/dev/null 2>&1; then echo setup.default.editor.zed:c:1; else echo setup.default.editor.zed:c:0; fi
if { omarchy-cmd-present sublime_text; } >/dev/null 2>&1; then echo setup.default.editor.sublime:w:1; else echo setup.default.editor.sublime:w:0; fi
if { [[ "${__omarchy_read_3}" == "sublime_text" ]]; } >/dev/null 2>&1; then echo setup.default.editor.sublime:c:1; else echo setup.default.editor.sublime:c:0; fi
if { omarchy-cmd-present helix; } >/dev/null 2>&1; then echo setup.default.editor.helix:w:1; else echo setup.default.editor.helix:w:0; fi
if { [[ "${__omarchy_read_3}" == "helix" ]]; } >/dev/null 2>&1; then echo setup.default.editor.helix:c:1; else echo setup.default.editor.helix:c:0; fi
if { omarchy-cmd-present vim; } >/dev/null 2>&1; then echo setup.default.editor.vim:w:1; else echo setup.default.editor.vim:w:0; fi
if { [[ "${__omarchy_read_3}" == "vim" ]]; } >/dev/null 2>&1; then echo setup.default.editor.vim:c:1; else echo setup.default.editor.vim:c:0; fi
if { omarchy-cmd-present emacs; } >/dev/null 2>&1; then echo setup.default.editor.emacs:w:1; else echo setup.default.editor.emacs:w:0; fi
if { [[ "${__omarchy_read_3}" == "emacs" ]]; } >/dev/null 2>&1; then echo setup.default.editor.emacs:c:1; else echo setup.default.editor.emacs:c:0; fi
if { compgen -G "$HOME/.config/omarchy/plugins/*/manifest.json"; } >/dev/null 2>&1; then echo setup.plugin.remove:w:1; else echo setup.plugin.remove:w:0; fi
if { omarchy-hw-fingerprint; } >/dev/null 2>&1; then echo setup.security.fingerprint:w:1; else echo setup.security.fingerprint:w:0; fi
if { omarchy-sudo-docker --configured; } >/dev/null 2>&1; then echo setup.security.sudoless-docker:w:1; else echo setup.security.sudoless-docker:w:0; fi
if { [[ $(findmnt -no FSTYPE /) == btrfs ]]; } >/dev/null 2>&1; then echo setup.reset:w:1; else echo setup.reset:w:0; fi
if { [[ ! -f $HOME/.local/share/applications/windows-vm.desktop ]]; } >/dev/null 2>&1; then echo install.windows:w:1; else echo install.windows:w:0; fi
if { [[ -f $HOME/.local/state/omarchy/preinstalls-removed ]]; } >/dev/null 2>&1; then echo install.preinstalls:w:1; else echo install.preinstalls:w:0; fi
if { ! omarchy-pkg-present google-chrome; } >/dev/null 2>&1; then echo install.browser.chrome:w:1; else echo install.browser.chrome:w:0; fi
if { ! omarchy-pkg-present microsoft-edge-stable-bin; } >/dev/null 2>&1; then echo install.browser.edge:w:1; else echo install.browser.edge:w:0; fi
if { ! omarchy-pkg-present brave-bin; } >/dev/null 2>&1; then echo install.browser.brave:w:1; else echo install.browser.brave:w:0; fi
if { ! omarchy-pkg-present brave-origin-bin; } >/dev/null 2>&1; then echo install.browser.brave-origin:w:1; else echo install.browser.brave-origin:w:0; fi
if { ! omarchy-pkg-present firefox; } >/dev/null 2>&1; then echo install.browser.firefox:w:1; else echo install.browser.firefox:w:0; fi
if { ! omarchy-pkg-present zen-browser-bin; } >/dev/null 2>&1; then echo install.browser.zen:w:1; else echo install.browser.zen:w:0; fi
if { ! omarchy-pkg-present 1password; } >/dev/null 2>&1; then echo install.service.1password:w:1; else echo install.service.1password:w:0; fi
if { ! omarchy-pkg-present dropbox; } >/dev/null 2>&1; then echo install.service.dropbox:w:1; else echo install.service.dropbox:w:0; fi
if { ! omarchy-pkg-present spotify; } >/dev/null 2>&1; then echo install.service.spotify:w:1; else echo install.service.spotify:w:0; fi
if { ! omarchy-pkg-present signal-desktop; } >/dev/null 2>&1; then echo install.service.signal:w:1; else echo install.service.signal:w:0; fi
if { ! omarchy-pkg-present tailscale; } >/dev/null 2>&1; then echo install.service.tailscale:w:1; else echo install.service.tailscale:w:0; fi
if { ! omarchy-pkg-present nordvpn-bin; } >/dev/null 2>&1; then echo install.service.nordvpn:w:1; else echo install.service.nordvpn:w:0; fi
if { ! omarchy-pkg-present once-bin; } >/dev/null 2>&1; then echo install.service.once:w:1; else echo install.service.once:w:0; fi
if { ! omarchy-pkg-present bitwarden; } >/dev/null 2>&1; then echo install.service.bitwarden:w:1; else echo install.service.bitwarden:w:0; fi
if { [[ -f ~/.config/chromium-flags.conf ]] && ! grep -q oauth2-client-id ~/.config/chromium-flags.conf; } >/dev/null 2>&1; then echo install.service.chromium-account:w:1; else echo install.service.chromium-account:w:0; fi
if { ! omarchy-pkg-present visual-studio-code-bin; } >/dev/null 2>&1; then echo install.editor.vscode:w:1; else echo install.editor.vscode:w:0; fi
if { ! omarchy-pkg-present cursor-bin; } >/dev/null 2>&1; then echo install.editor.cursor:w:1; else echo install.editor.cursor:w:0; fi
if { ! omarchy-pkg-present zed; } >/dev/null 2>&1; then echo install.editor.zed:w:1; else echo install.editor.zed:w:0; fi
if { ! omarchy-pkg-present sublime-text-4; } >/dev/null 2>&1; then echo install.editor.sublime:w:1; else echo install.editor.sublime:w:0; fi
if { ! omarchy-pkg-present helix; } >/dev/null 2>&1; then echo install.editor.helix:w:1; else echo install.editor.helix:w:0; fi
if { ! omarchy-pkg-present vim; } >/dev/null 2>&1; then echo install.editor.vim:w:1; else echo install.editor.vim:w:0; fi
if { ! omarchy-pkg-present omarchy-emacs; } >/dev/null 2>&1; then echo install.editor.emacs:w:1; else echo install.editor.emacs:w:0; fi
if { ! omarchy-pkg-present alacritty; } >/dev/null 2>&1; then echo install.terminal.alacritty:w:1; else echo install.terminal.alacritty:w:0; fi
if { ! omarchy-pkg-present foot; } >/dev/null 2>&1; then echo install.terminal.foot:w:1; else echo install.terminal.foot:w:0; fi
if { ! omarchy-pkg-present ghostty; } >/dev/null 2>&1; then echo install.terminal.ghostty:w:1; else echo install.terminal.ghostty:w:0; fi
if { ! omarchy-pkg-present kitty; } >/dev/null 2>&1; then echo install.terminal.kitty:w:1; else echo install.terminal.kitty:w:0; fi
if { ! omarchy-pkg-present openai-codex-desktop; } >/dev/null 2>&1; then echo install.ai.chatgpt:w:1; else echo install.ai.chatgpt:w:0; fi
if { ! omarchy-pkg-present voxtype-bin; } >/dev/null 2>&1; then echo install.ai.dictation:w:1; else echo install.ai.dictation:w:0; fi
if { ! omarchy-pkg-present grok-bot; } >/dev/null 2>&1; then echo install.ai.grok-bot:w:1; else echo install.ai.grok-bot:w:0; fi
if { ! omarchy-pkg-present lmstudio-bin; } >/dev/null 2>&1; then echo install.ai.lm-studio:w:1; else echo install.ai.lm-studio:w:0; fi
if { ! omarchy-cmd-present ollama; } >/dev/null 2>&1; then echo install.ai.ollama:w:1; else echo install.ai.ollama:w:0; fi
if { ! omarchy-pkg-present t3code-bin; } >/dev/null 2>&1; then echo install.ai.t3-code:w:1; else echo install.ai.t3-code:w:0; fi
if { ! omarchy-pkg-present hermes-desktop; } >/dev/null 2>&1; then echo install.ai.hermes:w:1; else echo install.ai.hermes:w:0; fi
if { ! omarchy-pkg-present openclaw; } >/dev/null 2>&1; then echo install.ai.openclaw:w:1; else echo install.ai.openclaw:w:0; fi
if { ! omarchy-pkg-present perplexity; } >/dev/null 2>&1; then echo install.ai.perplexity:w:1; else echo install.ai.perplexity:w:0; fi
if { ! omarchy-pkg-present steam; } >/dev/null 2>&1; then echo install.gaming.steam:w:1; else echo install.gaming.steam:w:0; fi
if { ! omarchy-pkg-present retroarch; } >/dev/null 2>&1; then echo install.gaming.retroarch:w:1; else echo install.gaming.retroarch:w:0; fi
if { ! omarchy-pkg-present minecraft-launcher; } >/dev/null 2>&1; then echo install.gaming.minecraft:w:1; else echo install.gaming.minecraft:w:0; fi
if { ! flatpak info com.nvidia.geforcenow; } >/dev/null 2>&1; then echo install.gaming.geforce-now:w:1; else echo install.gaming.geforce-now:w:0; fi
if { [[ ! -f "$HOME/.local/share/applications/Xbox Cloud Gaming.desktop" ]]; } >/dev/null 2>&1; then echo install.gaming.xbox-cloud:w:1; else echo install.gaming.xbox-cloud:w:0; fi
if { ! omarchy-pkg-present xpadneo-dkms; } >/dev/null 2>&1; then echo install.gaming.xbox-controllers:w:1; else echo install.gaming.xbox-controllers:w:0; fi
if { [[ ! -d $HOME/Games/battlenet ]]; } >/dev/null 2>&1; then echo install.gaming.battlenet:w:1; else echo install.gaming.battlenet:w:0; fi
if { ! omarchy-pkg-present lutris; } >/dev/null 2>&1; then echo install.gaming.lutris:w:1; else echo install.gaming.lutris:w:0; fi
if { ! omarchy-pkg-present heroic-games-launcher-bin; } >/dev/null 2>&1; then echo install.gaming.heroic:w:1; else echo install.gaming.heroic:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/ruby ]]; } >/dev/null 2>&1; then echo install.development.rails:w:1; else echo install.development.rails:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/go ]]; } >/dev/null 2>&1; then echo install.development.go:w:1; else echo install.development.go:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/python ]]; } >/dev/null 2>&1; then echo install.development.python:w:1; else echo install.development.python:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/zig ]]; } >/dev/null 2>&1; then echo install.development.zig:w:1; else echo install.development.zig:w:0; fi
if { [[ ! -d $HOME/.rustup ]]; } >/dev/null 2>&1; then echo install.development.rust:w:1; else echo install.development.rust:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/java ]]; } >/dev/null 2>&1; then echo install.development.java:w:1; else echo install.development.java:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/dotnet ]]; } >/dev/null 2>&1; then echo install.development.dotnet:w:1; else echo install.development.dotnet:w:0; fi
if { [[ ! -d $HOME/.opam ]]; } >/dev/null 2>&1; then echo install.development.ocaml:w:1; else echo install.development.ocaml:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/clojure ]]; } >/dev/null 2>&1; then echo install.development.clojure:w:1; else echo install.development.clojure:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/scala ]]; } >/dev/null 2>&1; then echo install.development.scala:w:1; else echo install.development.scala:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/node ]]; } >/dev/null 2>&1; then echo install.development.javascript.node:w:1; else echo install.development.javascript.node:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/bun ]]; } >/dev/null 2>&1; then echo install.development.javascript.bun:w:1; else echo install.development.javascript.bun:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/deno ]]; } >/dev/null 2>&1; then echo install.development.javascript.deno:w:1; else echo install.development.javascript.deno:w:0; fi
if { ! omarchy-pkg-present php; } >/dev/null 2>&1; then echo install.development.php.php:w:1; else echo install.development.php.php:w:0; fi
if { [[ ! -x $HOME/.config/composer/vendor/bin/laravel ]]; } >/dev/null 2>&1; then echo install.development.php.laravel:w:1; else echo install.development.php.laravel:w:0; fi
if { ! omarchy-pkg-present symfony-cli; } >/dev/null 2>&1; then echo install.development.php.symfony:w:1; else echo install.development.php.symfony:w:0; fi
if { [[ ! -d $HOME/.local/share/mise/installs/elixir ]]; } >/dev/null 2>&1; then echo install.development.elixir.elixir:w:1; else echo install.development.elixir.elixir:w:0; fi
if { ! compgen -G "$HOME/.mix/archives/phx_new*"; } >/dev/null 2>&1; then echo install.development.elixir.phoenix:w:1; else echo install.development.elixir.phoenix:w:0; fi
if { grep -qE '^Exec=.*(omarchy-launch-webapp|omarchy-webapp-handler)' $HOME/.local/share/applications/*.desktop; } >/dev/null 2>&1; then echo remove.webapp:w:1; else echo remove.webapp:w:0; fi
if { grep -qE '^Exec=.*(\$TERMINAL|xdg-terminal-exec).*-e' $HOME/.local/share/applications/*.desktop; } >/dev/null 2>&1; then echo remove.tui:w:1; else echo remove.tui:w:0; fi
if { omarchy-pkg-present voxtype-bin; } >/dev/null 2>&1; then echo remove.dictation:w:1; else echo remove.dictation:w:0; fi
if { [[ -f $HOME/.local/share/applications/windows-vm.desktop ]]; } >/dev/null 2>&1; then echo remove.windows:w:1; else echo remove.windows:w:0; fi
if { [[ ! -f $HOME/.local/state/omarchy/preinstalls-removed ]]; } >/dev/null 2>&1; then echo remove.preinstalls:w:1; else echo remove.preinstalls:w:0; fi
if { omarchy-pkg-present fprintd; } >/dev/null 2>&1; then echo remove.security.fingerprint:w:1; else echo remove.security.fingerprint:w:0; fi
if { omarchy-pkg-present pam-u2f; } >/dev/null 2>&1; then echo remove.security.fido2:w:1; else echo remove.security.fido2:w:0; fi
if { systemctl is-enabled --quiet sshd; } >/dev/null 2>&1; then echo remove.security.sshd:w:1; else echo remove.security.sshd:w:0; fi
if { ! omarchy-sudo-docker --configured; } >/dev/null 2>&1; then echo remove.security.sudoless-docker:w:1; else echo remove.security.sudoless-docker:w:0; fi
if { omarchy-pkg-present hermes-desktop; } >/dev/null 2>&1; then echo remove.ai.hermes:w:1; else echo remove.ai.hermes:w:0; fi
if { omarchy-pkg-present google-chrome; } >/dev/null 2>&1; then echo remove.browser.chrome:w:1; else echo remove.browser.chrome:w:0; fi
if { omarchy-pkg-present microsoft-edge-stable-bin; } >/dev/null 2>&1; then echo remove.browser.edge:w:1; else echo remove.browser.edge:w:0; fi
if { omarchy-pkg-present brave-bin; } >/dev/null 2>&1; then echo remove.browser.brave:w:1; else echo remove.browser.brave:w:0; fi
if { omarchy-pkg-present brave-origin-bin; } >/dev/null 2>&1; then echo remove.browser.brave-origin:w:1; else echo remove.browser.brave-origin:w:0; fi
if { omarchy-pkg-present firefox; } >/dev/null 2>&1; then echo remove.browser.firefox:w:1; else echo remove.browser.firefox:w:0; fi
if { omarchy-pkg-present zen-browser-bin; } >/dev/null 2>&1; then echo remove.browser.zen:w:1; else echo remove.browser.zen:w:0; fi
if { omarchy-pkg-present dropbox; } >/dev/null 2>&1; then echo remove.service.dropbox:w:1; else echo remove.service.dropbox:w:0; fi
if { omarchy-pkg-present tailscale; } >/dev/null 2>&1; then echo remove.service.tailscale:w:1; else echo remove.service.tailscale:w:0; fi
if { omarchy-pkg-present steam; } >/dev/null 2>&1; then echo remove.gaming.steam:w:1; else echo remove.gaming.steam:w:0; fi
if { omarchy-pkg-present retroarch; } >/dev/null 2>&1; then echo remove.gaming.retroarch:w:1; else echo remove.gaming.retroarch:w:0; fi
if { omarchy-pkg-present minecraft-launcher; } >/dev/null 2>&1; then echo remove.gaming.minecraft:w:1; else echo remove.gaming.minecraft:w:0; fi
if { flatpak info com.nvidia.geforcenow; } >/dev/null 2>&1; then echo remove.gaming.geforce-now:w:1; else echo remove.gaming.geforce-now:w:0; fi
if { [[ -f "$HOME/.local/share/applications/Xbox Cloud Gaming.desktop" ]]; } >/dev/null 2>&1; then echo remove.gaming.xbox-cloud:w:1; else echo remove.gaming.xbox-cloud:w:0; fi
if { omarchy-pkg-present xpadneo-dkms; } >/dev/null 2>&1; then echo remove.gaming.xbox-controllers:w:1; else echo remove.gaming.xbox-controllers:w:0; fi
if { [[ -d $HOME/Games/battlenet ]]; } >/dev/null 2>&1; then echo remove.gaming.battlenet:w:1; else echo remove.gaming.battlenet:w:0; fi
if { omarchy-pkg-present lutris; } >/dev/null 2>&1; then echo remove.gaming.lutris:w:1; else echo remove.gaming.lutris:w:0; fi
if { omarchy-pkg-present heroic-games-launcher-bin; } >/dev/null 2>&1; then echo remove.gaming.heroic:w:1; else echo remove.gaming.heroic:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/ruby ]]; } >/dev/null 2>&1; then echo remove.development.rails:w:1; else echo remove.development.rails:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/go ]]; } >/dev/null 2>&1; then echo remove.development.go:w:1; else echo remove.development.go:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/python ]]; } >/dev/null 2>&1; then echo remove.development.python:w:1; else echo remove.development.python:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/zig ]]; } >/dev/null 2>&1; then echo remove.development.zig:w:1; else echo remove.development.zig:w:0; fi
if { [[ -d $HOME/.rustup ]]; } >/dev/null 2>&1; then echo remove.development.rust:w:1; else echo remove.development.rust:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/java ]]; } >/dev/null 2>&1; then echo remove.development.java:w:1; else echo remove.development.java:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/dotnet ]]; } >/dev/null 2>&1; then echo remove.development.dotnet:w:1; else echo remove.development.dotnet:w:0; fi
if { [[ -d $HOME/.opam ]]; } >/dev/null 2>&1; then echo remove.development.ocaml:w:1; else echo remove.development.ocaml:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/clojure ]]; } >/dev/null 2>&1; then echo remove.development.clojure:w:1; else echo remove.development.clojure:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/scala ]]; } >/dev/null 2>&1; then echo remove.development.scala:w:1; else echo remove.development.scala:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/node ]]; } >/dev/null 2>&1; then echo remove.development.javascript.node:w:1; else echo remove.development.javascript.node:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/bun ]]; } >/dev/null 2>&1; then echo remove.development.javascript.bun:w:1; else echo remove.development.javascript.bun:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/deno ]]; } >/dev/null 2>&1; then echo remove.development.javascript.deno:w:1; else echo remove.development.javascript.deno:w:0; fi
if { omarchy-pkg-present php; } >/dev/null 2>&1; then echo remove.development.php.php:w:1; else echo remove.development.php.php:w:0; fi
if { [[ -x $HOME/.config/composer/vendor/bin/laravel ]]; } >/dev/null 2>&1; then echo remove.development.php.laravel:w:1; else echo remove.development.php.laravel:w:0; fi
if { omarchy-pkg-present symfony-cli; } >/dev/null 2>&1; then echo remove.development.php.symfony:w:1; else echo remove.development.php.symfony:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/elixir ]]; } >/dev/null 2>&1; then echo remove.development.elixir.elixir:w:1; else echo remove.development.elixir.elixir:w:0; fi
if { [[ -d $HOME/.local/share/mise/installs/elixir ]]; } >/dev/null 2>&1; then echo remove.development.elixir.phoenix:w:1; else echo remove.development.elixir.phoenix:w:0; fi
if { omarchy-pkg-present t3code-bin; } >/dev/null 2>&1; then echo remove.ai.t3-code:w:1; else echo remove.ai.t3-code:w:0; fi
if { omarchy-pkg-present openclaw; } >/dev/null 2>&1; then echo remove.ai.openclaw:w:1; else echo remove.ai.openclaw:w:0; fi
if { omarchy-pkg-present perplexity; } >/dev/null 2>&1; then echo remove.ai.perplexity:w:1; else echo remove.ai.perplexity:w:0; fi
if { omarchy-theme-extras; } >/dev/null 2>&1; then echo update.themes:w:1; else echo update.themes:w:0; fi
if { [[ "${__omarchy_read_0}" == "stable" ]]; } >/dev/null 2>&1; then echo update.channel.stable:c:1; else echo update.channel.stable:c:0; fi
if { [[ "${__omarchy_read_0}" == "rc" ]]; } >/dev/null 2>&1; then echo update.channel.rc:c:1; else echo update.channel.rc:c:0; fi
if { [[ "${__omarchy_read_0}" == "edge" ]]; } >/dev/null 2>&1; then echo update.channel.edge:c:1; else echo update.channel.edge:c:0; fi
if { [[ "${__omarchy_read_0}" == "dev" ]]; } >/dev/null 2>&1; then echo update.channel.dev:c:1; else echo update.channel.dev:c:0; fi
if { false; } >/dev/null 2>&1; then echo personal.hidden:w:1; else echo personal.hidden:w:0; fi
