# Audoxidy (Teksilo UI): The value controls shared by panels: sliders, steppers, the VU meter.
# Spanish: the Iced UI's strings, verbatim except the corrections listed in
# docs/teksilo-port-quirk-fixes.md (fix 15). A message the Iced UI does not
# have is marked `# Not in the Iced UI: <why>.`

## Sliders and steppers
# Not in the Iced UI: the accessible action doing what a secondary click does, worded after the equalizer hint (audio_center.rs:2988).
controls-reset-default = Restablecer al valor por defecto
controls-eq-value = { $value } dB

## VU meter
controls-meter-left = L
controls-meter-right = R
controls-meter-mid = M
controls-meter-side = S
controls-meter-peak = Pk
controls-meter-rms = Rms
controls-meter-crest = Crest
# Not in the Iced UI: the meter's accessible name ("Medidor", as in "Mostrar medidor", widgets.rs:5241).
controls-meter-name = Medidor VU
# Not in the Iced UI: a bar level in the meter's accessible value.
controls-meter-db = { $value } dB
# Not in the Iced UI: a silent bar in the meter's accessible value.
controls-meter-silent = silencio
