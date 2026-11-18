//! Named capture selections. A preset selects stored domains, not recommended values.
use std::{fmt, str::FromStr};

use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preset {
    #[default]
    Apple,
    Broad,
    Desktop,
    Input,
    Power,
    Appearance,
    Menubar,
    Windows,
    Accessibility,
    Language,
    Shortcuts,
    Search,
    Screenshots,
    Lockscreen,
    Notifications,
    Sound,
    Updates,
    Terminal,
}

impl Preset {
    /// Apple is exactly the union of these categories, including their source scopes.
    pub const CATEGORIES: &[Self] = &[
        Self::Desktop,
        Self::Input,
        Self::Power,
        Self::Appearance,
        Self::Menubar,
        Self::Windows,
        Self::Accessibility,
        Self::Language,
        Self::Shortcuts,
        Self::Search,
        Self::Screenshots,
        Self::Lockscreen,
        Self::Notifications,
        Self::Sound,
        Self::Updates,
        Self::Terminal,
    ];

    /// Stable CLI choice order, shared by parsing and interface documentation.
    pub fn all() -> impl Iterator<Item = Self> {
        [Self::Apple, Self::Broad]
            .into_iter()
            .chain(Self::CATEGORIES.iter().copied())
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apple => "apple",
            Self::Broad => "broad",
            Self::Desktop => "desktop",
            Self::Input => "input",
            Self::Power => "power",
            Self::Appearance => "appearance",
            Self::Menubar => "menubar",
            Self::Windows => "windows",
            Self::Accessibility => "accessibility",
            Self::Language => "language",
            Self::Shortcuts => "shortcuts",
            Self::Search => "search",
            Self::Screenshots => "screenshots",
            Self::Lockscreen => "lockscreen",
            Self::Notifications => "notifications",
            Self::Sound => "sound",
            Self::Updates => "updates",
            Self::Terminal => "terminal",
        }
    }

    pub(crate) fn includes_preferences(self) -> bool {
        self != Self::Power
    }

    pub(crate) fn includes_power(self) -> bool {
        match self {
            Self::Apple => Self::CATEGORIES
                .iter()
                .any(|category| category.includes_power()),
            Self::Broad | Self::Power => true,
            _ => false,
        }
    }

    pub(crate) fn includes_container(self, id: &str) -> bool {
        match self {
            Self::Broad => true,
            Self::Apple => Self::CATEGORIES
                .iter()
                .any(|category| category.includes_container(id)),
            _ => id != "NSGlobalDomain" && self.domains().contains(&id),
        }
    }

    pub(crate) fn includes_source(self, name: &str, container: Option<&str>) -> bool {
        match self {
            Self::Broad => true,
            // Compose complete source selections. Independently unioning containers
            // and domains would admit pairs that no category selects.
            Self::Apple => Self::CATEGORIES
                .iter()
                .any(|category| category.includes_source(name, container)),
            _ => {
                self.domains().contains(&name)
                    && container.is_none_or(|id| self.includes_container(id))
            }
        }
    }

    /// Exact, case-sensitive domains. Keep docs/presets.md aligned with this list.
    /// Shared global domains intentionally retain every supported field.
    fn domains(self) -> &'static [&'static str] {
        match self {
            Self::Apple | Self::Broad => &[],
            Self::Desktop => &[
                "com.apple.dock",
                "com.apple.finder",
                "com.apple.desktopservices",
                "com.apple.screencapture",
            ],
            Self::Input => &[
                "NSGlobalDomain",
                "com.apple.AppleMultitouchTrackpad",
                "com.apple.driver.AppleBluetoothMultitouch.trackpad",
                "com.apple.AppleMultitouchMouse",
                "com.apple.driver.AppleBluetoothMultitouch.mouse",
                "com.apple.driver.AppleHIDMouse",
                "com.apple.HIToolbox",
                "com.apple.keyboard.preferences",
                "com.apple.keyboardtype",
                "com.apple.keyboardservicesd",
            ],
            Self::Power => &[],
            Self::Appearance => &["NSGlobalDomain"],
            Self::Menubar => &[
                "com.apple.controlcenter",
                "com.apple.controlcenter.displayablemenuextras",
                "com.apple.controlcenter.bentoboxes",
                "com.apple.menuextra.clock",
                "com.apple.systemuiserver",
            ],
            Self::Windows => &[
                "NSGlobalDomain",
                "com.apple.WindowManager",
                "com.apple.spaces",
                "com.apple.dock",
            ],
            Self::Accessibility => &[
                "com.apple.Accessibility",
                "com.apple.Accessibility.Assets",
                "com.apple.universalaccess",
                "com.apple.universalaccessAuthWarning",
                "com.apple.accessibility.universalAccessAuthWarn",
                "com.apple.AccessibilityUIServer",
                "com.apple.AccessibilityHearingNearby",
                "com.apple.AccessibilityVisualsAgent",
                "com.apple.AssistiveControl.virtualKeyboard",
                "com.apple.FullKeyboardAccess4.local",
                "com.apple.accessibility.heard",
                "com.apple.mediaaccessibility.public",
                "com.apple.speech.recognition.AppleSpeechRecognition.CustomCommands",
            ],
            Self::Language => &[
                "NSGlobalDomain",
                "com.apple.HIToolbox",
                "com.apple.textInput.keyboardServices.textReplacement",
                "com.apple.TextInputMenu",
                "com.apple.TextInputMenuAgent",
                "com.apple.speech.recognition.AppleSpeechRecognition.prefs",
            ],
            Self::Shortcuts => &[
                "NSGlobalDomain",
                "com.apple.symbolichotkeys",
                "com.apple.HIToolbox",
            ],
            Self::Search => &[
                "com.apple.Spotlight",
                "com.apple.SpotlightResources.Defaults",
                "com.apple.spotlightknowledge",
                "com.apple.corespotlightui",
                "com.apple.IMCoreSpotlight",
                "com.apple.parsecd",
                "com.apple.spotlight.mdwrite",
            ],
            Self::Screenshots => &["com.apple.screencapture", "com.apple.screencaptureui"],
            Self::Lockscreen => &["com.apple.screensaver", "com.apple.loginwindow"],
            Self::Notifications => &[
                "com.apple.ncprefs",
                "com.apple.notificationcenterui",
                "com.apple.donotdisturbd",
            ],
            Self::Sound => &[
                "NSGlobalDomain",
                "com.apple.soundpref",
                "com.apple.ComfortSounds",
                "com.apple.PersonalAudio",
                "com.apple.BezelServices",
            ],
            Self::Updates => &[
                "com.apple.SoftwareUpdate",
                "com.apple.preferences.softwareupdate",
            ],
            Self::Terminal => &["com.apple.Terminal"],
        }
    }
}

impl fmt::Display for Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Preset {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        Self::all()
            .find(|preset| preset.as_str() == value)
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "unknown preset {value:?}; expected {}",
                    Self::all().map(Self::as_str).collect::<Vec<_>>().join(", ")
                ))
            })
    }
}
