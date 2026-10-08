@echo off
rem Baut OpenLaser neu und installiert es nach %USERPROFILE%\OpenLaser-App (alte Version nach alt\).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0update-windows.ps1" -Build %*
pause
