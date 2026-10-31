# Capture presets

Broad is the default and has no curated preference-key filter. It reads supported stores and records unsupported or unreadable sources.

The three curated presets preserve captured values. They do not substitute recommended values. Desktop and input include matching user/any-host and user/current-host preferences, but exclude system and container stores.

| Preset | Domain/mechanism | Included keys |
| --- | --- | --- |
| desktop | com.apple.dock | autohide, orientation, tilesize, magnification, largesize, mineffect, show-recents, minimize-to-application, mru-spaces |
| desktop | com.apple.finder | AppleShowAllFiles, ShowPathbar, ShowStatusBar, FXPreferredViewStyle |
| desktop | com.apple.screencapture | type, show-thumbnail, showsClicks |
| input | NSGlobalDomain | InitialKeyRepeat, KeyRepeat, AppleKeyboardUIMode, com.apple.swipescrolldirection |
| input | com.apple.AppleMultitouchTrackpad | Clicking, TrackpadRightClick, TrackpadThreeFingerDrag |
| input | com.apple.driver.AppleBluetoothMultitouch.trackpad | Clicking, TrackpadRightClick, TrackpadThreeFingerDrag |
| power | pmset battery/charger/UPS | Documented writable integer settings, with profiles kept separate |

Desktop excludes screenshot paths, Dock tiles, application state, and power. Input excludes unrelated global preferences. Power reads no preference files. Curated presets exclude unrelated dedicated providers rather than label them failed.

Power support depends on destination hardware and macOS. Unknown/read-only fields and unsupported non-integer fields are recorded as unavailable. Generation validates documented mappings and ranges.

Diagnostics identify the preset, included key count, filtered keys within selected domains, and excluded discovered sources. KDL remains usable without the preset implementation. Exact selections are tested in [capture fixtures](../tests/capture.rs).
