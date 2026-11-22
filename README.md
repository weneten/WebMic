# WebMic

Das iPhone dient über Safari als Mikrofon für einen Linux-PC im selben lokalen Netz. Der Rust-Server nimmt den Audiostream über HTTPS entgegen und stellt ihn über PipeWire als Eingang **WebMic** bereit. Auf dem iPhone genügt der Browser.

## Auf CachyOS starten

Du brauchst Rust, PipeWire und `pipewire-pulse`. Die benötigten Programme `pactl` und `pw-cat` findest du bei Bedarf über `pacman -F pactl pw-cat`.

```sh
cargo run --release --locked
```

Bei mehreren Netzwerkadaptern kannst du die zum iPhone erreichbare IP und optional den Port angeben. Ohne Portangabe verwendet WebMic TCP 8443.

```sh
cargo run --release --locked -- 192.168.178.42
```

## iPhone verbinden

1. Beim ersten Start erzeugt WebMic das öffentliche Zertifikat `.webmic/webmic-ca.cer`. Übertrage **nur diese Datei** aufs iPhone, z. B. über die Dateien-App und eine SMB-Freigabe oder als Anhang einer E-Mail an dich selbst. Der private Schlüssel `.webmic/ca-key.pem` bleibt auf dem PC.
2. Öffne die CER-Datei auf dem iPhone und installiere das geladene Profil unter **Einstellungen → Allgemein → VPN und Geräteverwaltung**. Aktiviere anschließend unter **Einstellungen → Allgemein → Info → Zertifikatsvertrauenseinstellungen** das volle Vertrauen für **WebMic Local CA**. Dieser Schritt ist einmal pro Zertifikat nötig.
3. Öffne den QR-Code oder den vollständigen Link aus dem Terminal in Safari. Tippe auf **Mikrofon starten** und erlaube den Mikrofonzugriff.
4. Wähle am PC in **KDE-Systemeinstellungen → Audio** oder in der Zielanwendung **WebMic** als Eingang. Lass Safari auf dem iPhone im Vordergrund und das Display an.

PC und iPhone müssen sich im selben erreichbaren Netz befinden. Der PC kann per Ethernet verbunden sein. Falls Safari die Seite nicht erreicht, prüfe die im Terminal angezeigte IP, die Firewall am PC und ob das iPhone in einem isolierten Gast-WLAN steckt.

Bei jedem Start gibt es einen neuen Verbindungslink. Ein Gerät kann gleichzeitig senden. WebMic speichert kein Audio. **Strg+C** beendet den Server und entfernt den temporären PipeWire-Eingang. Das iPhone-Zertifikatsprofil kannst du später unter **VPN und Geräteverwaltung** entfernen.
