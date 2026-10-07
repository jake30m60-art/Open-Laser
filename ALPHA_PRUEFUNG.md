# Aktualisierung 8. Oktober 2026

260 UI-Tests bestanden; Svelte ohne Fehler/Warnungen. Draggriff mit Wiederöffnen, kompletter neuer Skizzenauftrag mit zwei Gravuren bis Run, exklusive Übersicht und Skizzenarchiv-Entfernen/Wiederherstellen geprüft. Nesting-/Mehrblech-Teilersatz ist bereits implementiert und seine gezielten Regressionen bestehen. Offset-Bemaßung und minimale automatische Beziehungen abgesichert. Keine reale Maschine ausgeführt; Hardwareabnahme bleibt offen. Details im letzten Abschnitt von UEBERGABE.md.
- Finale Version dieser Fortsetzung installiert, Build/Installation SHA256 identisch: 368C6A39AA0C1CADE4C9C0D4DD3D2A041F5F5F13017DC3C8FF248A2792BACA31. Vorversion: C:\Users\joe\OpenLaser-App\OpenLaser-vor-skizzenleiste-20261008.exe. Finaler Release startet in Diagnosekopie mit UI index-C1U8lJRa.js; gespeicherte Ebenen Gravur 1/Gravur 2/0 nach Serverneustart vorhanden. Joe-Instanzen nicht beendet; vollständiger Neustart lädt die Änderungen.

---

# Alpha-Prüfung – 7. Oktober 2026

Geprüfter Arbeitsstand: Fortsetzung nach 6933194, lokal. Kein GitHub-Upload und keine Ausführung an einer realen Maschine. Handrad zurückgestellt.

## Bestandene Prüfungen

- Gesamter Rust-Workspace mit `cargo test --workspace --locked -- --skip imported_recipes_and_machine_files`: bestanden. Der bereits dokumentierte CRLF-Fixture-Test wurde ausgelassen. `restored_library_compiles_and_executes` benötigt zusätzliche Offline-Fixtures und wurde nicht ausgeführt.
- Nach den letzten Backend-Korrekturen: 50 Prep- und 77 Server-Lib-Tests bestanden. Compiler: 102 Tests bestanden.
- Neuer HTTP-Integrationstest: Skizzen mit Abhängigkeiten speichern/lesen, ein Teil im Mehrteilauftrag ersetzen, Rückgängig und ungültige Skizzendaten abweisen: bestanden.
- 245 UI-Tests in 54 Dateien bestanden. Svelte-Prüfung: keine Fehler oder Warnungen. Produktionsbuild erfolgreich; bekannte Größenwarnung des Materialmoduls und Vite-Hinweis zu PlaneGCS bleiben.
- Matrix umfasst unter anderem Schraffur mit Innenlöchern, nur ausgewählte Konturen, gemeinsame Kanten, Nesting, Kopien, Reihenfolge, Controller-Simulator und Sicherheitsablehnungen.
- Neue Brückentests: rechteckige und kreisförmige Schablonenlöcher, beide Konturrichtungen und Ablehnung unterschiedlicher Bearbeitungsebenen.

## Interaktive Prüfung

Separater Simulator auf Port 8097 mit einer Kopie der aktuellen Bibliothek. Joes Originalauftrag und laufende Instanzen wurden nicht verändert.

- Auftragsübersicht ist im Vorbereitungstab tatsächlich sichtbar. Ein bisher nur vorhandener Import ohne eingebundene Komponente wurde korrigiert.
- Gespeicherte Brücken erscheinen einzeln mit Fehlernummer und Entfernung der betroffenen sowie nachfolgenden Brücken. Neue Brücken können auch aus dem ausgeschalteten Werkzeug begonnen werden.
- Vorhandene DXF im Editor geöffnet, Werkzeuggruppen gewechselt, Text mit Schriftauswahl und Umrissvorschau eingefügt: 985 Textkurven als Geometrie übernommen.
- Geänderte Skizze im bestehenden Auftrag gespeichert: 16 Konturen statt 2, Brücken zurückgesetzt. Rückgängig stellte die ursprünglichen 2 Konturen, Ebenen und Brücken wieder her.
- API-Vorschau des aktuellen Zweikonturauftrags mit einer einzelnen Brücke und beiden Konturen auf derselben Schnittebene: kein Fehler, eine geschlossene Kontur mit Quellen 0 und 1. Vorschau verändert die Auftragsrevision nicht.

## Grenzen und verbleibende Abnahme

- Aktueller Originalauftrag enthält zwei problematische Brücken: die erste verbindet Gravur und Schnitt; die zweite liegt nur etwa 1 mm daneben bei 2 mm Kanalbreite. Diese müssen im Auftrag bewusst neu gesetzt werden. Originaldaten wurden nicht automatisch verändert.
- Geometrieersatz ist vor Nesting beziehungsweise Mehrblattzuordnung möglich; danach wird er ausdrücklich abgewiesen. Geänderte Konturen brauchen eine Kontrolle ihrer Bearbeitungseinstellungen. Unveränderte Konturen und andere Teile behalten ihre Einstellungen; Rückgängig/Weiterherstellen bleibt möglich.
- Mehrere Skizzen werden im Browser gespeichert; beim Übernehmen bleibt das vollständige Skizzenmodell mit Abhängigkeiten am Bibliotheksteil erhalten. Andere Geräte bekommen nicht automatisch das lokale Skizzenarchiv.
- Keine reale Laser-, Gas-, Fokus-, Achsen- oder Materialabnahme. Die Softwarematrix ersetzt diese Maschinenprüfung nicht. Alpha ist deshalb nicht als produktionsfreigegeben bezeichnet.
- GitHub erst nach stabiler Alpha und ausdrücklicher Freigabe; Handrad nicht bearbeitet.

Finaler installierter Desktop-Release: SHA256 7BCA0756D3CFDA3ACACC642B5BCD736210CF362B54D2A617B55A9FE46DEB7434. Nach Neustart: Skizze mit 993 Kurven lesbar; Auswahlvorschau exakt Quelle 1, Revision unverändert; ebenenübergreifende Brücke mit Fehler abgewiesen. Grafische Schnittsimulation im finalen Release bis 100 % geprüft. Vorversion gesichert, Nutzerinstanzen nicht beendet.
