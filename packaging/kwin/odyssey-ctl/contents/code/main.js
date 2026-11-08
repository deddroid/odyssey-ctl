// KWin script: tells Odyssey Control (odyssey-ctl-gui, over D-Bus) which window got the focus
// and which application windows are open, so it can apply the preset of a per-application rule
// and offer the open windows when a rule is added. Nothing is polled: one call per focus change
// or window added/removed. Installed and enabled from the app's Presets page.
const SERVICE = "io.github.deddroid.OdysseyCtl";
const PATH = "/io/github/deddroid/OdysseyCtl";

function isApplication(window) {
    // Panels, the desktop, popups and tooltips are not applications.
    return window && window.normalWindow;
}

// One line per window class: "class<US>title", so no array marshalling is needed.
function reportAll() {
    const seen = {};
    const lines = [];
    for (const window of workspace.windowList()) {
        if (!isApplication(window)) {
            continue;
        }
        const cls = String(window.resourceClass);
        if (seen[cls]) {
            continue;
        }
        seen[cls] = true;
        lines.push(cls + "\u001f" + String(window.caption));
    }
    callDBus(SERVICE, PATH, SERVICE, "Windows", lines.join("\n"));
}

function report(window) {
    reportAll();
    if (window && window.desktopWindow) {
        // Clicking the desktop: no application has the focus.
        callDBus(SERVICE, PATH, SERVICE, "WindowActivated", "", "");
        return;
    }
    if (!isApplication(window)) {
        return;
    }
    callDBus(SERVICE, PATH, SERVICE, "WindowActivated",
        String(window.resourceClass), String(window.caption));
}

workspace.windowActivated.connect(report);
workspace.windowAdded.connect(reportAll);
workspace.windowRemoved.connect(reportAll);
report(workspace.activeWindow);
