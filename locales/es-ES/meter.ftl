# Audoxidy (Teksilo UI): The VU meter and its settings popup.
# Spanish: the Iced UI's strings, verbatim except the corrections listed in
# docs/teksilo-port-quirk-fixes.md (fix 15). A message the Iced UI does not
# have is marked `# Not in the Iced UI: <why>.`

meter-popup-title = Configuración del Meter
meter-show = Mostrar medidor
meter-hold-time = Tiempo de hold del pico:
meter-rms-window = Ventana RMS:
meter-ms-mode = Modo M/S
meter-infinite-hold = Hold infinito
# Not in the Iced UI: its popup has no reset button (the `MeterResetPeak` message is never sent).
meter-reset-peak = Restablecer pico
# Not in the Iced UI: the stepper's accessible name; Iced shows only the label with its colon.
meter-hold-time-name = Tiempo de hold del pico
# Not in the Iced UI: the stepper's accessible name; Iced shows only the label with its colon.
meter-rms-window-name = Ventana RMS
# Not in the Iced UI: the slot has no accessible name or keyboard route; this one opens the popup.
meter-slot-name = Ajustes del medidor
# Not in the Iced UI: Iced logs a failed save and shows nothing.
meter-save-failed = No se pudo guardar el ajuste del medidor
