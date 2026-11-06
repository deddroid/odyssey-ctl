---
name: Monitor report
about: A setting does not work, or you have a different Odyssey model or firmware
title: "[Monitor] "
labels: hardware
---

**Monitor and firmware** (OSD → Support → Information): model code and software version.

**Connection**: HDMI / DisplayPort, graphics card and driver, Linux or Windows.

**Output of these commands** (no personal data is printed except the monitor serial, which you may blank):

```
odyssey-ctl list
odyssey-ctl info
odyssey-ctl get
```

**What you tried and what the monitor's own menu showed afterwards.**
If a write seems ignored, add the output of the same command with `ODYSSEY_CTL_DEBUG=1`.
