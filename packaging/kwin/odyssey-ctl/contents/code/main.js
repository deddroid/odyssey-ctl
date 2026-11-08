// KWin script: tells Odyssey Control (odyssey-ctl-gui, over D-Bus) which window got the focus,
// so it can apply the preset of a per-application rule. Nothing is polled: one call per focus
// change. Installed and enabled from the app's Presets page.
const SERVICE = "io.github.deddroid.OdysseyCtl";
const PATH = "/io/github/deddroid/OdysseyCtl";

function report(window) {
    if (!window || !window.normalWindow) {
        return; // panels, the desktop, popups and tooltips are not applications
    }
    callDBus(SERVICE, PATH, SERVICE, "WindowActivated",
        String(window.resourceClass), String(window.caption));
}

workspace.windowActivated.connect(report);
report(workspace.activeWindow);
