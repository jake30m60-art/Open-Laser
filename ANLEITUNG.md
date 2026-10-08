# OpenLaser – Kurzanleitung

Die wichtigsten Abläufe an der Gweike-Anlage, in der Reihenfolge, in der man sie
braucht. Die ausführliche (englische) Anleitung ist [USAGE.md](USAGE.md).

> Sicherheit zuerst: OpenLaser nie gleichzeitig mit Mlaser betreiben. Vor jeder
> Bewegung prüfen, dass der Verfahrweg frei ist. Erste Schnitte immer mit
> **Dry run** und **Umfahren** kontrollieren.

## 1. Starten

- **OpenLaser (Maschine)** auf dem Desktop startet mit der echten Steuerung,
  **OpenLaser Simulator** ohne Maschine – zum Üben und Ausprobieren.
- Oben die drei Schritte: **Teile → Vorbereitung → Maschine**. Daneben der
  Stift (Skizze), die Materialbibliothek und die Einstellungen.
- Sprache: Einstellungen → Allgemein → *Sprache · Language*.
- Wird ein Fenster „… hat die Steuerung“ angezeigt, bedient gerade ein anderer
  Bildschirm (z. B. das Handy). **Bedienung übernehmen** holt sie hierher.

## 2. Teile

- **+ DXF / SVG** importiert Zeichnungen, **Ordner importieren** ganze Ordner,
  **+ Bild** Fotos/Grafiken (PNG, JPEG, BMP, WebP, GIF) zum Graustufen-Gravieren.
- Nach dem Import zeigt eine Prüfung Größe, offene Konturen und Probleme.
  Rot markierte Konturen (kreuzt sich / berührt andere) mit
  **Konturen reparieren…** in den Teile-Details beheben.
- **Zeichnung vereinfachen…** fasst viele kleine Linien (z. B. aus Splines)
  zu Bögen zusammen und speichert das als neues Teil.
- Mehrere Teile in einen Auftrag: **Teile auswählen**, Teile antippen, dann
  **Auftrag vorbereiten**.
- Reiter **Skizzen** zeigt alle gespeicherten Skizzen; der Stift auf einer
  Karte öffnet sie wieder zum Bearbeiten.

## 3. Skizze (eigene Teile zeichnen)

- Werkzeuge links: Linie, Rechteck, Kreis, Bogen, Polygon, Langloch, Text …
  Maße während des Zeichnens eintippen, **Tab** wechselt das Feld, **Enter**
  übernimmt.
- Bemaßen: Bemaßungswerkzeug, dann Punkte, Linien oder Kreise anklicken.
  Kreismittelpunkte leuchten auf, wenn man darüberfährt.
- Blau = noch beweglich, Schwarz = vollständig bestimmt.
- Text bleibt Text: verschieben, Doppelklick zum Ändern (Schrift, Größe),
  am **Eckgriff** ziehen zum Skalieren. Schriften des PCs sind wählbar.
- **Zeichnung als Skizze bearbeiten** (in der Vorbereitung) öffnet ein
  vorhandenes Teil als Skizze; Brücken aus dem Auftrag werden mit übernommen.
- Speichern legt ein neues Teil an oder ersetzt das Teil im Auftrag.

## 4. Vorbereitung

1. **Material wählen** – Rezept nach Material, Dicke und Gas. In der
   Materialbibliothek filtern die Chips nach Herkunft: *Gweike*, *Eigene*,
   *Beispiele*.
2. Bearbeitung oben einstellen: **Anschnitt / Auslauf**, **Mikrostege**,
   **Kühlpunkte**, **Schnittfuge**, **Brücken**; links **Schnittreihenfolge**,
   **Start und Naht**, **Gemeinsame Kanten**, **Flycutting**, **Gravieren**,
   **Nesting**, **Simulation**.
3. Gilt für wen? Im offenen Werkzeug oben wählen:
   - **Auswahl (n)** – nur die gewählten Konturen (Kontur anklicken, nochmal
     klicken wählt sie einzeln). Solche Konturen sind **violett gestrichelt**
     umrandet; beim Kopieren nehmen sie ihre Einstellungen mit.
   - eine **Ebene** oder **Ganzer Auftrag / Alle Ebenen**.
   - Ein Werkzeug auf **Aus** schaltet es überall aus.
4. Anschnitt: innen/außen pro Kontur umschaltbar, Startpunkt im Anschnitt-Menü
   festlegen.
5. Brücken: erste Kante anklicken, zur zweiten fangen, klicken. Jede Brücke ist
   einzeln löschbar.
6. **Gravieren**: Flächen oder Konturen wählen, *Gemeinsam* (ein Scan über alle)
   oder *Einzeln* (Objekt für Objekt). Wer nach der Füllung die Umrisse abfährt,
   kann mit **Innenkonturen auch abfahren** die Löcher auslassen. Das zuletzt gewählte Gravurrezept wird vorgeschlagen.
7. **Simulation** zeigt den Ablauf mit Geschwindigkeit und Leerfahrten.
8. **Auftrag speichern**, wenn man ihn wieder braucht.

## 5. Maschine

1. **Verbinden**, dann **Referenzfahrt**.
2. Blech einlegen, Kopf mit den Pfeilen an den Nullpunkt fahren, **Nullpunkt
   hier setzen** (*Jeder Lauf* = neu bei jedem Lauf, *Absolut* = fester Anschlag).
3. **Umfahren** zeigt die Lage auf dem Blech (Kopf bewegt sich!).
4. Neu oder unsicher: erst **Dry run** (ohne Laser).
5. **Starten** → **Preflight-Checkliste** abhaken → Schnitt läuft.
   **Folgen** hält die Ansicht beim Kopf.
6. **Pause** hält an (Position bleibt), **Fortsetzen** fährt dort weiter.
   **Stopp** beendet den Lauf. **Wiederanlauf anpassen** startet ab einer
   gewählten Stelle neu.
7. Danach **Postflight-Checkliste**; Restblech bei Bedarf als Rest speichern.

Düsenhöhe: Schneiden mit kapazitiver Höhenregelung. Gravieren mit fester Höhe:
Der Kopf tastet am Konturanfang an, hebt um die Schnitthöhe ab und hält sie.

## 6. Material und Gas

- Die 51 Gweike-Parameter sind als „… (Gweike)“ importiert (Notiz: Autor
  Gweike). Bei Gweikes „air“-Dateien ist das **N₂-Ventil** gewählt – wie bei
  den Faser-Schichten aus Mlaser. Geschnitten wird mit dem, was am N₂-Eingang
  hängt. Vor dem Schnitt die Versorgung prüfen.
- Werte im Rezept antippen zum Ändern, **Rezept speichern** übernimmt.

## 7. Sicherung und Update

- **Datensicherung**: Einstellungen → Materialien & Prozesse → *Sicherung
  herunterladen* (eine ZIP mit Teilen, Aufträgen, Rezepten, Skizzen,
  Einstellungen und Maschinensicherung). Zusätzlich legt OpenLaser bei jedem
  Start einmal am Tag eine Kopie unter `data\backups` ab (die letzten 14).
- **Wiederherstellen**: OpenLaser schließen, ZIP entpacken, den Ordner `data`
  über `OpenLaser-App\data` kopieren.
- **Update**: `scripts\OpenLaser-aktualisieren.cmd` im Quellordner doppelklicken.
  Es baut die neue Version, bittet das laufende OpenLaser sich zu schließen,
  legt die alte Version unter `OpenLaser-App\alt\` ab und startet die neue.

## 8. Wenn etwas hakt

- Meldungen oben im Bild: rote bleiben stehen bis **OK**, bei „Details“ steht
  die technische Ursache.
- Nach einem Verbindungsabbruch nie annehmen, dass Position oder Pause noch
  stimmen – erst Zustand an der Maschine prüfen.
- Im Zweifel erst im **Simulator** nachstellen.
