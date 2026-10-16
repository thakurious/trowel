pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import "Trowel"

// trowel-app: search, Quick Look, Settings, Onboarding. Separate process
// so a bug here cannot take down omarchy-shell (UI-DESIGN §1 / F18).
ShellRoot {
  id: rootShell
  property string surface: {
    var s = Quickshell.env("TROWEL_SURFACE")
    return (s && s.length) ? s : "search"
  }
  // Folders Continue sets onboarded so indexing can start; keep the wizard
  // until Done instead of swapping this window to search.
  property bool wizardOpen: false

  Client {
    id: client
    clientName: "app"
    onConfigArrived: function (cfg) {
      if (cfg && cfg.onboarded === false)
        rootShell.wizardOpen = true
    }
  }

  FloatingWindow {
    id: main
    visible: surface === "search" || surface === "onboarding" || surface === "settings"
    title: "Trowel"
    implicitWidth: {
      if (surface === "settings")
        return 640
      if (surface === "onboarding" || rootShell.wizardOpen || (client.config && client.config.onboarded === false))
        return 680
      return 1040
    }
    implicitHeight: {
      if (surface === "settings")
        return 520
      if (surface === "onboarding" || rootShell.wizardOpen || (client.config && client.config.onboarded === false))
        return 600
      return 700
    }
    color: Theme.background

    Loader {
      anchors.fill: parent
      focus: true
      sourceComponent: {
        if (surface === "onboarding" || rootShell.wizardOpen || (client.config && client.config.onboarded === false))
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
      onFinished: {
        rootShell.wizardOpen = false
        rootShell.surface = "search"
      }
    }
  }
}
