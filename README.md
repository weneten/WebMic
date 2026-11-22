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

## Native iPhone-App mit Hintergrundaufnahme

Die native App nutzt denselben Rust-Server und denselben HTTPS-Link wie Safari. Die Aufnahme ist für den App-Wechsel und den Sperrbildschirm konfiguriert. Ein Anruf oder ein Wechsel des Audiogeräts kann sie unterbrechen. Zum Fortsetzen startest du das Mikrofon erneut. Die App benötigt iOS 17 oder neuer.

### IPA laden

1. Öffne [Actions → WebMic iOS](https://github.com/weneten/WebMic/actions/workflows/ios.yml). Der Workflow startet bei Änderungen am iPhone-Client auf `main`. Mit **Run workflow** kannst du ihn auch manuell starten.
2. Öffne den erfolgreichen Lauf und klicke unter **Artifacts** auf **WebMic-iOS**. Melde dich dafür gegebenenfalls bei GitHub an. Entpacke das heruntergeladene ZIP. Darin liegt `WebMic.ipa`. GitHub bewahrt das Artefakt einen Tag lang auf.
3. Die IPA ist absichtlich unsigniert. GitHub benötigt deshalb weder ein Apple-Konto noch ein Entwicklerzertifikat.

### SideStore auf dem iPhone einrichten

1. Installiere auf CachyOS `usbmuxd` mit `sudo pacman -S --needed usbmuxd`. Lade das Linux-AppImage von [iloader](https://iloader.app/), mache es ausführbar und starte es.
2. Installiere **LocalDevVPN** auf dem iPhone und erlaube das Anlegen des VPN-Profils. Verbinde das entsperrte iPhone per USB mit dem PC und bestätige **Diesem Computer vertrauen**.
3. Melde dich in iloader mit deinem Apple-Konto an, wähle das iPhone und klicke auf **Install SideStore (Stable)**. Folge der [SideStore-Anleitung](https://docs.sidestore.io/docs/installation/install.html), um deinem Entwicklerprofil zu vertrauen und den Entwicklermodus unter **Datenschutz & Sicherheit** einzuschalten.
4. Verbinde LocalDevVPN im WLAN. Öffne SideStore und melde dich mit demselben Apple-Konto an. Tippe unter **My Apps** auf die Tagesanzeige bei SideStore, um einmal manuell zu verlängern.
5. Übertrage `WebMic.ipa` in die Dateien-App, z. B. über eine lokale Freigabe. Tippe in **SideStore → My Apps** auf **+** und wähle `WebMic.ipa` aus. SideStore signiert und installiert die App.

### Signatur verlängern

Ein kostenloses Apple-Konto signiert Apps für sieben Tage. SideStore kann die Signatur im Hintergrund erneuern, wenn WLAN und LocalDevVPN verfügbar sind. Unter **My Apps** siehst du die verbleibenden Tage und kannst per Tipp auf den Zähler sofort verlängern. Die iOS-Kurzbefehlsaktion **Refresh All Apps** lässt sich gegebenenfalls täglich ausführen. Prüfe nach dem ersten automatischen Lauf den Zähler, weil iOS den Hintergrundlauf überspringen kann und es Berichte über fehlschlagende Kurzbefehle gibt.

### Mit CachyOS verbinden und testen

1. Starte auf dem PC `cargo run --release --locked`. Falls noch nicht eingerichtet, installiere das unter **iPhone verbinden** beschriebene WebMic-Zertifikat.
2. Füge den vollständigen Terminal-Link samt dem Teil hinter `#` in die native App ein. Tippe auf **Mikrofon starten** und erlaube den Mikrofon- und lokalen Netzwerkzugriff.
3. Wähle am PC **WebMic** als Eingang. Wenn der Pegel reagiert, sperre das iPhone und sprich weiter. Prüfe auch den Wechsel zu einer anderen App.
