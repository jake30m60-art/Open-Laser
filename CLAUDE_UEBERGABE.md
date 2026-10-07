# Übergabe für Claude – OpenLaser

Stand: 7. Oktober 2026. Diese Datei ist die aktuelle, zusammengefasste Übergabe der Codex-Arbeit. Zuerst diese Datei lesen, dann bei Bedarf ALPHA_PRUEFUNG.md und die konkret betroffenen Dateien. Alte Sitzungsprotokolle nicht erneut vollständig durchlesen. Historische Abschnitte in UEBERGABE.md enthalten inzwischen überholte offene Punkte.

## 1. Projekt, Git und installierte Anwendung

- Projekt: `C:\Users\joe\OpenLaser`, Branch `skizze`.
- Letzter Implementierungscommit: `def904f` – Improve bridge validation and editable sketches; verify alpha workflows.
- Davor: `6933194` (Konturbaum/Auftragsübersicht), `9ad717d` (Gravurauswahl/Simulation), `1558611` (Teilvorschau bei Geometriefehlern/Einzelkonturauswahl).
- Bei Beginn dieser Dokumentation war der Arbeitsbaum sauber. Diese Übergabe wird danach in einem eigenen Dokumentationscommit gesichert.
- Installiert: `C:\Users\joe\OpenLaser-App\OpenLaser.exe`, Desktop-Release mit eingebetteter Oberfläche aus dem Implementierungsstand def904f.
- SHA256: `7BCA0756D3CFDA3ACACC642B5BCD736210CF362B54D2A617B55A9FE46DEB7434`; Build und Installation stimmen überein.
- Gesicherte Vorversion: `C:\Users\joe\OpenLaser-App\OpenLaser-vor-alpha-20261007-1410.exe`.
- Joes laufende Instanzen wurden nicht beendet. Ein Neustart durch Joe ist nötig, um die neue EXE zu laden. Eine offene alte Oberfläche beweist deshalb keinen Fehler des neuen Builds.
- Keine Änderungen auf GitHub hochgeladen. Die Einrichtung/Authentifizierung wurde nicht abgeschlossen.

## 2. Verbindliche Nutzerentscheidungen

- Handrad vorerst ausdrücklich auslassen. Nicht ungefragt mit HID/PHBX/Live-MCUT weitermachen.
- GitHub erst nach stabiler und geprüfter Alpha. Früher erteilte Upload-Wünsche wurden durch diese Einschränkung ersetzt; keinen Upload jetzt starten.
- Bestehende Projekte, DXF und mehrere Skizzen müssen weiter bearbeitbar bleiben.
- Nur ausgewählte Konturen bearbeiten muss bis zur tatsächlich kompilierten Ausgabe gelten.
- Schnittreihenfolge bleibt eigener Tab; Wiedergabe gehört in den Simulationstab.
- Fehler sichtbar markieren und erklären. Ungültige Gesamtaufträge dürfen nicht ausgeführt werden.
- Gezielt arbeiten und Joes Originaldaten bei Tests nicht verändern.

## 3. Was Codex umgesetzt hat

### Gravur, Konturen, Sichtbarkeit und Fehler

- Einzelkonturen lassen sich auswählen; Auswahl eines ganzen Teils ist eine eigene Aktion.
- Gravurflächen werden ausdrücklich durch Außenkontur und Innenlöcher beschrieben (`fill_regions`/`FillRegion`), statt unklar alle Formen auf einer Ebene zu füllen. Ring-/Innenflächen und das innere Loch von Buchstaben werden berücksichtigt.
- Im Gravurtab gibt es die Konturliste zum erneuten Anwählen/Ändern und Entfernen aus der Gravur. Abwahl aktualisiert die Vorschau.
- Schraffur, getönte Flächen und Bahnen mit mehreren Quellkonturen werden dargestellt. Ebenen können für die Anzeige verborgen werden. Anzeige ausblenden und Ausgabe begrenzen sind verschiedene Zustände.
- Ein Fehler in einer anderen Kontur unterdrückt nicht mehr sämtliche gültigen Gravur-/Schnittvorschauen. Fehlerbanner, betroffene Konturmarkierung und Fehlerfokus ergänzt. Der gesamte fehlerhafte Auftrag bleibt gesperrt.
- `only_contours` begrenzt die tatsächliche Vorbereitung und damit die Kompilierung. Auswahl wird im Draft gespeichert; „Alle bearbeiten“ hebt sie auf. Teilweise ausgewählte Brücken werden abgewiesen.
- Der Run-Tab zeigt die Begrenzung für den aktuellen Draft an, ohne sie fälschlich einer anderen ausgeführten/wiederhergestellten Arbeit zuzuordnen.

Wichtige Dateien: `crates/openlaser-core/src/features.rs`, `crates/openlaser-prep/src/lib.rs`, `crates/openlaser-server/src/draft.rs`, `ui/src/routes/setup/Canvas.svelte`, `EngravePanel.svelte`, `ui/src/lib/engraving-preview.ts`, `geometry-issues.ts`, `viewer-geometry.ts`, `ui/src/stores/ui.svelte.ts`.

### Simulation, Schnittreihenfolge und Auftragsübersicht

- Eigener Simulationstab mit Schnitt/Gravur/Alles, Wiedergabe, Tempo, Fortschritt, Pfadnavigation, Leerfahrtenfilter und anklickbarer Bearbeitungsreihenfolge.
- Geometrische Wiedergabe verteilt Abspielzeit auf Ebenen. Ein kurzer Schnitt nach sehr langer Gravur ist sichtbar. Das ist keine Maschinenzeitberechnung.
- Wiedergabe und Schieberegler aus dem Schnittreihenfolgetab entfernt.
- Konturbaum nach Ebenen, Nummern, Teil/Kopie, Innen-/Außenkontur, Suchfeld und Konturfokus.
- Drag-and-drop innerhalb einer Ebene, Hoch/Runter und Alt+Pfeil speichern die manuelle Reihenfolge. Zusammengeführte Konturen bleiben eine Einheit; Gravurabschnitte werden nicht mehrfach gelistet.
- Manuelles Verschieben deaktiviert automatische Innen-zuerst/Kreise-zuerst/Wärmeverteilung. „Innen nach außen“ kann bewusst wieder aktiviert werden. Revision vom Dragstart wird beim Drop geprüft.
- Auftragsübersicht mit Teilen, Schnitt/Gravur/Ausschlüssen, Ebenen und Ausgabestatus bleibt bei den Vorbereitungswerkzeugen sichtbar. Ebenenreihenfolge ist mit Pfeilen änderbar.
- Wichtiger nachträglich gefundener Fehler: 6933194 hatte JobOverview nur importiert, nicht eingebunden. def904f bindet die Komponente tatsächlich ein; grafisch geprüft.

Wichtige Dateien: `ui/src/routes/Setup.svelte`, `setup/SimulationPanel.svelte`, `setup/JobOverview.svelte`, `setup/features/OrderForm.svelte`, `ui/src/lib/cut-order.ts`.

### Brücken

- Kanalbreite/Position im Auswahlzustand sichtbar; zweite Auswahl lässt die tatsächliche Geometrie auf einer Kopie prüfen. Übernehmen speichert, Abbrechen verwirft.
- „Brücke hinzufügen“ funktioniert auch bei ausgeschaltetem Brückenwerkzeug.
- Bestehende Brücken werden einzeln mit Nummer/Koordinaten und Fehlernummer angezeigt.
- Entfernen einer Brücke entfernt auch nachfolgende Brücken: deren Indizes beziehen sich auf die durch vorherige Brücken veränderte Konturliste und können sonst falsch werden.
- Letzte Brücke lässt sich auch im Vorschauzustand zurücknehmen. Während der Neuberechnung sind weitere Picks/Übernehmen gesperrt.
- Brücken zwischen unterschiedlichen Bearbeitungsebenen werden klar abgewiesen. Damit kann eine Gravur nicht versehentlich in die Bearbeitungsart einer Schnittkontur übergehen.
- Regressionstests für rechteckige Innenlöcher bei beiden Umlaufrichtungen und konzentrische Kreise sowie verschiedene Bearbeitungsebenen.

Wichtige Dateien: `crates/openlaser-prep/src/bridge.rs`, `ui/src/routes/setup/features/BridgesForm.svelte`, `FeaturePanel.svelte`, `PickBar.svelte`, `Canvas.svelte`, `ui/src/lib/bridge-preview.ts`.

### Skizzen, vorhandene DXF/SVG und Text

- Neues Projekt/Neue Skizze und Bearbeiten vorhandener Zeichnungen erreichbar.
- Quelllinien/Bögen und Ebenen werden übernommen, nicht abgeflachte Vorschau oder kompensierter Werkzeugweg. Unabhängige Konturen werden nicht durch zufällig identische Endpunkte verbunden.
- Mehrere benannte Skizzendokumente statt nur einer Skizze in localStorage. Wechsel erhält Geometrie und Abhängigkeiten; Werkzeugzustand/Undo-Historie werden beim Wechsel neu begonnen.
- Werkzeuggruppen Erstellen, Ändern, Abhängigkeiten und Bemaßung getrennt.
- Text mit vorhandener Schriftwahl/Umrissvorschau und DXF/SVG können in eine laufende Skizze eingefügt werden. Importierte Geometrie bekommt kollisionsfreie IDs und ist als ein Schritt rückgängig zu machen.
- Bibliotheksteil speichert optional das vollständige Skizzenmodell (`Part.sketch` als JSON), einschließlich Abhängigkeiten und Bemaßungen. Alte Bibliotheken bleiben lesbar.
- Beim Bearbeiten eines Teils im aktuellen Auftrag wird dessen Referenz durch das gespeicherte Teil ersetzt; andere Teile bleiben im Mehrteilauftrag. Originalteile bleiben in der Bibliothek.
- Kopien/Transformationen werden übernommen. Einstellungen unveränderter Konturen werden geometrisch zugeordnet; Einstellungen geänderter Konturen müssen geprüft werden. Brücken werden bei tatsächlichem Geometrieersatz zurückgesetzt.
- Undo/Redo des Auftrags berücksichtigt alte/neue Quelldaten. Gleicher unveränderter Import kann vom bestehenden Import-Deduplizieren als dasselbe Teil erkannt werden; dann erfolgt kein Geometrieersatz.
- Ersetzen prüft Auftragsbindung und Revision; veraltete Bearbeitung darf nicht einen anderen Auftrag ersetzen.

Neue APIs: `GET /api/parts/{id}/drawing` liefert Name/Zeichnung/Skizze; `POST /api/parts/{id}/sketch` hält das Skizzenmodell; `POST /api/draft/replace-part?revision=...` erwartet `{old,new}`.

Wichtige Dateien: `crates/openlaser-library/src/lib.rs`, `crates/openlaser-server/src/api.rs`, `job_parts.rs`, `draft.rs`, `ui/src/lib/sketch-workflow.ts`, `ui/src/routes/Sketch.svelte`, `ui/src/sketch/store.svelte.ts`, `documents.ts`, `append-drawing.ts`, `from-drawing.ts`, `dxf.ts`, `ui/src/components/CreateText.svelte`.

### Run-Darstellung und manueller Trennschnitt

- Laserfreie Scan-Anlauf-/Auslauffahrten werden vom Compiler als Travel statt Joint eingeordnet. Test beweist unveränderte Maschinenrecords. Das ist eine Darstellungsberichtigung.
- Vorhandener vorbereiteter manueller Trennschnitt über die Auftragsübersicht zugänglich gemacht. Nicht mit dem noch fehlenden Mlaser-Live-MCUT oder Handrad verwechseln.
- Schutzregel gegen X/Y-Verfahren bei gesenktem Kopf nicht aufgehoben.

Wichtige Dateien: `crates/openlaser-compiler/src/program.rs`, `ui/src/routes/Setup.svelte`, `setup/JobOverview.svelte`; bestehende `ManualCut.svelte`/`manual_cut.rs`.

## 4. Aktuelle Probleme in Joes Originaldaten

1. Aktueller Auftrag ist „Sketch“ mit zwei Konturen, nicht mehr das OPH-Logo. Zwei gespeicherte Brücken sind problematisch: erste verbindet Gravur- und Schnittebene; zweite ist etwa 1 mm daneben, bei 2 mm Kanalbreite. Nicht automatisch still repariert. Joe muss beide Enden auf dieselbe gewünschte Schnittebene legen und ausreichend Abstand halten beziehungsweise die ungültigen Brücken neu setzen.
2. Positiver Gegenversuch ausschließlich in Diagnosekopie: dieselben zwei Konturen, eine Brücke, beide auf Schnittebene → eine geschlossene vorbereitete Kontur mit beiden Quellen, kein Fehler. Grafisch ein korrekter Schablonenkanal zwischen Außenkontur und Innenloch.
3. OPH Logo Schablone hat weiterhin `contour 41: crosses itself` (Backend-Index, nicht ungeprüft mit UI-Nummer gleichsetzen). Ebene 0, vier Bögen und zwei mikroskopisch kurze Linien ungefähr X=35/Y=-54 mm. Nicht geometrisch korrigiert. Vor Ausführung dieses Auftrags den Import/Umriss gezielt untersuchen.
4. Kein Test hat Joes Originalauftrag, Original-DXF oder echte Maschinenkonfiguration verändert. Keine reale Maschine ausgeführt.

## 5. Prüfungen und Nachweise

- Vollständiger Rust-Workspace: `cargo test --workspace --locked -- --skip imported_recipes_and_machine_files` bestanden.
- Ausnahme: bereits vorher dokumentierter CRLF-Fixture-Test `imported_recipes_and_machine_files` ausgelassen; Ursache nicht behoben.
- `restored_library_compiles_and_executes` ignoriert: benötigt ausdrücklich isolierte Offline-Fixtures und OPENLASER_TEST_LIBRARY/BACKUP/REPORT. Nicht als bestanden ausgeben.
- Nach letzter Backend-Korrektur: 50 Prep- und 77 Server-Lib-Tests bestanden. Compiler: 102 Tests bestanden.
- Neuer HTTP-Integrationstest `sketches_and_replaced_job_parts_survive_the_http_boundary`: Modell/Abhängigkeiten speichern/lesen, Mehrteilauftrag ersetzen, Undo und ungültige Daten abweisen bestanden.
- UI: 245 Tests in 54 Dateien; `npm run check` ohne Fehler/Warnungen; Produktionsbuild erfolgreich. Bekannte Vite-Hinweise zu PlaneGCS und großem Materialmodul bleiben.
- Matrix deckt unter anderem Gravurlöcher, Auswahl, Brücken, gemeinsame Kanten, Nesting, Kopieren, Reihenfolge, simulierte Controllerabläufe und Sicherheitsablehnungen ab. Das ist keine vollständige grafische oder reale Maschinenabnahme jeder Kombination.
- Grafische Prüfung in eigener Bibliothekskopie auf Simulator-Port 8097: Auftragsübersicht, DXF im Editor, Werkzeuggruppen, Schriftvorschau, Textimport, Ersetzen und Undo.
- Textimport: 985 neue Kurven; Auftrag nach Übernahme 16 statt 2 Konturen. Undo stellte ursprüngliche Konturen/Ebenen/Brücken her.
- Finalen Desktop-Release in eigener Kopie neu gestartet: gespeichertes Skizzenmodell mit 993 Kurven wieder lesbar; ebenenübergreifende Brücke abgewiesen; nur Quelle 1 liefert genau einen Vorschauschnitt ohne Revision zu verändern.
- Schnittsimulation im finalen Release grafisch bis 100 % geprüft, Bearbeitungsreihenfolge sichtbar.
- Frühere isolierte Kompilierungsprüfung: nur Kontur 1 → ein Schnittdurchgang, vier Moves, keine Gravur, error=null. Auswahl wirkt somit auch vor Maschinenkompilierung.
- Eigener Simulator beendet; Joes Instanzen nicht beendet.
- Bildnachweise: `C:\Users\joe\Videos\openlaser-alpha-pruefung-20261007\bruecke-geprueft.png` und `simulation-geprueft.png`.
- Details: `ALPHA_PRUEFUNG.md` im Repository.

## 6. Was fehlt / nächster sinnvoller Arbeitsplan

### Zuerst: offene Fehler und Alpha-Abnahme

- OPH-Kontur 41 geometrisch untersuchen und korrigieren; keine automatische breit wirkende Importreparatur ohne gezielten Regressionstest.
- Originalauftrag mit Joe neu prüfen, nachdem er die neue EXE gestartet und seine ungültigen Brücken neu gesetzt hat. Bisherige positive Brückenprüfung lief in Kopie.
- CRLF-Fixture-Test bereinigen; den ignorierten Bibliotheks-Akzeptanztest auf expliziter Offline-Kopie ausführen und Ergebnisse dokumentieren.
- Weitere grafische Kombinationstests: Gravur/Schnitt/Ebenenabwahl/Auswahlbegrenzung im Run, Konturbaum-Drag-and-drop, gemeinsame Kanten, Nesting und Kopien. Backendtests vorhanden; nicht jeden Pfad erneut grafisch geprüft.
- Echte Maschinenabnahme mit Joe steht vollständig aus: Verbindung, Achsen/Homing, Folgen/Kopf, Fokus/Gas, Freigaben und Materialversuche. Nur in abgestimmter Reihenfolge; nicht eigenständig echte Bewegung/Laser starten. Alpha nicht als produktionsfreigegeben deklarieren.

### Funktionale Grenzen / weitere Verbesserungen

- Teilgeometrie nach Nesting oder Mehrblattzuordnung ersetzen: aktuell ausdrücklich abgewiesen. Sicheres Aktualisieren von Nesting/Blättern noch nicht umgesetzt.
- Skizzenarchiv wird browserlokal gehalten, nicht als gemeinsames serverseitiges Dokumentarchiv synchronisiert. Bibliotheksteil enthält das übernommene Modell; Entwürfe anderer Browser erscheinen nicht automatisch.
- Text wird als bearbeitbare Kurven eingefügt; keine nachträgliche parametrische Text-/Schriftbearbeitung als Textobjekt implementiert.
- Importierte offene Umrisse haben kein neu ergänztes automatisches „alle schließen“-Werkzeug; vorhandene Skizzenwerkzeuge verwenden. Anforderungen dafür gezielt mit realer Problemdatei überprüfen.
- Geänderte Konturen können Einstellungen verlieren; keine garantierte semantische Zuordnung beliebig stark veränderter Konturen. Hinweise und Undo vorhanden, Einstellungen kontrollieren.
- Gespeichertes Skizzenmodell wird derzeit nur auf grundlegende Dokumentstruktur geprüft. Tiefere Schema-/Referenzvalidierung wäre eine weitere Robustheitsaufgabe.
- Englische Reste in der deutschen UI beseitigen; auch neu hinzugefügte Meldungen konsequent über die Übersetzung führen. Bisher keine vollständige Sprachbereinigung.
- Echte Gweike-1200-W-Schneidparameter aus `C:\Users\joe\Desktop\gweike laser\C260411-0695-0807\C260411-0695-0807\Cutting parameters` sind laut bisheriger Übergabe noch nicht importiert/abgenommen. Parameterbericht: `C:\Users\joe\Videos\OpenLaser-Schneidparameter-Pruefung.md`. Vor Nutzung aktuellen Dateistand verifizieren.
- Mlaser-artiges Live-MCUT bis zum Blechende ist nicht umgesetzt; vorhandener endlicher vorbereiteter Trennschnitt ist etwas anderes. Handrad ausdrücklich zurückgestellt.
- Kamera-Nesting erst später nach belastbaren Grundfunktionen an der Maschine.
- GitHub-Repository/Authentifizierung/Upload erst nach stabiler geprüfter Alpha und erneuter Freigabe. Nicht als erledigt melden.

## 7. Praktische Arbeitsweise für die Fortsetzung

1. `git status` und letzten Commit prüfen; fremde Änderungen erhalten. Nur relevante Dateien lesen. Diese Übersicht hat Vorrang vor veralteten Statusangaben in frühen UEBERGABE-Abschnitten.
2. Tests nur gegen temporäre Verzeichnisse beziehungsweise eine eigene Bibliothekskopie. Joe verwendet 8080/8081; eigenen freien Port wählen. Immer `--simulate --no-browser` für Tests. Niemals seine Instanzen beenden.
3. UI prüfen: `cd ui`, `npm run check`, `npm test`, `npm run build`. Backendtests passend zur Änderung; vollständige Matrix bei erneuter Alpha-Abnahme.
4. Desktop bauen: im Projektroot `cargo build --release -p openlaser --features desktop --locked`. Ein Build ohne desktop ist nicht die installierbare Windows-Oberfläche. UI vorher fertig bauen.
5. Bestehende EXE vor Installation sichern/bei Sperre umbenennen und neue daneben kopieren. Hash prüfen; Joe zum Neustart auffordern. Keine automatische Beendigung seines Prozesses.
6. Bei Änderungen an generierten TS-Typen bestehende Binding-Anleitung in UEBERGABE.md beachten. CRLF erzeugt unter Windows falsche Änderungsmeldungen; echte Diffs prüfen.
7. Neue UI-Texte über `t('English')` und de.json. Prüfberichte unterscheiden Tests, grafische Nachweise und Maschinenabnahme.
8. Markdown wird überwiegend durch `.gitignore` ignoriert. Diese Datei und ALPHA_PRUEFUNG.md sind bewusst mit `git add -f` aufgenommen. Keine privaten Mitschnitte/Fixture-Dateien versehentlich committen.
9. Git-Autor war nur pro Commit gesetzt; keine globale Konfiguration verändert. Bei Bedarf bestehende lokale Commit-Identität prüfen, statt eine neue E-Mail zu erfinden.