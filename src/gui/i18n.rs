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
        let locale = std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default();
        if locale.starts_with("it") {
            Lang::It
        } else {
            Lang::En
        }
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
    ("nav.presets", "Presets", "Preset"),
    ("tray.show", "Show window", "Mostra finestra"),
    ("tray.quit", "Quit", "Esci"),
    ("card.desktop", "Tray and startup", "Tray e avvio"),
    (
        "card.desktop.hint",
        "From the tray icon you can apply presets without opening the window. Tray changes take effect at the next start.",
        "Dall'icona nel tray puoi applicare i preset senza aprire la finestra. Le modifiche al tray valgono dal prossimo avvio.",
    ),
    (
        "row.close_to_tray",
        "Keep in the tray when closed",
        "Resta nel tray alla chiusura",
    ),
    (
        "row.start_hidden",
        "Start minimised in the tray",
        "Avvia ridotto nel tray",
    ),
    ("row.autostart", "Start at login", "Avvia all'accesso"),
    (
        "card.save_preset",
        "Save the current state",
        "Salva lo stato attuale",
    ),
    (
        "card.save_preset.hint",
        "Stores picture, game and lighting settings as they are now. Input and OSD language are not included.",
        "Memorizza le impostazioni di immagine, gioco e luci come sono adesso. Ingresso e lingua dell'OSD non sono inclusi.",
    ),
    (
        "preset_name",
        "Preset name, e.g. Night",
        "Nome del preset, es. Notte",
    ),
    ("save", "Save", "Salva"),
    ("card.presets", "Your presets", "I tuoi preset"),
    (
        "card.presets.hint",
        "Applying a preset changes only the settings that differ from the monitor.",
        "Applicando un preset cambiano solo le impostazioni diverse da quelle del monitor.",
    ),
    (
        "presets_empty",
        "No presets yet. Set the monitor the way you like it and save it above.",
        "Nessun preset. Regola il monitor come preferisci e salvalo qui sopra.",
    ),
    ("apply", "Apply", "Applica"),
    ("overwrite", "Overwrite", "Sovrascrivi"),
    ("delete", "Delete", "Elimina"),
    ("confirm_delete", "Confirm delete", "Conferma eliminazione"),
    ("active", "active", "attivo"),
    ("n_settings", "{n} settings", "{n} impostazioni"),
    (
        "preset_saved",
        "Preset “{name}” saved.",
        "Preset “{name}” salvato.",
    ),
    (
        "preset_applied",
        "Preset “{name}” applied: {n} changes.",
        "Preset “{name}” applicato: {n} modifiche.",
    ),
    ("nav.picture", "Picture", "Immagine"),
    ("nav.game", "Game", "Gioco"),
    ("nav.lighting", "Lighting", "Luci"),
    ("nav.source", "Inputs & audio", "Ingressi e audio"),
    ("nav.monitor", "Monitor", "Monitor"),
    ("nav.settings", "Settings", "Impostazioni"),
    ("refresh", "Refresh", "Aggiorna"),
    (
        "busy",
        "Talking to the monitor…",
        "Comunicazione con il monitor…",
    ),
    (
        "loaded",
        "Settings read from the monitor.",
        "Impostazioni lette dal monitor.",
    ),
    ("connected", "Connected", "Connesso"),
    ("disconnected", "Not connected", "Non connesso"),
    ("demo_mode", "Demo mode", "Modalità demo"),
    ("no_data", "not read yet", "non ancora letto"),
    ("not_supported", "not supported", "non supportata"),
    (
        "disabled_now",
        "not available right now",
        "non disponibile ora",
    ),
    ("raw_value", "raw value", "valore grezzo"),
    ("locked", "to verify", "da verificare"),
    (
        "locked_hint",
        "Not tested on this monitor yet. You can allow it in Settings.",
        "Non ancora provata su questo monitor. Puoi abilitarla in Impostazioni.",
    ),
    ("card.mode", "Picture mode", "Modalità immagine"),
    (
        "card.mode.hint",
        "Monitor preset for the kind of content. Changing it also changes brightness, colours and sharpness.",
        "Preset del monitor per il tipo di contenuto. Cambiandolo cambiano anche luminosità, colori e nitidezza.",
    ),
    (
        "card.basic",
        "Brightness and sharpness",
        "Luminosità e nitidezza",
    ),
    ("card.color", "Colour", "Colore"),
    (
        "card.color.hint",
        "Colour tone and white balance of the current picture mode.",
        "Tonalità e bilanciamento del bianco della modalità attuale.",
    ),
    ("card.comfort", "Eye comfort", "Comfort visivo"),
    (
        "card.comfort.hint",
        "Eye Saver reduces blue light. While it is on, the monitor locks brightness, colour and picture mode.",
        "Eye Saver riduce la luce blu. Mentre è attivo il monitor blocca luminosità, colore e modalità immagine.",
    ),
    ("card.black_eq", "Black Equalizer", "Equalizzatore del nero"),
    (
        "card.black_eq.hint",
        "Brightens dark areas so you can see more in the shadows.",
        "Schiarisce le zone scure per vedere meglio nelle ombre.",
    ),
    ("card.aim", "Virtual Aim Point", "Mirino virtuale"),
    (
        "card.aim.hint",
        "Draws a crosshair on the screen; pick an icon, then place it.",
        "Disegna un mirino sullo schermo; scegli un'icona, poi posizionalo.",
    ),
    ("aim.position", "Position", "Posizione"),
    (
        "aim.area",
        "Area: from {x}, {y}, {w} × {h}",
        "Area: da {x}, {y}, {w} × {h}",
    ),
    ("aim.steps", "Step", "Passo"),
    ("aim.type", "Icon", "Icona"),
    ("v.type-a", "Type A", "Tipo A"),
    ("v.type-b", "Type B", "Tipo B"),
    ("v.light-blue", "Light blue", "Azzurro"),
    ("v.red", "Red", "Rosso"),
    ("v.white", "White", "Bianco"),
    ("row.theme", "Theme", "Tema"),
    ("theme.system", "System", "Sistema"),
    ("theme.light", "Light", "Chiaro"),
    ("theme.dark", "Dark", "Scuro"),
    ("aim.center", "Centre", "Centra"),
    ("aim.at", "Aim point at {x}, {y}", "Mirino in {x}, {y}"),
    (
        "aim.pad_hint",
        "Click on the screen outline where the crosshair should be, then fine-tune with the arrows.",
        "Clicca sulla sagoma dello schermo dove vuoi il mirino, poi rifinisci con le frecce.",
    ),
    ("aim.saved", "Saved positions", "Posizioni salvate"),
    ("aim.save", "Save position", "Salva posizione"),
    ("aim.name", "Name", "Nome"),
    (
        "card.sync",
        "Adaptive-Sync / FreeSync",
        "Adaptive-Sync / FreeSync",
    ),
    (
        "card.sync.hint",
        "Variable refresh rate. The lighting is hidden by the monitor while it is on.",
        "Frequenza di aggiornamento variabile. Finché è attiva il monitor nasconde le luci.",
    ),
    (
        "card.lighting",
        "Infinity Core Lighting",
        "Infinity Core Lighting",
    ),
    (
        "card.lighting.hint",
        "LED lighting on the back of the monitor.",
        "Luci LED sul retro del monitor.",
    ),
    ("card.input", "Input", "Ingresso"),
    ("card.audio", "Audio", "Audio"),
    ("card.pip", "Picture in Picture", "Picture in Picture"),
    (
        "card.pip.hint",
        "A second input in a window. While it is on the monitor locks several game and picture settings.",
        "Un secondo ingresso in una finestra. Finché è attivo il monitor blocca varie impostazioni di gioco e immagine.",
    ),
    ("pip.size", "Window size", "Dimensione finestra"),
    ("pip.corner", "Corner", "Angolo"),
    ("pip.corner.0", "Top left", "In alto a sinistra"),
    ("pip.corner.1", "Top right", "In alto a destra"),
    ("pip.corner.2", "Bottom left", "In basso a sinistra"),
    ("pip.corner.3", "Bottom right", "In basso a destra"),
    ("pip.main", "Main input", "Ingresso principale"),
    ("pip.sub", "Window input", "Ingresso finestra"),
    ("pip.done", "PIP {state}", "PIP {state}"),
    ("row.coresync", "CoreSync", "CoreSync"),
    (
        "coresync.hint",
        "CoreSync: the lighting follows the colours on screen (needs the lighting on).",
        "CoreSync: le luci seguono i colori sullo schermo (richiede le luci accese).",
    ),
    ("card.monitors", "Monitors found", "Monitor trovati"),
    (
        "card.monitors.hint",
        "The window controls one monitor at a time.",
        "La finestra controlla un monitor alla volta.",
    ),
    ("use_monitor", "Use", "Usa"),
    ("current", "in use", "in uso"),
    ("rescan", "Rescan", "Cerca di nuovo"),
    (
        "no_monitors",
        "No monitor found.",
        "Nessun monitor trovato.",
    ),
    ("card.osd", "Monitor menu", "Menu del monitor"),
    ("card.monitor", "Monitor", "Monitor"),
    (
        "card.sdm",
        "Samsung extended profile",
        "Profilo esteso Samsung",
    ),
    (
        "card.sdm.hint",
        "Switches the capability list to the one used by Samsung Display Manager (VCP EB). Picture settings do not change.",
        "Passa alla lista di funzioni usata da Samsung Display Manager (VCP EB). Le impostazioni dell'immagine non cambiano.",
    ),
    ("card.app", "Application", "Applicazione"),
    ("card.about", "About", "Informazioni"),
    (
        "card.about.hint",
        "Samsung codes come from Samsung Display Manager; every change is read back from the monitor.",
        "I codici Samsung vengono da Samsung Display Manager; ogni modifica viene riletta dal monitor.",
    ),
    ("row.level", "Level", "Livello"),
    ("row.state", "State", "Stato"),
    ("row.type", "Type", "Tipo"),
    ("row.lighting", "Lights", "Luci"),
    ("row.effect", "Effect", "Effetto"),
    ("row.colour", "Colour", "Colore"),
    ("row.input", "Source", "Sorgente"),
    ("row.volume", "Volume", "Volume"),
    ("row.supported", "Supported", "Supportato"),
    ("row.sources", "Sources", "Sorgenti"),
    ("row.main", "main", "principale"),
    ("row.sub", "sub", "secondaria"),
    ("row.pip_sound", "Sound from", "Audio da"),
    ("row.name", "Model", "Modello"),
    ("row.profile", "Profile", "Profilo"),
    ("row.mccs", "MCCS", "MCCS"),
    ("row.codes", "VCP codes", "Codici VCP"),
    ("row.language", "Language", "Lingua"),
    (
        "row.unverified",
        "Unverified settings",
        "Voci non verificate",
    ),
    ("row.version", "Version", "Versione"),
    (
        "unverified_hint",
        "Allows changing settings not yet tested on this monitor. Every change is read back.",
        "Permette di cambiare voci non ancora provate su questo monitor. Ogni modifica viene riletta.",
    ),
    ("sync.on", "On", "Attivo"),
    ("sync.off", "Off", "Spento"),
    (
        "profile.lemon",
        "extended (Samsung Display Manager)",
        "esteso (Samsung Display Manager)",
    ),
    ("profile.sec", "standard", "standard"),
    (
        "profile.falcon",
        "old firmware, limited writes",
        "firmware vecchio, scritture limitate",
    ),
    ("caps_raw", "Capability string", "Capability string"),
    ("sdm_on", "Enable", "Attiva"),
    ("sdm_off", "Disable", "Disattiva"),
    ("f.brightness", "Brightness", "Luminosità"),
    ("f.contrast", "Contrast", "Contrasto"),
    ("f.sharpness", "Sharpness", "Nitidezza"),
    ("f.red-gain", "Red", "Rosso"),
    ("f.green-gain", "Green", "Verde"),
    ("f.blue-gain", "Blue", "Blu"),
    ("f.picture-mode", "Picture mode", "Modalità immagine"),
    (
        "f.black-equalizer",
        "Black Equalizer",
        "Equalizzatore del nero",
    ),
    ("f.color-tone", "Color tone", "Tonalità colore"),
    ("f.eye-saver", "Eye Saver Mode", "Modalità Eye Saver"),
    ("f.input", "Input source", "Sorgente di ingresso"),
    ("f.volume", "Volume", "Volume"),
    ("f.mute", "Audio mute", "Audio muto"),
    ("f.language", "OSD language", "Lingua dell'OSD"),
    (
        "f.adaptive-sync",
        "Adaptive-Sync / FreeSync",
        "Adaptive-Sync / FreeSync",
    ),
    ("f.aim-point", "Virtual Aim Point", "Mirino virtuale"),
    (
        "f.lighting",
        "Infinity Core Lighting",
        "Infinity Core Lighting",
    ),
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
    ("v.a-blue", "Type A, light blue", "Tipo A, azzurro"),
    ("v.a-white", "Type A, white", "Tipo A, bianco"),
    ("v.b-red", "Type B, red", "Tipo B, rosso"),
    ("v.b-blue", "Type B, light blue", "Tipo B, azzurro"),
    ("v.b-white", "Type B, white", "Tipo B, bianco"),
    ("v.reset-position", "Reset position", "Ripristina posizione"),
    ("v.aurora", "Aurora", "Aurora"),
    ("v.aurora-rotation", "Aurora rotation", "Aurora rotante"),
    ("v.comet", "Comet", "Cometa"),
    ("v.static", "Static", "Statico"),
    ("v.rainbow", "Rainbow", "Arcobaleno"),
    ("v.breathing", "Breathing", "Respiro"),
    ("v.main", "Main", "Principale"),
    ("v.channel1", "Sub 1", "Secondario 1"),
    ("v.channel2", "Sub 2", "Secondario 2"),
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
        }
    }

    #[test]
    fn keys_are_unique() {
        for (i, a) in STRINGS.iter().enumerate() {
            assert!(STRINGS[i + 1..].iter().all(|b| a.0 != b.0), "{}", a.0);
        }
    }
}
