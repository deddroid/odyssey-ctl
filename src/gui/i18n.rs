//! English / Italian strings for the GUI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    It,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::It];

    /// Italian when the system locale is Italian, English otherwise.
    pub fn from_env() -> Self {
        let locale = std::env::var("LC_ALL").or_else(|_| std::env::var("LANG")).unwrap_or_default();
        if locale.starts_with("it") { Lang::It } else { Lang::En }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::It => "it",
        }
    }

    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::It => "Italiano",
        }
    }
}

/// (key, English, Italian)
const STRINGS: &[(&str, &str, &str)] = &[
    ("tab.picture", "Picture", "Immagine"),
    ("tab.game", "Game", "Gioco"),
    ("tab.lighting", "Lighting", "Luci"),
    ("tab.source", "Source & sound", "Sorgente e audio"),
    ("tab.info", "Info & settings", "Info e impostazioni"),
    ("refresh", "Refresh", "Aggiorna"),
    ("busy", "Talking to the monitor...", "Comunicazione con il monitor..."),
    ("loaded", "Settings read from the monitor.", "Impostazioni lette dal monitor."),
    ("not_supported", "not supported", "non supportata"),
    ("raw_value", "raw value", "valore grezzo"),
    ("raw_hint", "format not decoded yet, read only", "formato non ancora decodificato, sola lettura"),
    ("no_data", "Not read yet. Press Refresh.", "Non ancora letto. Premi Aggiorna."),
    ("locked_hint", "Not verified on hardware yet: enable it in Info & settings.", "Non ancora verificata sul monitor: abilitala in Info e impostazioni."),
    ("locked_banner", "Some settings are locked because they are not verified on this monitor yet.", "Alcune voci sono bloccate perché non ancora verificate su questo monitor."),
    ("unlock", "Unlock", "Sblocca"),
    ("status.verified", "verified", "verificata"),
    ("status.standard", "standard", "standard"),
    ("status.read-verified", "read verified", "lettura verificata"),
    ("status.unverified", "unverified", "non verificata"),
    ("monitor", "Monitor", "Monitor"),
    ("model", "Firmware profile", "Profilo firmware"),
    ("profile.lemon", "(extended: Samsung Display Manager)", "(esteso: Samsung Display Manager)"),
    ("profile.sec", "(standard retail profile)", "(profilo standard)"),
    ("profile.falcon", "(old firmware: limited writes)", "(firmware vecchio: scritture limitate)"),
    ("mccs", "MCCS version", "Versione MCCS"),
    ("codes", "VCP codes", "Codici VCP"),
    ("caps_raw", "Capability string", "Capability string"),
    ("sdm", "Samsung extended profile (VCP EB)", "Profilo esteso Samsung (VCP EB)"),
    ("sdm_hint", "Switches the capability list to the one used by Samsung Display Manager. Picture settings do not change.", "Passa alla lista di capability usata da Samsung Display Manager. Le impostazioni dell'immagine non cambiano."),
    ("sdm_on", "Enable (EB=A1)", "Attiva (EB=A1)"),
    ("sdm_off", "Disable (EB=A2)", "Disattiva (EB=A2)"),
    ("app_settings", "App settings", "Impostazioni dell'app"),
    ("language", "Language", "Lingua"),
    ("allow_unverified", "Allow changing unverified settings", "Consenti di cambiare impostazioni non verificate"),
    ("allow_unverified_hint", "These codes come from Samsung's software and were not tested on this monitor. Every change is read back.", "Questi codici vengono dal software Samsung e non sono stati provati su questo monitor. Ogni modifica viene riletta."),
    ("demo", "Demo mode: no real monitor is used.", "Modalità demo: nessun monitor reale."),
    ("f.brightness", "Brightness", "Luminosità"),
    ("f.contrast", "Contrast", "Contrasto"),
    ("f.sharpness", "Sharpness", "Nitidezza"),
    ("f.red-gain", "White balance: red", "Bilanciamento del bianco: rosso"),
    ("f.green-gain", "White balance: green", "Bilanciamento del bianco: verde"),
    ("f.blue-gain", "White balance: blue", "Bilanciamento del bianco: blu"),
    ("f.picture-mode", "Picture mode", "Modalità immagine"),
    ("f.black-equalizer", "Black Equalizer", "Equalizzatore del nero"),
    ("f.color-tone", "Color tone", "Tonalità colore"),
    ("f.eye-saver", "Eye Saver Mode", "Modalità Eye Saver"),
    ("f.input", "Input source", "Sorgente di ingresso"),
    ("f.volume", "Volume", "Volume"),
    ("f.mute", "Audio mute", "Audio muto"),
    ("f.language", "OSD language", "Lingua dell'OSD"),
    ("f.adaptive-sync", "Adaptive-Sync / FreeSync", "Adaptive-Sync / FreeSync"),
    ("f.aim-point", "Virtual Aim Point", "Mirino virtuale"),
    ("f.lighting", "Infinity Core Lighting", "Infinity Core Lighting"),
    ("f.lighting-effect", "Lighting effect", "Effetto luci"),
    ("f.lighting-color", "Lighting color", "Colore luci"),
    ("f.pip", "PIP/PBP status", "Stato PIP/PBP"),
    ("f.pip-source", "PIP/PBP sources", "Sorgenti PIP/PBP"),
    ("f.pip-sound", "PIP/PBP sound", "Audio PIP/PBP"),
    ("f.pc-av-mode", "PC/AV mode", "Modalità PC/AV"),
    ("v.entertain", "Entertain", "Intrattenimento"),
    ("v.graphic", "Graphic", "Grafico"),
    ("v.eco", "Eco", "Eco"),
    ("v.game-standard", "Game Standard", "Standard di gioco"),
    ("v.rpg", "RPG", "RPG"),
    ("v.rts", "RTS", "RTS"),
    ("v.fps", "FPS", "FPS"),
    ("v.sports", "Sports", "Sport"),
    ("v.original", "Original", "Originale"),
    ("v.custom", "Custom", "Personalizzato"),
    ("disabled_now", "disabled in the current state", "disattivata in questo stato"),
    ("v.cool", "Cool", "Freddo"),
    ("v.standard", "Standard", "Standard"),
    ("v.warm1", "Warm 1", "Caldo 1"),
    ("v.warm2", "Warm 2", "Caldo 2"),
    ("v.natural", "Natural", "Naturale"),
    ("v.off", "Off", "Spento"),
    ("v.on", "On", "Acceso"),
    ("v.low", "Low", "Basso"),
    ("v.high", "High", "Alto"),
    ("v.pc", "PC", "PC"),
    ("v.av", "AV", "AV"),
    ("v.a-red", "Type A, red", "Tipo A, rosso"),
    ("v.a-blue", "Type A, blue", "Tipo A, blu"),
    ("v.a-white", "Type A, white", "Tipo A, bianco"),
    ("v.b-red", "Type B, red", "Tipo B, rosso"),
    ("v.b-blue", "Type B, blue", "Tipo B, blu"),
    ("v.b-white", "Type B, white", "Tipo B, bianco"),
    ("v.reset-position", "Reset position", "Ripristina posizione"),
    ("v.aurora", "Aurora", "Aurora"),
    ("v.aurora-rotation", "Aurora rotation", "Aurora rotante"),
    ("v.comet", "Comet", "Cometa"),
    ("v.static", "Static", "Statico"),
    ("v.rainbow", "Rainbow", "Arcobaleno"),
    ("v.breathing", "Breathing", "Respiro"),
    ("v.main", "Main screen", "Schermo principale"),
    ("v.channel1", "Sub screen 1", "Schermo secondario 1"),
    ("v.channel2", "Sub screen 2", "Schermo secondario 2"),
    ("v.chinese", "Chinese", "Cinese"),
    ("v.english", "English", "Inglese"),
    ("v.french", "French", "Francese"),
    ("v.german", "German", "Tedesco"),
    ("v.italian", "Italian", "Italiano"),
    ("v.japanese", "Japanese", "Giapponese"),
    ("v.korean", "Korean", "Coreano"),
    ("v.portuguese", "Portuguese", "Portoghese"),
    ("v.russian", "Russian", "Russo"),
    ("v.spanish", "Spanish", "Spagnolo"),
    ("v.swedish", "Swedish", "Svedese"),
    ("v.turkish", "Turkish", "Turco"),
    ("v.hungarian", "Hungarian", "Ungherese"),
    ("v.polish", "Polish", "Polacco"),
    ("v.ukrainian", "Ukrainian", "Ucraino"),
    ("v.kyrgyz", "Kyrgyz", "Kirghiso"),
    ("v.displayport", "DisplayPort", "DisplayPort"),
    ("v.hdmi1", "HDMI 1", "HDMI 1"),
    ("v.hdmi2", "HDMI 2", "HDMI 2"),
];

/// Translated string for `key`; the key itself when it is missing.
pub fn tr(lang: Lang, key: &str) -> String {
    match STRINGS.iter().find(|(k, _, _)| *k == key) {
        Some((_, en, it)) => match lang {
            Lang::En => en,
            Lang::It => it,
        }
        .to_string(),
        None => key.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use odyssey_ctl::features::FEATURES;

    fn has(key: &str) -> bool {
        STRINGS.iter().any(|(k, _, _)| *k == key)
    }

    #[test]
    fn every_feature_and_value_is_translated() {
        for f in FEATURES {
            assert!(has(&format!("f.{}", f.name)), "f.{}", f.name);
            for (_, v) in f.values {
                assert!(has(&format!("v.{v}")), "v.{v}");
            }
            assert!(has(&format!("status.{}", f.status.as_str())));
        }
    }

    #[test]
    fn keys_are_unique() {
        for (i, a) in STRINGS.iter().enumerate() {
            assert!(STRINGS[i + 1..].iter().all(|b| a.0 != b.0), "{}", a.0);
        }
    }
}
