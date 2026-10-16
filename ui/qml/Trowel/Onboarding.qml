import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io

// First-run wizard. Port of upstream/Sources/DigUpApp/Onboarding.swift
// (680×600, five steps). Product name Trowel (DECISIONS D17).
Item {
  id: root
  required property var client
  property int step: {
    var s = Quickshell.env("TROWEL_ONBOARDING_STEP")
    var n = s ? parseInt(s) : 0
    return (n >= 0 && n <= 4) ? n : 0
  }
  property var folderRows: []
  property var skippedFolders: []
  property var estimates: ({})
  property bool skipOpen: false
  property bool searchCode: false
  property var codeRows: []
  property string skippedTypesText: ""
  property string folderNote: ""
  property bool launchAtLogin: true
  property bool indexOnBattery: false
  property var modelInfo: ({})
  property bool dirsReady: false

  readonly property var steps: ["welcome", "folders", "code", "shortcut", "ready"]
  readonly property int stepCount: steps.length
  readonly property bool foldersEmpty: selectedRoots().length === 0
  readonly property string continueTitle: {
    if (step === 0)
      return "Get Started"
    if (step === stepCount - 1)
      return "Done"
    return "Continue"
  }

  signal finished
  // DigUp OnboardingView.size is 680×600. Hosts `anchors.fill` this item;
  // do not also bind width/height (that fights the host and crushes the layout).
  implicitWidth: 680
  implicitHeight: 600
  focus: true
  clip: true

  Keys.onPressed: function (event) {
    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
      root.next()
      event.accepted = true
    } else if (event.key === Qt.Key_Escape) {
      return
    } else if ((event.key === Qt.Key_Backspace || event.key === Qt.Key_Left) && root.step > 0 && root.step < root.stepCount - 1) {
      root.back()
      event.accepted = true
    }
  }

  Component.onCompleted: {
    loadCandidates()
    root.forceActiveFocus()
    if (client && client.modelStatus)
      client.modelStatus()
    if (root.step === 1)
      Qt.callLater(requestEstimates)
  }

  Connections {
    target: root.client
    function onLineEvent(msg) {
      if (!msg)
        return
      if (msg.event === "estimate" && msg.per_path)
        root.applyEstimates(msg.per_path)
      else if (msg.per_path && msg.ok === true)
        root.applyEstimates(msg.per_path)
      if (msg.present !== undefined && msg.bytes !== undefined)
        root.modelInfo = msg
    }
    function onConfigArrived(cfg) {
      if (cfg && cfg.index_on_battery !== undefined)
        root.indexOnBattery = !!cfg.index_on_battery
    }
  }

  function next() {
    if (step === 1 && foldersEmpty)
      return
    if (step === 0 && client && client.modelDownload)
      client.modelDownload("start")
    if (step === 1)
      applyFolders()
    if (step === 2)
      applyCode()
    if (step === steps.length - 1) {
      finish()
      return
    }
    step += 1
    if (step === 1)
      Qt.callLater(requestEstimates)
  }

  function back() {
    if (step > 0 && step < stepCount - 1)
      step -= 1
  }

  function finish() {
    if (client && client.configSet) {
      client.configSet({
          "onboarded": true,
          "index_on_battery": root.indexOnBattery
        })
    }
    autostart.command = ["trowel", "autostart", root.launchAtLogin ? "on" : "off"]
    autostart.running = true
    root.finished()
  }

  function applyFolders() {
    if (!client || !client.configSet)
      return
    var types = skippedTypesText.split(/[,\s]+/).filter(function (s) {
      return s.length > 0
    })
    // Indexing starts now (file names first; meaning once the model is in),
    // while the rest of the wizard is still open. `upstream/Sources/DigUpApp/Onboarding.swift:142`
    // Linux: helper/sync are gated on `onboarded` (engine.rs), so flip it here.
    client.configSet({
        "roots": selectedRoots(),
        "excluded_folders": skippedFolders.slice(),
        "excluded_types": types,
        "onboarded": true
      })
  }

  function applyCode() {
    if (!client || !client.configSet)
      return
    client.configSet({
        "code_roots": root.searchCode ? selectedCodeRoots() : []
      })
  }

  function expand(p) {
    var s = String(p || "")
    if (s === "~")
      return Theme.home
    if (s.indexOf("~/") === 0)
      return Theme.home + s.slice(1)
    return s
  }

  function loadCandidates() {
    var override = Quickshell.env("TROWEL_ONBOARDING_LOCATIONS")
    if (override && override.length) {
      setFolderRows(override.split(":").filter(function (s) {
        return s.length > 0
      }))
      dirsReady = true
      return
    }
    dirsProc.running = true
  }

  function setFolderRows(paths) {
    var rows = []
    var seen = {}
    for (var i = 0; i < paths.length; i++) {
      var p = expand(paths[i])
      if (!p.length || seen[p])
        continue
      seen[p] = true
      rows.push({
                 "path": p,
                 "checked": true
               })
    }
    folderRows = rows
    if (root.step === 1)
      Qt.callLater(requestEstimates)
  }

  function selectedRoots() {
    var out = []
    for (var i = 0; i < folderRows.length; i++) {
      if (folderRows[i].checked)
        out.push(folderRows[i].path)
    }
    return out
  }

  function selectedCodeRoots() {
    var out = []
    for (var i = 0; i < codeRows.length; i++) {
      if (codeRows[i].checked)
        out.push(codeRows[i].path)
    }
    return out
  }

  function folderIndex(path) {
    for (var i = 0; i < folderRows.length; i++) {
      if (folderRows[i].path === path)
        return i
    }
    return -1
  }

  function toggleFolder(path) {
    var next = []
    for (var i = 0; i < folderRows.length; i++) {
      var row = folderRows[i]
      next.push({
                  "path": row.path,
                  "checked": row.path === path ? !row.checked : row.checked
                })
    }
    folderRows = next
  }

  function addFolder(path) {
    path = expand(path)
    if (!path.length)
      return
    folderNote = ""
    var idx = folderIndex(path)
    if (idx >= 0) {
      if (!folderRows[idx].checked)
        toggleFolder(path)
      else
        folderNote = "“" + basename(path) + "” is already in the list."
      return
    }
    for (var s = 0; s < skippedFolders.length; s++) {
      if (skippedFolders[s] === path) {
        unskipFolder(path)
        break
      }
    }
    var next = folderRows.slice()
    next.push({
                "path": path,
                "checked": true
              })
    folderRows = next
    requestEstimates()
  }

  function skipFolder(path) {
    path = expand(path)
    if (!path.length)
      return
    folderNote = ""
    var idx = folderIndex(path)
    if (idx >= 0) {
      if (folderRows[idx].checked)
        toggleFolder(path)
      return
    }
    for (var i = 0; i < skippedFolders.length; i++) {
      if (skippedFolders[i] === path)
        return
    }
    var skips = skippedFolders.slice()
    skips.push(path)
    skippedFolders = skips
    skipOpen = true
  }

  function unskipFolder(path) {
    var next = []
    for (var i = 0; i < skippedFolders.length; i++) {
      if (skippedFolders[i] !== path)
        next.push(skippedFolders[i])
    }
    skippedFolders = next
  }

  function toggleCodeFolder(path) {
    var next = []
    for (var i = 0; i < codeRows.length; i++) {
      var row = codeRows[i]
      next.push({
                  "path": row.path,
                  "checked": row.path === path ? !row.checked : row.checked
                })
    }
    codeRows = next
  }

  function addCodeFolder(path) {
    path = expand(path)
    if (!path.length)
      return
    for (var i = 0; i < codeRows.length; i++) {
      if (codeRows[i].path === path) {
        if (!codeRows[i].checked)
          toggleCodeFolder(path)
        return
      }
    }
    var next = codeRows.slice()
    next.push({
                "path": path,
                "checked": true
              })
    codeRows = next
  }

  function basename(p) {
    var s = String(p || "").replace(/\/+$/, "")
    var i = s.lastIndexOf("/")
    return i >= 0 ? s.slice(i + 1) : s
  }

  function requestEstimates() {
    if (!client || !client.estimate)
      return
    var paths = []
    for (var i = 0; i < folderRows.length; i++)
      paths.push(folderRows[i].path)
    if (paths.length)
      client.estimate(paths, false)
  }

  function applyEstimates(rows) {
    var next = Object.assign({}, estimates)
    for (var i = 0; i < rows.length; i++) {
      var row = rows[i]
      if (row && row.path)
        next[row.path] = row
    }
    estimates = next
  }

  function estimateLine(path) {
    var e = estimates[path]
    if (!e)
      return ""
    if (e.skips && e.skips.length && (!e.files || e.files === 0))
      return e.skips[0]
    var n = e.files || 0
    var secs = e.est_seconds || 0
    var kinds = kindPhrase(e.by_kind)
    var left = kinds.length ? kinds : (n === 1 ? "1 file" : n + " files")
    if (n === 0)
      return "Nothing to search here yet"
    return left + " · " + roughDuration(secs)
  }

  function kindPhrase(by) {
    if (!by)
      return ""
    var order = ["screenshot", "image", "pdf", "doc", "audio", "video"]
    var labels = {
      "screenshot": "screenshots",
      "image": "photos",
      "pdf": "PDFs",
      "doc": "documents",
      "audio": "audio",
      "video": "videos"
    }
    var parts = []
    for (var i = 0; i < order.length; i++) {
      var n = by[order[i]] || 0
      if (n > 0)
        parts.push(n + " " + labels[order[i]])
    }
    return parts.slice(0, 3).join(", ")
  }

  function roughDuration(seconds) {
    if (seconds < 1)
      return "under a second"
    if (seconds < 90)
      return "about " + Math.max(1, Math.round(seconds)) + " s"
    if (seconds < 3600)
      return "about " + Math.max(1, Math.round(seconds / 60)) + " min"
    return "about " + (seconds / 3600).toFixed(1) + " h"
  }

  function foldersSummary() {
    if (foldersEmpty)
      return "Choose at least one folder"
    var total = 0
    var pending = false
    for (var i = 0; i < folderRows.length; i++) {
      if (!folderRows[i].checked)
        continue
      var e = estimates[folderRows[i].path]
      if (!e)
        pending = true
      else
        total += e.est_seconds || 0
    }
    if (pending)
      return "Looking at your folders…"
    return "First pass: " + roughDuration(total)
  }

  function downloadLabel() {
    if (modelInfo.present)
      return "Model ready"
    if (modelInfo.downloading)
      return "Downloading the model…"
    if (modelInfo.error)
      return String(modelInfo.error)
    return "Model not downloaded"
  }

  // Opaque fill so the search field cannot show through a transparent wizard.
  Rectangle {
    anchors.fill: parent
    color: Theme.background
  }

  // Brand wash. UI-DESIGN §2 / Onboarding.swift Brand.wash.
  Rectangle {
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.top: parent.top
    height: 180
    gradient: Gradient {
      GradientStop {
        position: 0
        color: Theme.alpha(Theme.brandMoss, 0.18)
      }
      GradientStop {
        position: 1
        color: "transparent"
      }
    }
  }

  Item {
    id: body
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.top: parent.top
    anchors.bottom: footer.top
    anchors.leftMargin: 36
    anchors.rightMargin: 36
    anchors.topMargin: 28

    // Welcome — Onboarding.swift WelcomeStep
    Flickable {
      id: welcomeFlick
      visible: root.step === 0
      anchors.fill: parent
      clip: true
      boundsBehavior: Flickable.StopAtBounds
      contentWidth: width
      contentHeight: welcomeCol.implicitHeight
      ScrollBar.vertical: ScrollBar {
        policy: welcomeFlick.contentHeight > welcomeFlick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
      }

      Column {
        id: welcomeCol
        width: parent.width
        spacing: 0

          Item {
            width: parent.width
            height: 72
            Rectangle {
              width: 72
              height: 72
              radius: 18
              anchors.horizontalCenter: parent.horizontalCenter
              color: Theme.brandMoss
              Text {
                anchors.centerIn: parent
                text: "T"
                color: Theme.brandSprout
                font.pixelSize: 36
                font.weight: Font.Bold
              }
            }
          }
          Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: "Trowel"
            color: Theme.foreground
            font.pixelSize: 30
            font.weight: Font.Bold
            topPadding: 10
          }
          Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: "Describe it. Dig it up."
            color: Theme.secondary
            font.pixelSize: 16
            topPadding: 2
          }
          Column {
            width: Math.min(470, parent.width)
            anchors.horizontalCenter: parent.horizontalCenter
            topPadding: 28
            spacing: 14

            FeatureRow {
              width: parent.width
              kind: "screenshot"
              title: "Screenshots and photos"
              example: "“that payment error”, “the zebra”, “whiteboard from Tuesday”"
            }
            FeatureRow {
              width: parent.width
              kind: "pdf"
              title: "PDFs and documents"
              example: "“the clause about pets in the lease”, “flight receipt”"
            }
            FeatureRow {
              width: parent.width
              kind: "audio"
              title: "Moments inside audio and video"
              example: "“where they talk about sleep”, “the part with the drums”"
            }
          }
          Row {
            width: Math.min(470, parent.width)
            anchors.horizontalCenter: parent.horizontalCenter
            topPadding: 22
            spacing: 12
            Text {
              text: "🔒"
              font.pixelSize: 16
            }
            Text {
              width: parent.width - 28
              wrapMode: Text.Wrap
              text: "Everything is indexed and searched on this computer. Trowel downloads its AI model (865 MB) once; after that it works offline, and nothing you index ever leaves this machine."
              color: Theme.secondary
              font.pixelSize: 12
            }
          }
      }
    }

    // Folders — Onboarding.swift FoldersStep. ScrollView only around the
    // list + skip (`upstream/Sources/DigUpApp/Onboarding.swift:434`), so
    // extra folders overflow instead of stretching the dialog.
    Item {
      visible: root.step === 1
      anchors.fill: parent

      StepTitle {
        id: foldersTitle
        anchors.top: parent.top
        width: parent.width
        title: "What should Trowel search?"
        subtitle: "Pick folders, or add any other folder on this computer. You can change this anytime in Settings."
      }

      Flickable {
        id: foldersFlick
        anchors.top: foldersTitle.bottom
        anchors.topMargin: 12
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: foldersStatic.top
        anchors.bottomMargin: 10
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        contentWidth: width
        contentHeight: foldersList.implicitHeight
        ScrollBar.vertical: ScrollBar {
          policy: foldersFlick.contentHeight > foldersFlick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
        }

        Column {
          id: foldersList
          width: foldersFlick.width
          spacing: 8

          Repeater {
            model: root.folderRows
            delegate: FolderRow {
              width: parent.width
              path: modelData.path
              checked: modelData.checked
              busy: root.estimates[modelData.path] === undefined
              estimate: root.estimateLine(modelData.path)
              onToggled: root.toggleFolder(modelData.path)
            }
          }
        }
      }

      Column {
        id: foldersStatic
        anchors.bottom: parent.bottom
        width: parent.width
        spacing: 8

        Rectangle {
          width: parent.width
          height: 42
          radius: 10
          color: "transparent"
          border.color: Theme.hairline
          border.width: 1

          Text {
            anchors.centerIn: parent
            text: "+  Add Folder…"
            color: Theme.secondary
            font.pixelSize: 13
            font.weight: Font.Medium
          }
          MouseArea {
            anchors.fill: parent
            onClicked: {
              picker.mode = "add"
              picker.running = true
            }
          }
        }

        Text {
          visible: root.folderNote.length > 0
          width: parent.width
          text: root.folderNote
          color: Theme.secondary
          font.pixelSize: 11
          wrapMode: Text.Wrap
        }

        Column {
          width: parent.width
          spacing: 8

          Text {
            text: root.skipOpen ? "▾  Skip some folders or file types" : "▸  Skip some folders or file types"
            color: Theme.secondary
            font.pixelSize: 12
            font.weight: Font.Medium
            MouseArea {
              anchors.fill: parent
              anchors.margins: -6
              onClicked: root.skipOpen = !root.skipOpen
            }
          }

          Column {
            visible: root.skipOpen
            width: parent.width
            spacing: 8

            Repeater {
              model: root.skippedFolders
              delegate: Row {
                width: parent.width
                spacing: 8
                Text {
                  width: parent.width - 24
                  text: modelData
                  color: Theme.foreground
                  font.pixelSize: 12
                  elide: Text.ElideMiddle
                }
                Text {
                  text: "−"
                  color: Theme.secondary
                  font.pixelSize: 16
                  MouseArea {
                    anchors.fill: parent
                    anchors.margins: -8
                    onClicked: root.unskipFolder(modelData)
                  }
                }
              }
            }

            Text {
              text: "+  Skip a Folder…"
              color: Theme.accent
              font.pixelSize: 12
              MouseArea {
                anchors.fill: parent
                anchors.margins: -6
                onClicked: {
                  picker.mode = "skip"
                  picker.running = true
                }
              }
            }

            Text {
              text: "File types to skip"
              color: Theme.foreground
              font.pixelSize: 12
            }
            Rectangle {
              width: parent.width
              height: 32
              radius: 8
              color: Theme.keycapFill
              border.color: Theme.hairline
              border.width: Theme.hairlineWidth()
              TextInput {
                id: typesField
                anchors.fill: parent
                anchors.margins: 8
                text: root.skippedTypesText
                color: Theme.foreground
                font.pixelSize: 12
                clip: true
                onTextChanged: root.skippedTypesText = text
              }
              Text {
                visible: typesField.text.length === 0 && !typesField.activeFocus
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.leftMargin: 8
                text: "e.g. heic, txt"
                color: Theme.secondary
                font.pixelSize: 12
              }
            }
            Text {
              width: parent.width
              wrapMode: Text.Wrap
              text: "Code, keys, certificates, hidden folders and caches are always skipped."
              color: Theme.secondary
              font.pixelSize: 11
            }
          }
        }

        Row {
          width: parent.width
          spacing: 8
          Text {
            text: "ℹ"
            color: Theme.secondary
            font.pixelSize: 12
          }
          Text {
            width: parent.width - 20
            wrapMode: Text.Wrap
            text: "Trowel only reads files — it never changes, moves or uploads them. Git repos contribute screenshots here; turn on code search next to index source."
            color: Theme.secondary
            font.pixelSize: 11
          }
        }
      }
    }

    // Code — Onboarding.swift CodeStep
    Item {
      visible: root.step === 2
      anchors.fill: parent

      StepTitle {
        id: codeTitle
        anchors.top: parent.top
        width: parent.width
        title: "Search your code too?"
        subtitle: "Trowel can find code by what it does: type code: in the search panel, as in “code: retry with backoff”. It gets an index of its own and never shows up in other searches. You can turn it on later in Settings → Code."
      }

      Rectangle {
        id: codeSwitchBox
        anchors.top: codeTitle.bottom
        anchors.topMargin: 14
        width: parent.width
        radius: 10
        color: Theme.alpha(Theme.foreground, 0.04)
        height: codeSwitch.implicitHeight + 28
        SwitchRow {
          id: codeSwitch
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          anchors.margins: 14
          title: "Search my code"
          detail: "Off unless you turn it on."
          checked: root.searchCode
          onToggled: function (on) {
            root.searchCode = on
            if (on && root.codeRows.length === 0 && root.folderRows.length > 0) {
              var seed = []
              for (var i = 0; i < root.folderRows.length; i++) {
                if (root.folderRows[i].checked)
                  seed.push({
                              "path": root.folderRows[i].path,
                              "checked": true
                            })
              }
              root.codeRows = seed
            }
          }
        }
      }

      Flickable {
        id: codeFlick
        visible: root.searchCode
        anchors.top: codeSwitchBox.bottom
        anchors.topMargin: 10
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: codeStatic.top
        anchors.bottomMargin: 10
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        contentWidth: width
        contentHeight: codeList.implicitHeight
        ScrollBar.vertical: ScrollBar {
          policy: codeFlick.contentHeight > codeFlick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
        }

        Column {
          id: codeList
          width: codeFlick.width
          spacing: 8

          Repeater {
            model: root.codeRows
            delegate: FolderRow {
              width: parent.width
              path: modelData.path
              checked: modelData.checked
              estimate: "Code in this folder"
              onToggled: root.toggleCodeFolder(modelData.path)
            }
          }
        }
      }

      Column {
        id: codeStatic
        visible: root.searchCode
        anchors.bottom: parent.bottom
        width: parent.width
        spacing: 8

        Rectangle {
          width: parent.width
          height: 42
          radius: 10
          color: "transparent"
          border.color: Theme.hairline
          border.width: 1
          Text {
            anchors.centerIn: parent
            text: "+  Add Folder…"
            color: Theme.secondary
            font.pixelSize: 13
            font.weight: Font.Medium
          }
          MouseArea {
            anchors.fill: parent
            onClicked: {
              picker.mode = "code"
              picker.running = true
            }
          }
        }
        Text {
          width: parent.width
          wrapMode: Text.Wrap
          text: "Code search reads source files, notebooks and a repo's own docs. It skips what git ignores, vendored code, data files, and anything that looks like a key."
          color: Theme.secondary
          font.pixelSize: 11
        }
      }
    }

    // Shortcut — Onboarding.swift ShortcutStep
    Flickable {
      id: shortcutFlick
      visible: root.step === 3
      anchors.fill: parent
      clip: true
      boundsBehavior: Flickable.StopAtBounds
      contentWidth: width
      contentHeight: shortcutCol.implicitHeight
      ScrollBar.vertical: ScrollBar {
        policy: shortcutFlick.contentHeight > shortcutFlick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
      }
      Column {
        id: shortcutCol
        width: parent.width
        spacing: 8

          StepTitle {
            width: parent.width
            title: "Open Trowel from anywhere"
            subtitle: "Press the shortcut in any app, describe what you're looking for, and press Return."
          }

          Item {
            width: parent.width
            height: 48
            HotkeyRecorder {
              anchors.horizontalCenter: parent.horizontalCenter
              chord: (root.client && root.client.config && root.client.config.hotkey) ? root.client.config.hotkey : "SUPER + D"
              onCaptured: function (chord) {
                if (root.client && root.client.setHotkey)
                  root.client.setHotkey(chord)
              }
            }
          }

          Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: "Try it now: press Super+D from another app after you're done here."
            color: Theme.secondary
            font.pixelSize: 13
            font.weight: Font.Medium
            topPadding: 8
          }

          Rectangle {
            width: Math.min(340, parent.width)
            anchors.horizontalCenter: parent.horizontalCenter
            radius: 12
            color: Theme.alpha(Theme.foreground, 0.04)
            implicitHeight: tipsCol.implicitHeight + 36
            Column {
              id: tipsCol
              x: 18
              y: 18
              width: parent.width - 36
              spacing: 9
              Text {
                text: "IN THE SEARCH PANEL"
                color: Theme.secondary
                font.pixelSize: 10
                font.weight: Font.DemiBold
              }
              KeyTip {
                width: parent.width
                caps: ["↩"]
                text: "Open the file"
              }
              KeyTip {
                width: parent.width
                caps: ["Ctrl", "↩"]
                text: "Show it in the file manager"
              }
              KeyTip {
                width: parent.width
                caps: ["Space"]
                text: "Quick Look, after arrowing to a result"
              }
              KeyTip {
                width: parent.width
                caps: ["Ctrl", "C"]
                text: "Copy the file, to paste anywhere"
              }
              KeyTip {
                width: parent.width
                caps: ["esc"]
                text: "Close"
              }
            }
          }
      }
    }

    // Ready — Onboarding.swift ReadyStep
    Flickable {
      id: readyFlick
      visible: root.step === 4
      anchors.fill: parent
      clip: true
      boundsBehavior: Flickable.StopAtBounds
      contentWidth: width
      contentHeight: readyCol.implicitHeight
      ScrollBar.vertical: ScrollBar {
        policy: readyFlick.contentHeight > readyFlick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
      }
      Column {
        id: readyCol
        width: parent.width
        spacing: 0

          Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: "You're all set"
            color: Theme.foreground
            font.pixelSize: 26
            font.weight: Font.Bold
            topPadding: 12
          }
          Text {
            width: Math.min(470, parent.width)
            anchors.horizontalCenter: parent.horizontalCenter
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
            text: root.modelInfo.present ? ("Trowel is ready to read your folders" + (root.foldersEmpty ? "." : ", newest files first. You can search right away; results fill in as it goes.")) : "Trowel starts reading your folders once the model has downloaded. Until then, search finds files by name."
            color: Theme.secondary
            font.pixelSize: 13
            topPadding: 8
          }

          Rectangle {
            width: Math.min(470, parent.width)
            anchors.horizontalCenter: parent.horizontalCenter
            radius: 12
            color: Theme.alpha(Theme.foreground, 0.04)
            implicitHeight: readySwitches.implicitHeight + 40
            Column {
              id: readySwitches
              width: parent.width - 40
              x: 20
              y: 20
              spacing: 16
              SwitchRow {
                width: parent.width
                title: "Open Trowel when you log in"
                detail: "So the shortcut always works."
                checked: root.launchAtLogin
                onToggled: function (on) {
                  root.launchAtLogin = on
                }
              }
              Rectangle {
                width: parent.width
                height: 1
                color: Theme.hairline
              }
              SwitchRow {
                width: parent.width
                title: "Index on battery power"
                detail: "When it's off, a folder's first pass waits for a charger. New files always go in right away."
                checked: root.indexOnBattery
                onToggled: function (on) {
                  root.indexOnBattery = on
                }
              }
            }
          }

          Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            topPadding: 22
            text: "Trowel lives in the status bar, which shows how indexing is going. Super+D opens search."
            color: Theme.secondary
            font.pixelSize: 12
            wrapMode: Text.Wrap
          }
      }
    }
  }

  Rectangle {
    id: footer
    z: 2
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    height: 64
    color: Theme.background

    Rectangle {
      anchors.top: parent.top
      width: parent.width
      height: Theme.hairlineWidth()
      color: Theme.hairline
    }

    Row {
      anchors.verticalCenter: parent.verticalCenter
      anchors.left: parent.left
      anchors.leftMargin: 20
      spacing: 6
      Repeater {
        model: root.stepCount
        delegate: Rectangle {
          width: 6
          height: 6
          radius: 3
          color: Theme.alpha(Theme.foreground, index === root.step ? 0.55 : 0.15)
        }
      }
    }

    Text {
      visible: root.step > 0
      anchors.verticalCenter: parent.verticalCenter
      anchors.left: parent.left
      anchors.leftMargin: 70
      text: root.step === 1 ? root.foldersSummary() : root.downloadLabel()
      color: Theme.secondary
      font.pixelSize: 12
      elide: Text.ElideRight
      width: Math.max(80, footer.width - 280)
    }

    Row {
      anchors.verticalCenter: parent.verticalCenter
      anchors.right: parent.right
      anchors.rightMargin: 20
      spacing: 10

      FooterButton {
        visible: root.step > 0 && root.step < root.stepCount - 1
        label: "Back"
        primary: false
        onClicked: root.back()
      }

      FooterButton {
        label: root.continueTitle
        primary: true
        enabled: !(root.step === 1 && root.foldersEmpty)
        onClicked: root.next()
      }
    }
  }

  // DigUp footer uses paired large controls (Onboarding.swift:262–269). Back is
  // a real button, not naked text; Continue is a high-contrast fill so a washed
  // theme accent cannot turn it into a chip.
  component FooterButton: Rectangle {
    id: fbtn
    property string label: ""
    property bool primary: false
    signal clicked
    readonly property bool blocked: !enabled
    implicitHeight: 36
    implicitWidth: Math.max(88, fbtnLabel.implicitWidth + 28)
    height: 36
    radius: 8
    color: {
      if (fbtn.blocked)
        return Theme.keycapFill
      if (fbtn.primary)
        return Theme.foreground
      return hover.containsMouse ? Theme.alpha(Theme.foreground, 0.12) : Theme.alpha(Theme.foreground, 0.08)
    }
    border.width: fbtn.primary ? 0 : Theme.hairlineWidth()
    border.color: Theme.hairline
    opacity: fbtn.blocked ? 0.55 : 1
    Text {
      id: fbtnLabel
      anchors.centerIn: parent
      text: fbtn.label
      color: fbtn.primary && !fbtn.blocked ? Theme.background : Theme.foreground
      font.pixelSize: 14
      font.weight: fbtn.primary ? Font.DemiBold : Font.Medium
    }
    MouseArea {
      id: hover
      anchors.fill: parent
      hoverEnabled: true
      enabled: !fbtn.blocked
      cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
      onClicked: fbtn.clicked()
    }
  }

  component FeatureRow: Row {
    id: feat
    property string kind: "doc"
    property string title: ""
    property string example: ""
    spacing: 14
    height: 42

    Rectangle {
      width: 34
      height: 34
      radius: 9
      color: Theme.brandMoss
      anchors.verticalCenter: parent.verticalCenter
      KindIcon {
        anchors.centerIn: parent
        kind: feat.kind
        side: 18
      }
    }
    Column {
      width: parent.width - 48
      anchors.verticalCenter: parent.verticalCenter
      spacing: 2
      Text {
        text: feat.title
        color: Theme.foreground
        font.pixelSize: 14
        font.weight: Font.DemiBold
      }
      Text {
        width: parent.width
        text: feat.example
        color: Theme.secondary
        font.pixelSize: 12
        elide: Text.ElideRight
      }
    }
  }

  component StepTitle: Column {
    property string title: ""
    property string subtitle: ""
    spacing: 6
    width: parent ? parent.width : 400
    Text {
      width: parent.width
      text: title
      color: Theme.foreground
      font.pixelSize: 24
      font.weight: Font.Bold
      wrapMode: Text.Wrap
    }
    Text {
      width: parent.width
      text: subtitle
      color: Theme.secondary
      font.pixelSize: 13
      wrapMode: Text.Wrap
    }
  }

  component SwitchRow: Item {
    id: sw
    property string title: ""
    property string detail: ""
    property bool checked: false
    signal toggled(bool on)
    width: parent ? parent.width : 400
    implicitHeight: Math.max(28, swCol.implicitHeight)

    Column {
      id: swCol
      anchors.left: parent.left
      anchors.right: knob.left
      anchors.rightMargin: 16
      spacing: 2
      Text {
        width: parent.width
        text: sw.title
        color: Theme.foreground
        font.pixelSize: 13
        font.weight: Font.Medium
        wrapMode: Text.Wrap
      }
      Text {
        width: parent.width
        text: sw.detail
        color: Theme.secondary
        font.pixelSize: 11
        wrapMode: Text.Wrap
      }
    }
    Rectangle {
      id: knob
      width: 44
      height: 24
      radius: 12
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      color: sw.checked ? Theme.accent : Theme.keycapFill
      Rectangle {
        width: 18
        height: 18
        radius: 9
        x: sw.checked ? 22 : 4
        y: 3
        color: Theme.foreground
      }
      MouseArea {
        anchors.fill: parent
        onClicked: sw.toggled(!sw.checked)
      }
    }
  }

  component KeyTip: Row {
    property var caps: []
    property string text: ""
    spacing: 10
    height: 22
    Row {
      spacing: 3
      width: 66
      Repeater {
        model: caps
        delegate: Keycap {
          label: modelData
          size: 11
        }
      }
    }
    Text {
      text: parent.text
      color: Theme.foreground
      font.pixelSize: 12
      anchors.verticalCenter: parent.verticalCenter
    }
  }

  Process {
    id: dirsProc
    property var acc: []
    command: ["sh", "-c", "for k in DESKTOP DOCUMENTS DOWNLOAD PICTURES VIDEOS MUSIC; do p=$(xdg-user-dir \"$k\" 2>/dev/null) || continue; [ -n \"$p\" ] && [ -d \"$p\" ] && printf '%s\\n' \"$p\"; done"]
    running: false
    stdout: SplitParser {
      onRead: function (line) {
        var p = String(line).replace(/^\s+|\s+$/g, "")
        if (!p.length)
          return
        var next = dirsProc.acc.slice()
        next.push(p)
        dirsProc.acc = next
      }
    }
    onExited: {
      var paths = dirsProc.acc.slice()
      dirsProc.acc = []
      if (paths.length === 0) {
        paths = [Theme.home + "/Desktop", Theme.home + "/Documents", Theme.home + "/Downloads", Theme.home + "/Pictures", Theme.home + "/Videos", Theme.home + "/Music"]
      }
      root.setFolderRows(paths)
      root.dirsReady = true
    }
  }

  Process {
    id: picker
    property string mode: "add"
    command: ["trowel", "portal-pick-folder"]
    running: false
    stdout: SplitParser {
      onRead: function (line) {
        var p = String(line).replace(/^\s+|\s+$/g, "")
        if (!p.length)
          return
        if (picker.mode === "skip")
          root.skipFolder(p)
        else if (picker.mode === "code")
          root.addCodeFolder(p)
        else
          root.addFolder(p)
      }
    }
  }

  Process {
    id: autostart
    running: false
  }
}
