# Übergabe – OpenLaser-Fork von Joe

> Aktueller Gesamtstand und offene Aufgaben: zuerst CLAUDE_UEBERGABE.md lesen (7. Oktober 2026, Implementierungscommit def904f). Die folgenden frühen Statusangaben sind historisch; die letzte Fortsetzung und ALPHA_PRUEFUNG.md enthalten den aktuellen Prüfstand.

Für die nächste Sitzung (ChatGPT/Codex oder Claude). Branch `skizze` in `C:\Users\joe\OpenLaser`.
Installiertes Programm: `C:\Users\joe\OpenLaser-App\OpenLaser.exe` (Stand e0756c5, 10:18).
Nichts davon wurde an der echten Maschine getestet – nur im Simulator.

## Arbeitsweise (bitte einhalten)

- Vor eigener Arbeit `git status` prüfen. Fremde, uncommittete Änderungen erst sichern/committen.
- Prüfen: `cd ui; npm run check; npx vitest run` und `cargo test -p openlaser-server -p openlaser-prep -p openlaser-controller`.
  Bekannt rot, nicht von uns: `imported_recipes_and_machine_files` (CRLF).
- Bindings: `cargo test -p openlaser-server --features typescript --lib export_bindings`, danach
  Leerzeichen am Zeilenende entfernen, unveränderte Bindings zurücksetzen, `npm run bindings`.
  `node ui/scripts/check-bindings.mjs` meldet unter Windows alles als geändert (CRLF) – nur echte Diffs zählen.
- Bauen: `cd ui; npm run build`, dann `cargo build --release -p openlaser --features desktop --locked`,
  `target\release\openlaser.exe` nach `C:\Users\joe\OpenLaser-App\OpenLaser.exe` kopieren
  (ist die Datei gesperrt: laufende Datei umbenennen, neue daneben kopieren).
- Texte: `t('English')` aus `ui/src/i18n/index.ts`, Deutsch in `ui/src/i18n/de.json`.
- Test-Simulator: `openlaser.exe --simulate --config <eigene machine.toml> --no-browser` auf eigenem Port
  (nicht 8080/8081, die gehören Joe). Joes laufende OpenLaser-Instanzen nie beenden.

## Zuletzt erledigt (Commits 2c3889a, 0d1ce00, e0756c5)

- Werkzeug „Gravieren“ im Einrichten (`ui/src/routes/setup/EngravePanel.svelte`, `FillSettings.svelte`):
  Auswahl → Ebene „Engraving“ (Markieren + Füllung), Rezept, Linienabstand, Winkel, Überlauf, Durchgänge,
  Hinweis auf nötigen Anlaufweg und Umkehrspiel aus der Kalibrierung, „nicht mehr gravieren“.
- Schraffur auf der Zeichnung ausgedünnt je nach Zoom (`lib/engraving.ts`), Anlauf gestrichelt,
  Fläche getönt; Bahnen aus mehreren Formen (Füllung, Flugschnitt, gemeinsame Kanten) als eigene,
  nicht anklickbare Ebene (`Canvas.svelte`, `shared`).
- Richtungspfeile drehen mit der Schnittrichtung; Flugschnitt lässt Markier-Ebenen in Ruhe (`prep/flying.rs`).
- Abspielen der Schnittreihenfolge färbt Gefahrenes grün (`lib/cut-order.ts`).
- Folgen-Knopf im Ausführen-Tab (`RunSide.svelte`): halten → Kopf auf Schnitthöhe, folgt (Kopfbefehl 104,
  Höhe/Geschwindigkeit wie Schnitt-Folgebefehl, `coordinator.rs::follow_action`); folgt er,
  „Kopf hoch“ → Rückzug auf Stopp-Sicherheitshöhe. Controller: `operations/head.rs`. Test `tests/follow.rs`.
- Druck 0,85 → 8,5 bar in 16 Rezepten; Gravur-Rezept „Stainless Steel“ aus Mlaser-Ebene 7;
  Mlaser-Umkehrspiel-Tabellen (Faser/CO2) in `data\scan_offsets.json` übernommen.

## Offen – dringend (Joes letzte Meldung)

1. **Gravur: keine Schraffur/Formen sichtbar, Buchstaben werden nicht gefüllt.**
   Der Server berechnet die Füllung für Text korrekt (per API geprüft: Füllkontur mit Quellen aller
   Buchstaben + geschlossene Umrisse, auch ohne Ebenen-Rezept, kein Fehler). Das Problem liegt also
   in der Darstellung in Joes Oberfläche oder an seinem Auftrag. Konnte nicht visuell prüfen
   (Browserfenster verdeckt). Prüfen:
   - Ist die Ebene über das Auge ausgeblendet (`ui.hiddenDrawingLayers`, localStorage)?
   - `Canvas.svelte`: `shapes` (Konturen mit genau einer Quelle) und `shared` (mehrere Quellen);
     `shownPaths()`/`thinFill()`; Ebenen-Filter `hidden()`; `filled`-Map aus `draft.features.layers`.
   - Läuft bei Joe wirklich die neueste exe (nach Installation neu starten)?
   - Am besten Joes Auftrag (Teile-Bibliothek in `OpenLaser-App\data`) im Simulator öffnen und ansehen.
2. **Teile-Seite: „Neues Teil“ / „Bearbeiten“ → Skizze.** Neue Teile im Skizzenmodus anlegen,
   DXF/SVG in der Skizze importieren und bearbeiten (offene Umrisse schließen!), speichern → Teil in der
   Liste; Knopf „Bearbeiten“ öffnet ein Teil wieder in der Skizze (mit Abhängigkeiten, wenn selbst
   gezeichnet). Mehrere Skizzen statt einer im localStorage (`ui/src/sketch/store.svelte.ts`).
3. **Skizze:** Text-Werkzeug mit Schriften (Server hat Schriften, siehe `CreateText.svelte`,
   `/api/text/preview`); Werkzeugleiste übersichtlicher, Erstellen / Ändern / Abhängigkeiten+Bemaßung
   klar getrennt.
4. **Manual Cut wie Mlaser** (der Software-Entwurf `ManualCut.svelte`/`manual_cut.rs` ist aus dem
   Einrichten ausgeblendet): hinfahren → MCUT → Bestätigungsfenster → Laser scharf → Pfeiltaste am
   Handrad startet Einstechen + Schneiden in Pfeilrichtung mit den Parametern der gewählten Ebene, bis
   kein Blech mehr unter dem Kopf ist (Höhenregler verliert das Blech), dann kurz nachlaufen und hoch.
5. **Handrad XHC PHB02** (Funk, USB-Empfänger am PC): Mlaser nutzt `PHBX.dll` (32-bit, Hersteller
   VID 0x10CE, PIDs 0xEB61–0xEBAA, Exporte `XOpen`, `XGetInput`, `SetGetKeyCallbackFunction` …).
   In OpenLaser per USB-HID (z. B. hidapi) nachbauen; dafür muss der Empfänger stecken, um Tastencodes
   mitzuschneiden. Im Normalbetrieb soll das Handrad immer steuern können (Verfahren, Folgen, Kopf hoch);
   nur Laser zünden (Manual Cut) braucht Scharfschalten in der Software.
6. **Ausführen-Tab:** Anlauf-/Überlauffahrten werden dort vermutlich noch als Mikrosteg (magenta)
   gezeichnet (Compiler-Abschnitte `Kind::Joint`), sollten wie im Einrichten gestrichelt sein.
   Verfahren in X/Y ist bei gesenktem Kopf gesperrt (Schutzregel in `operations/motion.rs`) – so lassen,
   außer Joe will es anders.

## Offen – später

- Schneidparameter: Gweikes echte 1200-W-Dateien liegen in
  `C:\Users\joe\Desktop\gweike laser\C260411-0695-0807\C260411-0695-0807\Cutting parameters` (N2, Luft, CO2)
  und sind noch nicht importiert; Befund zur mitgelieferten Bibliothek: `C:\Users\joe\Videos\OpenLaser-Schneidparameter-Pruefung.md`
  (Geschwindigkeiten eher für 1,5–2 kW, Schneidhöhe 0,3 mm knapp, Fokus-Bezüge gemischt).
- Englische Reste in der deutschen Oberfläche (z. B. „Save job“, „Dry run“, „Go to Run →“, „Machining“,
  „Preflight“, „est. time“, „pierces“, „Enter positions (%)…“, „Nozzle“, „Pierce“).
- „Gemeinsame Kanten“, „Teile nesten“, „Auftrag kopieren“ nach den letzten Änderungen nicht erneut getestet.
- Kamera-Nesting (4K-Kamera) – erst wenn die Grundfunktionen an der Maschine laufen.
- Erste echte Verbindung zur Maschine: Mlaser vorher schließen, Not-Halt in Reichweite;
  Laser-Freigabe braucht ggf. Gweike-Code (Mlaser-Lizenz abgelaufen 2026-09-06).


## Fortsetzung Codex – 2026-10-07: Gravurflächen fertiggestellt

Die zuvor dringend offene Gravurdarstellung ist bearbeitet und die aktualisierte
OpenLaser-App/OpenLaser.exe installiert (vorherige EXE als OpenLaser-vor-gravurflaechen-*.exe gesichert).

- Gravurpanel: explizit Nur Konturen / Fläche füllen / Füllung + Konturen;
  Auswahl sichtbar oben, Rezeptauswahl eingeklappt, Berechnungsstatus und Änderungsbestätigung.
  Rezeptwechsel und Hinzufügen von Formen schalten Konturgravur nicht ungefragt auf Füllung um.
- Vorschau: Flächen bleiben unabhängig von sichtbaren Leerfahrten getönt, Löcher in Buchstaben
  bleiben frei. Multipass und Kreuzschraffur zeigen ihre Laserbahnen; Fahrten separat zuschaltbar.
- Neue Preview.engraving_regions bewahrt originale geschlossene Formen auch bei outline=false.
  Die Flächenkonturen sind rein zur Anzeige/Auswahl; sie fügen keine Laserbewegung hinzu.
  Canvas ergänzt fehlende auswählbare Konturen daraus, Bearbeitung bleibt möglich.
- Compiled und CompiledSummary enthalten engraving_regions, engraving_paths, engraving_passes.
  Run zeigt für Füllgravuren nur getönte Flächen und lesbare Schraffur, keine zugehörigen Leerfahrten.
  Start/Fortsetzen übernehmen die Anzeigegeometrie aus dem unveränderlichen kompilierten Auftrag.
- Änderungen sind nachträglich möglich und rückgängig zu machen; gespeicherte Features enthalten
  weiterhin Winkel, spacing, passes, pass_angle, outline usw. Keine Änderung des Laser-Compilers.
- Prüfung: UI-Typprüfung fehlerfrei, 227 UI-Tests bestanden. 72 Server-Lib-Tests bestanden,
  danach zusätzlicher Regressionstest für fill-only, Innenloch, 2 Passrichtungen, Feature-Serialisierung
  und Rückwechsel zur Konturgravur bestanden. 6 scan_coupon-, 1 simulation_batch- und
  9 workflow-Tests bestanden (bekannten CRLF-Test explizit ausgelassen).
- Browser mit isolierter Kopie von Claudes Testauftrag Gravur ABO: outline=false behält 10
  auswählbare Umrisse; Kreuzschraffur sichtbar, Run ohne travel-Pfade; Umschalten auf Konturen
  und Undo inklusive Multipass geprüft. Screenshots und Sicherung von Claudes ursprünglichem
  Diff: C:/Users/joe/Videos/openlaser-gravur-uebernahme/.
- Nur Simulator geprüft, keine Verbindung oder Ausführung an der echten Maschine.

Die übrigen Aufgaben (Skizzenteile, Handrad, Manual Cut usw.) bleiben offen.

## Fortsetzung Codex – 2026-10-07: fehlende Gravurvorschau und Einzelauswahl

Joes Meldung am aktuellen Auftrag OPH Logo Schablone nachvollzogen:
Die Vorbereitung scheitert an `contour 41: crosses itself`. Die bisherige Fehleransicht
zeigte nur rohe Umrisse und verlor dabei auch die im Auftrag zugewiesenen Ebenen.
Im Gravurpanel wurde der eigentliche Fehler durch den Hinweis auf das fehlende
Auftragsrezept verdeckt. Die Angabe 12 Formen war der Ebeneninhalt, nicht die Auswahl.

- Gravurkontext: ein Klick nimmt genau eine Kontur, Umschalt-Klick nimmt weitere
  hinzu bzw. entfernt sie; auch das Auswahlrechteck arbeitet dort auf Konturen.
- Aktuelle Auswahl und Anzahl auf der gesamten Ebene sind getrennt beschriftet.
- Geometriefehler werden unabhängig vom Auftragsrezept angezeigt. Ein eigenes
  Ebenenrezept erlaubt bereits die Gravurvorschau ohne globales Auftragsrezept.
- Bei fehlgeschlagener Auftragsvorbereitung bleiben Ebenenzuweisungen erhalten.
  Jede gültige gefüllte Markierebene erhält eine getrennt berechnete Vorschau
  mit Flächen und tatsächlichen Laser-/Leerfahrtpfaden. Fehlerhafte Gravurebenen
  erhalten keine erfundene Füllung. `prepared` bleibt None, der ursprüngliche
  Geometriefehler sperrt weiterhin Kompilierung und Ausführung des Gesamtauftrags.
- Joes Geometrie wurde nicht geändert oder aus der Ausgabe entfernt. Nur in der
  isolierten Diagnosekopie wurde Kontur 41 vorübergehend deaktiviert, um die Ursache
  nachzuweisen. Anschließend wurde die unveränderte aktuelle Auftragskopie geprüft.
- Prüfung: 227 UI-Tests bestanden, UI-Typprüfung fehlerfrei, UI-Build erfolgreich.
  Vollständige Server/Prep/Controller-Testprüfung erfolgreich (bekannten CRLF-Test
  imported_recipes_and_machine_files ausgelassen). Nach Ergänzung der Teilvorschau
  alle 74 Server-Lib-Tests erneut bestanden, einschließlich Regressionstest für
  gültige Füllung neben fehlerhafter Schneidkontur und ungültige Gravurkontur.
- Finale Release-EXE installiert und Oberfläche gestartet; EXE-Hashes identisch.
  Sicherung: OpenLaser-App/OpenLaser-vor-gravurauswahl-20261007-112938.exe.
  Direkt am laufenden installierten Server geprüft: 13 Gravurflächen, 2954 cut-
  und 2914 travel-Pfade, weiterhin Fehler Kontur 41 und compiled=null.
  Der Auftrag hatte bei der ersten Diagnose 12, beim letzten gespeicherten Stand
  13 Gravurkonturen. Kein Test an der echten Maschine.
- Diagnosekopie und API-Nachweis: C:/Users/joe/Videos/openlaser-gravur-diagnose-20261007/.
  Der eigene Simulator auf Port 8097 ist beendet. Joes Instanzen nicht beendet.
- Die Aufnahme des nativen Fensters lief in einen Timeout; UI-Screenshotprüfung
  war deshalb nicht möglich. Die Pfade wurden über die reale Server-API verifiziert.

Offen am Auftrag: Kontur 41 auf Ebene 0 besitzt vier Bögen und zwei mikroskopisch
kurze Linien bei ungefähr X=35 mm / Y=-54 mm. Den Import/Umriss gezielt untersuchen
und geometrisch korrigieren, bevor der Gesamtauftrag ausgeführt wird.
Die übrigen Aufgaben aus der Übergabe bleiben offen.

## Fortsetzung Codex – 2026-10-07: Gravurflächen, Konturliste und Simulation

- Gravurtab zeigt die aktuelle Auswahl als Konturliste: Anzeigen fokussiert die Kontur,
  Aus Gravur entfernen entfernt ihre Vorschau, Geometrie bearbeiten öffnet die
  Originalzeichnung als Skizze mit der betreffenden Kontur ausgewählt. Übernehmen
  speichert die Gravurzuweisung; Schließen verwirft nicht übernommene Änderungen.
- Live-Vorschau wird auf einer Draft-Kopie berechnet; Änderungen überschreiben den
  Auftrag erst beim Übernehmen. Flächenauswahl definiert Eigentümerkontur und direkte
  Löcher ausdrücklich: Ring einer 0 und Innenfläche lassen sich getrennt gravieren.
  Konturmodus graviert den Umriss. Ebenensichtbarkeit und Auswahl leeren ergänzt.
- Ein Klick wählt auch außerhalb des Gravurkontexts eine Kontur. Ganzes Teil ist eine
  eigene Aktion. Nur ausgewählte bearbeiten begrenzt echte vorbereitete Ausgabe;
  Alle bearbeiten stellt den gesamten Auftrag wieder her. Innere Konturen behalten
  ihren topologischen Kontext und ihre Abfallseite. Teilweise ausgewählte Brücken
  werden abgewiesen. Die Auswahl wird im Draft gespeichert.
- Simulation hat eigenen Tab mit Gravur/Schnitt/Alles, Tempo, Wiedergabe, Fortschritt
  und anklickbarer Bearbeitungsreihenfolge. Schnittreihenfolge bleibt vorhanden;
  Innen nach außen ausdrücklich beschriftet. Geometrische Wiedergabe verteilt Zeit
  auf Ebenen, damit ein kurzer Schnitt nach langer Gravur sichtbar bleibt; keine
  Schätzung der tatsächlichen Maschinenzeit.
- Aktueller gespeicherter Auftrag ist inzwischen eine neue Skizze mit zwei Konturen,
  nicht mehr OPH Logo: 17 Gravurabschnitte mit ca. 459734 mm gegenüber ca. 470 mm
  Schnitt. In der alten distanzbasierten 20-s-Wiedergabe erschien der Schnitt nur
  ca. 0,02 s. OPH-Kontur 41 ist weiterhin nicht geometrisch korrigiert.
- Fehlerbanner, rote Konturmarkierung und Fehlerfokus ergänzt. Teilvorschau enthält
  auch gültige Schnitte inklusive Anschnitt/Auslauf. Gesamtfehler blockiert weiterhin
  die vollständige Ausgabe. Brücken zeigen Breite/Position und werden vor Übernahme
  auf einer Kopie geometrisch geprüft.
- Neues Projekt/Neue Skizze und Bearbeiten geöffneter DXF ergänzt. Quelllinien/Bögen
  und Ebenen werden in den Skizzeneditor übernommen. Speichern einer bearbeiteten
  Zeichnung erstellt eine Kopie als neues Teil/neues Projekt; ersetzt das Original
  im vorhandenen Mehrteilauftrag noch nicht. Neue Skizze im Auftrag kann hinzugefügt
  werden. Keine Original-DXF oder vorhandenen Auftragsdaten für Tests verändert.
- Prüfung: 240 UI-Tests, Svelte-Prüfung ohne Fehler/Warnungen, UI-Build erfolgreich;
  23 Core-, 48 Prep- und 76 Server-Lib-Tests bestanden. Tests für Ring/Innenfläche,
  Auswahl und Teilvorschau, Wiedergabereihenfolge und exakte Skizzenübernahme.
  Grafische Screenshotprüfung war wegen Tool-Timeouts nicht möglich.

- Finaler Release-Build installiert; Release-/Installationshash identisch. Sicherung: C:\Users\joe\OpenLaser-App\OpenLaser-vor-simulation-20261007-125110.exe. API an isolierter aktueller Auftragskopie: eine Gravurfläche, nach Abwahl null, nur ausgewählter Schnitt genau eine Kontur; Revision unverändert. Quellzeichnungs-API liefert beide Originalkonturen. Eigener Simulator auf Port 8097 beendet. Keine Maschinenausführung.

## Fortsetzung Codex – 2026-10-07: Konturbaum und Auftragsübersicht

- Wiedergabe/Schieberegler aus Schnittreihenfolge entfernt; Canvas-Wiedergabe nur
  noch im Simulationstab. Schnittreihenfolge zeigt Konturbaum nach Ebenen mit
  Nummern, Teil/Kopie, Innen-/Außenkontur, Suchfeld und Konturfokus.
- Drag-and-drop innerhalb einer Ebene sowie Hoch/Runter und Alt+Pfeil speichern
  echte manuelle Reihenfolge. Zusammengeführte Konturen bleiben eine Einheit;
  wiederholte Gravurabschnitte erscheinen einmal. Manuelles Verschieben deaktiviert
  Innen-zuerst/Kreise-zuerst/Wärmeverteilung, damit die Eingabe wirksam wird.
  Innen nach außen kann anschließend bewusst wieder aktiviert werden. Revision
  vom Dragstart wird beim Drop geprüft; Fehleraufträge sind für Reihenfolge gesperrt.
- Auftragsübersicht bleibt bei allen Vorbereitungswerkzeugen sichtbar: Teile,
  Schnitt/Gravur/Ausschlüsse, Ebenen mit Rezept-/Ausgabestatus, Simulationzugang,
  Auswahlmodus. Ebenenreihenfolge über Pfeile tatsächlich änderbar.
- Nur Auswahl bearbeiten war bereits im vorherigen Stand eingebaut und begrenzt
  Prepared vor Kompilierung. machine.rs kompiliert genau diese Prepared-Daten.
  Run zeigt jetzt zusätzlich ausdrücklich die Begrenzung für den aktuellen Draft
  (nicht fälschlich für eine separat angezeigte ausgeführte/wiederhergestellte Arbeit).
- DXF-Skizzenbearbeitung: zufällig übereinander liegende Endpunkte unabhängiger
  Konturen werden nicht länger miteinander verbunden. Regressionstest ergänzt.
- 242 UI-Tests bestanden; Svelte-Prüfung ohne Fehler/Warnungen, UI-Build erfolgreich.
  Keine Maschinenausführung. Weitere alte Übergabeaufgaben und Ersetzen eines
  bearbeiteten Originalteils im bestehenden Mehrteilauftrag bleiben offen.

- Finale EXE installiert, Hash identisch. Vorversion: C:\Users\joe\OpenLaser-App\OpenLaser-vor-konturbaum-20261007-131219.exe. Laufende Joe-Instanzen nicht beendet: Neustart zum Laden erforderlich. Isolierter Simulator mit Auswahl nur Kontur 1 (UI-Nummer 2): kompiliertes Programm ein Schnittdurchgang mit genau source=1, vier Moves, keine Gravur, error=null. Dafür nur in Diagnosekopie Material gewählt; echter Auftrag unverändert. Simulator beendet.

## Fortsetzung Codex – 2026-10-07: Alpha-Prüfung und Skizzenbearbeitung

- Verbindliche aktuelle Einschränkungen: Handrad vorerst auslassen. GitHub erst nach stabiler und geprüfter Alpha, kein Upload/Auth-Versuch in dieser Fortsetzung.
- Skizzen können als mehrere benannte Dokumente gehalten werden. DXF/SVG sowie Text mit Schriftwahl lassen sich in die laufende Skizze einfügen. Speichern hält Abhängigkeiten/Bemaßungen am Bibliotheksteil.
- Bearbeitung vorhandener Teile ersetzt diese jetzt im bestehenden Mehrteilauftrag. Andere Teile/Kopien bleiben bestehen, unveränderte Konturen behalten ihre Einstellungen. Undo/Redo berücksichtigt die Quelldaten. Geometrieersatz nach Nesting/Mehrblattzuordnung wird derzeit klar abgewiesen. Geänderte Konturen prüfen; Brücken werden bei tatsächlichem Geometrieersatz zurückgesetzt.
- Auftragsübersicht war im vorigen Commit nur importiert, nicht eingebunden. Jetzt tatsächlich im Vorbereitungspanel sichtbar; interaktiv geprüft. Vorbereiteter manueller Trennschnitt zugänglich, kein Handrad oder Live-MCUT ergänzt.
- Scan-Anschnitt/Auslauf mit Laser aus wird als Fahrt statt Mikrobrücke angezeigt; Maschinenrecords bleiben unverändert (Regressionstest).
- Brücken: Hinzufügen auch bei ausgeschaltetem Werkzeug, Liste mit Koordinaten/Fehlernummer, Entfernung betroffener und folgender Brücken, Rücknahme im Vorschauzustand. Unterschiedliche Bearbeitungsebenen werden ausdrücklich abgewiesen. Tests für Schablonen-Innenlöcher ergänzt.
- Joes aktueller Originalauftrag hat zwei ungültige Brücken: zuerst Gravur mit Schnitt verbunden, dann überlappende Kanäle (ca. 1 mm Abstand bei 2 mm Breite). Original nicht verändert. Einzelne Brücke auf gleicher Schnittebene in isolierter Kopie geprüft: eine geschlossene Schnittkontur, beide Quellen, kein Fehler.
- Vollständige Workspace-Testmatrix bestanden mit dem bekannten CRLF-Fixture-Test ausgelassen; Offline-Fixture-Test ohne Umgebungsvariablen ignoriert. 245 UI-Tests, Svelte ohne Fehler/Warnungen, Produktionsbuild erfolgreich. Abschließend 50 Prep-/77 Server-Lib-Tests, 102 Compiler-Tests und neuer HTTP-Test bestanden.
- Grafischer Test an isoliertem Simulator 8097: Übersicht, DXF bearbeiten, Werkzeuggruppen, Schriftvorschau/Textgeometrie, Ersetzen im Auftrag und Rückgängig tatsächlich geprüft. Joes laufende Instanzen nicht beendet. Keine reale Maschine ausgeführt.
- Details und Grenzen siehe ALPHA_PRUEFUNG.md. Nicht als produktionsfreigegeben deklarieren; Maschinenabnahme bleibt erforderlich.
- Finaler Desktop-Release mit eingebetteter UI installiert; SHA256 7BCA0756D3CFDA3ACACC642B5BCD736210CF362B54D2A617B55A9FE46DEB7434 stimmt zwischen Build und Installation überein. Sicherung: C:\Users\joe\OpenLaser-App\OpenLaser-vor-alpha-20261007-1410.exe. Neustart von Joe erforderlich, seine Instanzen bleiben offen.
- Finaler Release erneut im isolierten Simulator geprüft: gleiche Ebenen gültige Brücke; unterschiedliche Ebenen ausdrücklich abgewiesen; nur Quelle 1 ergibt genau einen Vorschauschnitt bei unveränderter Revision. Gespeicherte Textskizze mit 993 Kurven nach Serverneustart wieder lesbar. Schnittsimulation erreicht 100 %, Reihenfolge sichtbar. Bildnachweise in C:\Users\joe\Videos\openlaser-alpha-pruefung-20261007.
## Codex-Fortsetzung 2026-10-07: Simulation und Reihenfolge
- Nutzerkorrektur: Gravurlogik unverändert lassen. Gemeinsame Füllberechnung für mehrere Konturen daher nicht umgesetzt.
- Simulation: bearbeitete Schnitte blau, Gravuren orange, Fahrten ohne Laser grau; Legende ergänzt. Teilfortschritt färbt nur den bereits abgefahrenen Abschnitt. Reine Anzeigeänderung, keine Änderung an Maschinenpfaden.
- Schnittreihenfolge: zusätzlicher Pointer-Draggriff (auch Touch), weiterhin innerhalb derselben Ebene. Gravur vor Schnitt / Schnitt vor Gravur ordnet echte Ebenen einschließlich Revisionsprüfung.
- Anfahrweg geprüft: draft.calibrated_features erhöht run_up auf das geschwindigkeitsabhängig notwendige Minimum; Benutzerwert kann darüber liegen. Berechnung und Gravureinstellungen unverändert.
- Prüfung: 250 UI-Tests bestanden, Svelte ohne Fehler/Warnungen, UI-Produktionsbuild erfolgreich. Neue Regressionstests unterscheiden Schnitt, Gravur, laserfreie Fahrt und Teilfortschritt.
- Noch offen: grafischer Test des Pointer-Draggriffs und der Farben, Fehlerkontur per Fehlerfokus auswählen/löschen. Frühere Raster-Reviewbefunde stehen in C:\Users\joe\Videos\OpenLaser-Claude-Review-20261007.md; nicht mit dieser Anzeigeänderung repariert.
- Präzisierung Anfahrweg: automatische Mindestlänge setzt ein zugeordnetes Rezept mit gültigem CutSpeed voraus. Ohne dieses Rezept gilt der manuell eingestellte Überlauf. Ein kurzer sichtbarer Weg allein beweist daher keinen Berechnungsfehler.

## Codex-Fortsetzung 2026-10-08: Auswahl, Brücken und unabhängige Gravuraufträge

Die jüngste Nutzeranforderung erlaubt ausdrücklich Änderungen an Gravuraufträgen und Scanarten; die vorherige Einschränkung „Gravurlogik nicht anfassen“ ist dafür überholt. GitHub weiterhin erst nach stabiler Alpha; Handrad auslassen.

- Nur ausgewählte bearbeiten: Simulation berücksichtigt only_contours. Ausgeschlossene Originalkonturen bleiben über Preview.source_contours editierbar, werden aber nicht zu Maschinenpfaden. Bedienprüfung: Quellen 0/1/2 mit zwei Brücken ergeben genau eine Schnittoperation; zwei andere Gravuren werden erst nach Alle bearbeiten wieder berücksichtigt und bleiben gespeichert.
- Werkzeugleiste zeigt tatsächliche Einstellungen aus Ebenen statt nur globale Aus-Werte (Anschnitt, Auslauf, Mikrostege, Kühlpunkte, Start). Auftragsübersicht vollständig einklappbar, ohne Mindesthöhe.
- Jeder Toolbar-Aufruf Gravieren beginnt einen neuen benannten Auftrag Gravur N. Bestehende Aufträge über die Auftragsübersicht bearbeiten. engravingFeatures ändert ausschließlich Besitzer/Füllregionen des aktuellen Auftrags. Andere Gravurebenen sind in der Auswahl geschützt; Klick ins Leere verwirft die aktuelle Auswahl nicht. Auch vollständiges Abwählen eines vorhandenen Auftrags ist speicherbar.
- Scanarten pro Gravur: gemeinsame Ebene oder Objekte einzeln. Fill.per_object ist optional; None/false bewahrt die bisherige gemeinsame Berechnung. Einzelobjekte behalten Innenlöcher. Explizite bridged regions verwenden die geschlossene vorbereitete Brückengeometrie: Kanal bleibt ungraviert.
- Brücken: erster Klick zeigt aufgebrochene erste Linie; zweite Kontur wird geometrisch gesnappt, zweiter Klick validiert und speichert direkt. Danach nächste Brücke im gleichen Modus. Kein schwebendes Anwenden-Menü, Status/Abbruch rechts. ui.picking muss state.raw bleiben: Proxy-Identitätsvergleich hatte previewPending dauerhaft blockiert. Revision wird nach Commit/Undo neu übernommen.
- Wiederhergestellte Skizze: geschlossene Konturen mit mehreren Quellen nicht mehr pauschal aus Auswahl ausschließen. Verbundene Quellen atomar wählen/entfernen. Fehlerfokus wählt betroffene Quellen zum Löschen, überschreibt aber keine aktive Gravurauswahl.
- Rastergravur verwendet bei expliziter Auswahl die Bildmaske statt vollflächiger Rahmenfüllung. Speicherlimit wird vor Rasterzellen-Allokation geprüft, auch weiße Bilder und mehrere Durchgänge.

Wesentliche Dateien: Setup.svelte, Canvas.svelte, EngravePanel.svelte, JobOverview.svelte, FillSettings.svelte, BridgesForm.svelte, ui.svelte.ts; lib/engraving-preview.ts, bridge-preview.ts, viewer-geometry.ts, features.ts, images.ts; core/features.rs, prep/lib.rs, prep/flying.rs, server/document.rs, server/draft.rs. Neue und erweiterte Regressionstests in engraving-preview, features, bridge-preview, simulation-trail und prep/tests/layers.rs. TypeScript-Bindings Fill/Preview um optionale Felder ergänzt.

Prüfungen: 256 UI-Tests in 57 Dateien bestanden, Svelte 0 Fehler/0 Warnungen, Produktionsbuild erfolgreich. Core 28 / Prep 62 / Server-Lib 83 Tests bestanden; zuletzt zusätzlich alle 8 layers-Regressionen sowie Auswahl-Servertest bestanden. Regulärer Clippy erfolgreich mit bestehenden Warnungen; -D warnings ist wegen älterer Raster-/Stilwarnungen nicht sauber. Keine Aussage über Maschinenabnahme.

Grafischer Bedienungstest ausschließlich in eigener Datenkopie auf Simulator 8097: zwei unabhängige Gravuren, Objektscan, geschützter anderer Auftrag, Leerraumklick, zweimal Brücken setzen ohne Anwenden, Nur Auswahl/Alle bearbeiten, vollständig eingeklappte Übersicht, wiederhergestellte Sketch mit verbundenen Quellen und sichtbarer Schraffur. Originalbibliothek und laufende Joe-Instanzen nicht verändert. Screenshot: C:/Users/joe/Videos/openlaser-bedienpruefung-20261007/simulation-nur-auswahl.png.

Noch offen: Hardwareabnahme; Pointer-Draggriff des Konturbaums noch nicht grafisch vollständig geprüft. Bestehende Clippy-Warnungen und frühere nicht ausdrücklich erledigte Alpha-Punkte bleiben offen. Repository enthält zahlreiche bereits vorhandene uncommittete Änderungen von Claude, insbesondere Übersetzungen; diese wurden erhalten. Keine Veröffentlichung auf GitHub.

- Finale Desktop-Version am 2026-10-08 installiert: C:\Users\joe\OpenLaser-App\OpenLaser.exe. SHA256 Build/Installation identisch: 4A05A82D6FB14AEB673E84666D9F2C9DC0CCEAFA4A9B01B91B7EDE3327655B26. Vorversion gesichert als OpenLaser-vor-auswahl-gravur-20261008.exe. Joes laufende Instanzen bleiben offen; vollständiger Neustart lädt die neue Version.
- Finalen Release auf Simulator 8097 gestartet; ausgeliefertes UI-Bundle index-DIK7SCvB.js stimmt mit Produktionsbuild überein. Bestehende Gravur nach Neustart wieder auswählbar. Alles abwählen + Fertig ergibt 0 Gravurkonturen; Rückgängig stellt 2 Gravurkonturen wieder her. Tests nur in Diagnosekopie.

## Codex-Fortsetzung 2026-10-08: Skizzenleiste, exklusive Übersicht und Abschluss Punkte 1/2/3

- Nutzerkorrektur: vorhandener Knopf Zeichnung als Skizze bearbeiten reicht; kein zweiter Sprungknopf ergänzt.
- Skizzenleiste: genau drei räumlich getrennte Gruppen Erstellen / Ändern / Abhängigkeiten, zwei Symbolreihen. Text mit Schriftauswahl und DXF/SVG importieren jetzt Symbole mit Tooltip und zugänglichem Namen; Dimension in Abhängigkeiten, Undo/Redo/Löschen/Einpassen in Ändern.
- Auftragsübersicht startet geschlossen. Rechts entweder Parameter oder offene Übersicht, keine gleichzeitig sichtbare zweite Spalte/Hälfte. Klick auf Werkzeug oder Auftrag in Übersicht schließt die Übersicht und zeigt Parameter; Parameter anzeigen wechselt zurück. Setup.svelte, JobOverview.svelte, ui.svelte.ts.
- Automatische Zusatzbeziehungen standardmäßig aus. Endpunktverbindungen und Grundformen bleiben; line Tool nimmt Tangenten/Mittelpunkt-/Auf-Kurve-Beziehungen nur bei ausdrücklich aktiviertem Zusatzmodus auf. Store-Option und Checkbox für bewusste Aktivierung. Intrinsische Beziehungen von Rechteck, Langloch und Polygon bleiben Bestandteil der Grundform.
- Offsetkonturen: horizontale/vertikale Kanten bekommen ausschließlich primäre Achsbeziehungen, keine Bindung an die Ausgangskontur. primary-relations.ts ergänzt fehlende Achsen auch beim Wiederöffnen bestehender Archive/DXF-Quellen. Regressionstest: Offsetrechteck auf 120 bemaßen, keine Schiefstellung, ursprüngliches Rechteck unverändert; ältere ungebundene Kontur ebenfalls abgesichert. Konkreter Nutzerproblemauftrag wurde nicht direkt verändert, keine Garantie für alle beliebigen Bemaßungskonflikte.
- Skizzenarchiv: Gewählten Eintrag entfernen, vorhandene Bibliotheksteile/Aufträge bleiben. Entfernte Dokumente in ol-sketch-removed-documents gespeichert und auch nach Browserneustart wiederherstellbar. Wiederherstellung behält sourcePart/jobKey. UI-Knopf Entfernen rückgängig. Archiv weiterhin browserlokal.

Punkt 1: Echter Pointer-Drag geprüft: Kontur 3 vor 2; serverseitig order.strategy.manual=[0,2,1], nach Seitenneuladen unverändert. Gravur vor Schnitt ordnet die tatsächlichen Ebenen.
Punkt 2: Neues Projekt mit 3 rechteckigen Konturen gezeichnet, aus Skizze übernommen, Material gewählt, Gravur 1 auf Innenfläche und Gravur 2 auf anderem Objekt gespeichert; über bestehenden Knopf zur Skizze, 4. Kontur ergänzt, im Auftrag ersetzt. Beide Gravuren und Material blieben erhalten. Simulation erreicht 100 %, Reihenfolge Gravur 1/Gravur 2/Schnitt 1/Schnitt 4. Zur Maschine übernimmt in Run, 5 Durchgänge kompiliert (inklusive zusätzlicher Pfadart). Nicht mit echter Maschinenausführung verwechseln: Run blieb unverbunden und Start entsprechend gesperrt.
Punkt 3: Vorige Behauptung Nesting/Mehrblech-Ersatz pauschal abgewiesen war veraltet! Claude hat das bereits in 7233cb9 implementiert. Gezielte Tests layer_moves_follow_their_contours_when_a_part_is_replaced, a_replaced_part_reaches_every_sheet und a_nested_part_is_replaced_in_place bestanden. Platzierungen, Kopien und andere Bleche bleiben; Änderung des verwendeten Stock-Umrisses weiterhin bewusst abgewiesen. Vergrößerte Teile brauchen erneute Geometrie-/Kollisionsprüfung; Brücken werden bei Quelleersatz zurückgesetzt.

Validierung: 260 UI-Tests in 57 Dateien, Svelte 0 Fehler/0 Warnungen, Produktionsbuild erfolgreich. Grafisch geprüft: drei zweireihige Werkzeuggruppen, exklusive Übersicht und automatischer Parameterwechsel, Entfernen/Neuladen/Wiederherstellen eines Skizzeneintrags. Diagnosekopie auf 8097; keine Änderung an Joes Bibliothek und keine reale Maschine. Nachweise unter C:/Users/joe/Videos/openlaser-bedienpruefung-20261007/: skizzenleiste-zweireihig.png, parameter-ohne-uebersicht.png, nur-auftragsuebersicht.png, skizzenarchiv-wiederhergestellt.png, ablauf-simulation-zwei-gravuren.png, konturbaum-drag.json, ablauf-run.json.

Noch offen: Hardwareabnahme, bestehende Clippy-/Build-Größenwarnungen, Git-Sicherung und spätere GitHub-Freigabe. Handrad weiterhin auslassen. Die oben erledigten Bedienprüfungen und Nesting-Unterstützung nicht erneut als offen führen.

- Letzte Präzisierung: statt Textknopf nun Mistkübel unmittelbar neben dem Skizzen-Dropdown, Tooltip nennt die aktuell gewählte Skizze. Entfernen/Rückgängig grafisch erneut geprüft. Finaler UI-Stand: 260 Tests bestanden; Bundle index-C1U8lJRa.js. Zusatzabhängigkeits-Schalter gilt konsistent für alle Zeichenwerkzeuge. Hauptbeziehungen werden beim Laden bestehender Quellen ergänzt, vorhandene ausdrückliche Beziehungen nicht entfernt.
- Kompilierter Run des Bedienungstests enthält beide Gravurregionen und 550 Gravurpfade, 2 Schnittkonturen, error=null. Durchlauf bis Run/Startfreigabe geprüft; keine Verbindung oder physische Ausführung ausgelöst.

- Finale Version dieser Fortsetzung installiert, Build/Installation SHA256 identisch: 368C6A39AA0C1CADE4C9C0D4DD3D2A041F5F5F13017DC3C8FF248A2792BACA31. Vorversion: C:\Users\joe\OpenLaser-App\OpenLaser-vor-skizzenleiste-20261008.exe. Finaler Release startet in Diagnosekopie mit UI index-C1U8lJRa.js; gespeicherte Ebenen Gravur 1/Gravur 2/0 nach Serverneustart vorhanden. Joe-Instanzen nicht beendet; vollständiger Neustart lädt die Änderungen.
