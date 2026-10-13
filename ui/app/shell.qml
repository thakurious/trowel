pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import "../qml/Trowel"

// trowel-app: search, Quick Look, Settings, Onboarding. Separate process
// so a bug here cannot take down omarchy-shell (UI-DESIGN §1 / F18).
ShellRoot {
  id: rootShell
  property string surface: {
    var s = Quickshell.env("TROWEL_SURFACE")
    return (s && s.length) ? s : "search"
  }

  Client {
    id: client
    clientName: "app"
  }

  FloatingWindow {
    id: main
    visible: surface === "search" || surface === "onboarding" || surface === "settings"
    title: "Trowel"
    implicitWidth: surface === "settings" ? 640 : (surface === "onboarding" ? 560 : 1040)
    implicitHeight: surface === "settings" ? 520 : (surface === "onboarding" ? 640 : 700)
    color: Theme.background

    Loader {
      anchors.fill: parent
      sourceComponent: {
        if (surface === "onboarding" || (client.config && client.config.onboarded === false && surface === "search"))
          return onboardingComp
        if (surface === "settings")
          return settingsComp
        return searchComp
      }
    }
  }

  FloatingWindow {
    id: look
    visible: rootShell.surface === "quicklook"
    title: "Trowel Quick Look"
    implicitWidth: 1200
    implicitHeight: 800
    color: Theme.background
    QuickLook {
      anchors.fill: parent
      client: client
      onClosed: look.visible = false
    }
  }

  Component {
    id: searchComp
    SearchWindow {
      client: client
      onQuickLookRequested: look.visible = true
    }
  }
  Component {
    id: settingsComp
    SettingsWindow {
      client: client
    }
  }
  Component {
    id: onboardingComp
    Onboarding {
      client: client
      onFinished: rootShell.surface = "search"
    }
  }
}
