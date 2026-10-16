//! Onboarding must match DigUp's wizard: extra folders, skip, real chrome.
//! `upstream/Sources/DigUpApp/Onboarding.swift`

use std::fs;
use std::path::PathBuf;

fn onboarding_qml() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/qml/Trowel/Onboarding.qml");
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

#[test]
fn onboarding_matches_digup_wizard() {
    let qml = onboarding_qml();
    for needle in [
        "Get Started",
        "Add Folder…",
        "Skip a Folder…",
        "Skip some folders or file types",
        "What should Trowel search?",
        "Search your code too?",
        "Open Trowel from anywhere",
        "You're all set",
        "Index on battery power",
        "Open Trowel when you log in",
        "portal-pick-folder",
        "DESKTOP DOCUMENTS DOWNLOAD PICTURES VIDEOS MUSIC",
        "TROWEL_ONBOARDING_LOCATIONS",
        "TROWEL_ONBOARDING_STEP",
        "Describe it. Dig it up.",
        "Screenshots and photos",
        "Choose at least one folder",
    ] {
        assert!(qml.contains(needle), "onboarding is missing {needle:?}");
    }
    assert!(
        qml.contains("implicitWidth: 680"),
        "window width must match OnboardingView.size 680"
    );
    assert!(
        qml.contains("implicitHeight: 600"),
        "window height must match OnboardingView.size 600"
    );
    assert!(
        qml.contains("id: foldersFlick"),
        "folder list must scroll inside a fixed-size dialog"
    );
    assert!(
        qml.contains("ScrollBar.vertical"),
        "overflowing folder list must show a scrollbar"
    );
    assert!(
        qml.contains("\"onboarded\": true"),
        "folders Continue must set onboarded so indexing starts (Onboarding.swift:142)"
    );
}
