# Capture presets

Apple is the default. It combines all 16 categories below, including Terminal.
Use `--preset broad` to capture the wider supported stores, including other Apple apps and third-party apps.
Use any category name with `--preset` to select that category alone.

Presets select exact, case-sensitive domains. They retain every supported field within those domains, including unknown keys, counters, and learned state. They do not substitute recommended values or identify which settings you changed manually. The shared generation support checks omit unsupported complete settings from every preset.

~~~sh
raskin capture -o myprefs.kdl
raskin capture --preset search -o search.kdl
raskin capture --preset terminal -o terminal.kdl
raskin capture --preset broad -o broad.kdl
~~~

## Categories and exact domains

| Preset | Selected domains or provider |
| --- | --- |
| `desktop` | `com.apple.dock`, `com.apple.finder`, `com.apple.desktopservices`, `com.apple.screencapture` |
| `input` | `NSGlobalDomain`, `com.apple.AppleMultitouchTrackpad`, `com.apple.driver.AppleBluetoothMultitouch.trackpad`, `com.apple.AppleMultitouchMouse`, `com.apple.driver.AppleBluetoothMultitouch.mouse`, `com.apple.driver.AppleHIDMouse`, `com.apple.HIToolbox`, `com.apple.keyboard.preferences`, `com.apple.keyboardtype`, `com.apple.keyboardservicesd` |
| `power` | Existing `pmset` battery, charger, and UPS profiles |
| `appearance` | `NSGlobalDomain` |
| `menubar` | `com.apple.controlcenter`, `com.apple.controlcenter.displayablemenuextras`, `com.apple.controlcenter.bentoboxes`, `com.apple.menuextra.clock`, `com.apple.systemuiserver` |
| `windows` | `NSGlobalDomain`, `com.apple.WindowManager`, `com.apple.spaces`, `com.apple.dock` |
| `accessibility` | `com.apple.Accessibility`, `com.apple.Accessibility.Assets`, `com.apple.universalaccess`, `com.apple.universalaccessAuthWarning`, `com.apple.accessibility.universalAccessAuthWarn`, `com.apple.AccessibilityUIServer`, `com.apple.AccessibilityHearingNearby`, `com.apple.AccessibilityVisualsAgent`, `com.apple.AssistiveControl.virtualKeyboard`, `com.apple.FullKeyboardAccess4.local`, `com.apple.accessibility.heard`, `com.apple.mediaaccessibility.public`, `com.apple.speech.recognition.AppleSpeechRecognition.CustomCommands` |
| `language` | `NSGlobalDomain`, `com.apple.HIToolbox`, `com.apple.textInput.keyboardServices.textReplacement`, `com.apple.TextInputMenu`, `com.apple.TextInputMenuAgent`, `com.apple.speech.recognition.AppleSpeechRecognition.prefs` |
| `shortcuts` | `NSGlobalDomain`, `com.apple.symbolichotkeys`, `com.apple.HIToolbox` |
| `search` | `com.apple.Spotlight`, `com.apple.SpotlightResources.Defaults`, `com.apple.spotlightknowledge`, `com.apple.corespotlightui`, `com.apple.IMCoreSpotlight`, `com.apple.parsecd`, `com.apple.spotlight.mdwrite` |
| `screenshots` | `com.apple.screencapture`, `com.apple.screencaptureui` |
| `lockscreen` | `com.apple.screensaver`, `com.apple.loginwindow` |
| `notifications` | `com.apple.ncprefs`, `com.apple.notificationcenterui`, `com.apple.donotdisturbd` |
| `sound` | `NSGlobalDomain`, `com.apple.soundpref`, `com.apple.ComfortSounds`, `com.apple.PersonalAudio`, `com.apple.BezelServices` |
| `updates` | `com.apple.SoftwareUpdate`, `com.apple.preferences.softwareupdate` |
| `terminal` | `com.apple.Terminal` |

## Overlap and scopes

Global preferences mix several categories. `appearance`, `input`, `windows`, `language`, `shortcuts`, and `sound` each retain the complete supported `NSGlobalDomain`. Dock and screenshot domains also overlap categories. Apple captures their union once, preserving user, system, host, and container identities separately.

Categories inspect matching user and system preference files, including ByHost files. They enter a container only when its exact identifier is a selected domain. The container identifier and the preference domain must belong to the same category. For example, `terminal` includes `com.apple.Terminal` inside its own container, but excludes that container's `NSGlobalDomain`. Apple preserves this rule when composing categories. Excluded preference files are not opened.

The existing generation mappings still determine which scopes are supported. System ByHost and current-host container values are omitted. Group containers and dedicated settings databases are outside category coverage. See [coverage](coverage.md) for provider limits.

Only Apple, broad, and power read the power provider. Power reads no preference stores. Lockscreen includes persisted screen saver and login-window preferences; battery/charger display sleep remains in power.

## Retention and limits

`desktop` and `input` previously selected short key lists from user preferences. They now retain whole supported domains and inspect the selected system/container sources too. Screenshot paths, Dock tiles, saved application state, and unfamiliar fields can therefore appear in these captures. Absolute home paths remain literal. A category selection does not guarantee portability or complete coverage of a System Settings pane.

Apple excludes unlisted domains, including standalone Apple apps other than Terminal and third-party apps. `broad` retains its previous enumeration and unsupported-provider diagnostics. It remains useful for inspecting preferences left by old applications. No preset deletes anything.

Power support depends on hardware and macOS. Unknown or non-writable fields and unsupported values are omitted with diagnostics. Battery, charger, and UPS profiles remain separate.

Normal stderr reports captured counts, power profiles, and a compact omission summary. `--verbose` adds per-source details and preset exclusion counts. Omission records never enter captured KDL. Saved KDL remains usable without the preset implementation. The selectors and source boundaries are covered by [synthetic capture tests](../tests/capture.rs).
